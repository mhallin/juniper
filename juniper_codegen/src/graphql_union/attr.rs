use std::{mem, ops::Deref as _};

use proc_macro2::{Span, TokenStream};
use quote::{quote, ToTokens as _};
use syn::{self, ext::IdentExt as _, parse_quote, spanned::Spanned as _};

use crate::{
    result::GraphQLScope,
    util::{path_eq_single, span_container::SpanContainer, to_pascal_case, unparenthesize, Mode},
};

use super::{UnionDefinition, UnionMeta, UnionVariantDefinition, UnionVariantMeta};

const SCOPE: GraphQLScope = GraphQLScope::AttrUnion;

/// Returns name of the `proc_macro_attribute` for deriving `GraphQLUnion` implementation depending
/// on the provided `mode`.
fn attr_path(mode: Mode) -> &'static str {
    match mode {
        Mode::Public => "graphql_union",
        Mode::Internal => "graphql_union_internal",
    }
}

/// Expands `#[graphql_union]`/`#[graphql_union_internal]` macros into generated code.
pub fn expand(attr_args: TokenStream, body: TokenStream, mode: Mode) -> syn::Result<TokenStream> {
    let attr_path = attr_path(mode);

    let mut ast = syn::parse2::<syn::ItemTrait>(body).map_err(|_| {
        syn::Error::new(
            Span::call_site(),
            format!(
                "#[{}] attribute is applicable to trait definitions only",
                attr_path,
            ),
        )
    })?;

    let mut trait_attrs = Vec::with_capacity(ast.attrs.len() + 1);
    trait_attrs.push({
        let attr_path = syn::Ident::new(attr_path, Span::call_site());
        parse_quote! { #[#attr_path(#attr_args)] }
    });
    trait_attrs.extend_from_slice(&ast.attrs);

    // Remove repeated attributes from the definition, to omit duplicate expansion.
    ast.attrs = ast
        .attrs
        .into_iter()
        .filter_map(|attr| {
            if path_eq_single(&attr.path, attr_path) {
                None
            } else {
                Some(attr)
            }
        })
        .collect();

    let meta = UnionMeta::from_attrs(attr_path, &trait_attrs)?;

    let trait_span = ast.span();
    let trait_ident = &ast.ident;

    let name = meta
        .name
        .clone()
        .map(SpanContainer::into_inner)
        .unwrap_or_else(|| to_pascal_case(&trait_ident.unraw().to_string()));
    if matches!(mode, Mode::Public) && name.starts_with("__") {
        SCOPE.no_double_underscore(
            meta.name
                .as_ref()
                .map(SpanContainer::span_ident)
                .unwrap_or_else(|| trait_ident.span()),
        );
    }

    let mut variants: Vec<_> = ast
        .items
        .iter_mut()
        .filter_map(|i| match i {
            syn::TraitItem::Method(m) => {
                parse_variant_from_trait_method(m, trait_ident, &meta, mode)
            }
            _ => None,
        })
        .collect();

    proc_macro_error::abort_if_dirty();

    if !meta.custom_resolvers.is_empty() {
        let crate_path = mode.crate_path();
        // TODO: refactor into separate function
        for (ty, rslvr) in meta.custom_resolvers {
            let span = rslvr.span_joined();

            let resolver_fn = rslvr.into_inner();
            let resolver_code = parse_quote! {
                #resolver_fn(self, #crate_path::FromContext::from(context))
            };
            // Doing this may be quite an expensive, because resolving may contain some heavy
            // computation, so we're preforming it twice. Unfortunately, we have no other options
            // here, until the `juniper::GraphQLType` itself will allow to do it in some cleverer
            // way.
            let resolver_check = parse_quote! {
                ({ #resolver_code } as ::std::option::Option<&#ty>).is_some()
            };

            // TODO: We may not check here for existence, as we do the duplication check when
            //       parsing methods.
            if let Some(var) = variants.iter_mut().find(|v| v.ty == ty) {
                var.resolver_code = resolver_code;
                var.resolver_check = resolver_check;
                var.span = span;
            } else {
                variants.push(UnionVariantDefinition {
                    ty,
                    resolver_code,
                    resolver_check,
                    enum_path: None,
                    span,
                })
            }
        }
    }
    if variants.is_empty() {
        SCOPE.custom(trait_span, "expects at least one union variant");
    }

    // NOTICE: This is not an optimal implementation, as it's possible to bypass this check by using
    // a full qualified path instead (`crate::Test` vs `Test`). Since this requirement is mandatory,
    // the `std::convert::Into<T>` implementation is used to enforce this requirement. However, due
    // to the bad error message this implementation should stay and provide guidance.
    let all_variants_different = {
        let mut types: Vec<_> = variants.iter().map(|var| &var.ty).collect();
        types.dedup();
        types.len() == variants.len()
    };
    if !all_variants_different {
        SCOPE.custom(trait_span, "each union variant must have a different type");
    }

    proc_macro_error::abort_if_dirty();

    let generated_code = UnionDefinition {
        name,
        ty: parse_quote! { #trait_ident },
        is_trait_object: true,
        description: meta.description.map(SpanContainer::into_inner),
        context: meta.context.map(SpanContainer::into_inner),
        scalar: meta.scalar.map(SpanContainer::into_inner),
        generics: ast.generics.clone(),
        variants,
        span: trait_span,
        mode,
    };

    Ok(quote! {
        #ast

        #generated_code
    })
}

fn parse_variant_from_trait_method(
    method: &mut syn::TraitItemMethod,
    trait_ident: &syn::Ident,
    trait_meta: &UnionMeta,
    mode: Mode,
) -> Option<UnionVariantDefinition> {
    let attr_path = attr_path(mode);
    let method_attrs = method.attrs.clone();

    // Remove repeated attributes from the method, to omit incorrect expansion.
    method.attrs = mem::take(&mut method.attrs)
        .into_iter()
        .filter_map(|attr| {
            if path_eq_single(&attr.path, attr_path) {
                None
            } else {
                Some(attr)
            }
        })
        .collect();

    let meta = UnionVariantMeta::from_attrs(attr_path, &method_attrs)
        .map_err(|e| proc_macro_error::emit_error!(e))
        .ok()?;

    if let Some(rslvr) = meta.custom_resolver {
        SCOPE.custom(
            rslvr.span_ident(),
            format!(
                "cannot use #[{0}(with = ...)] attribute on a trait method, instead use \
                 #[{0}(ignore)] on the method with #[{0}(on ... = ...)] on the trait itself",
                attr_path,
            ),
        )
    }
    if meta.ignore.is_some() {
        return None;
    }

    let method_span = method.sig.span();
    let method_ident = &method.sig.ident;

    let ty = parse_trait_method_output_type(&method.sig)
        .map_err(|span| {
            SCOPE.custom(
                span,
                "trait method return type can be `Option<&VariantType>` only",
            )
        })
        .ok()?;
    let accepts_context = parse_trait_method_input_args(&method.sig)
        .map_err(|span| {
            SCOPE.custom(
                span,
                "trait method can accept `&self` and optionally `&Context` only",
            )
        })
        .ok()?;
    if let Some(is_async) = &method.sig.asyncness {
        SCOPE.custom(
            is_async.span(),
            "async union variants resolvers are not supported yet",
        );
        return None;
    }

    let resolver_code = {
        if let Some(other) = trait_meta.custom_resolvers.get(&ty) {
            SCOPE.custom(
                method_span,
                format!(
                    "trait method `{}` conflicts with the custom resolver `{}` declared on the \
                     trait to resolve the variant type `{}`, use `#[{}(ignore)]` attribute to \
                     ignore this trait method for union variants resolution",
                    method_ident,
                    other.to_token_stream(),
                    ty.to_token_stream(),
                    attr_path,
                ),
            );
        }

        if accepts_context {
            let crate_path = mode.crate_path();

            parse_quote! {
                #trait_ident::#method_ident(self, #crate_path::FromContext::from(context))
            }
        } else {
            parse_quote! {
                #trait_ident::#method_ident(self)
            }
        }
    };

    // Doing this may be quite an expensive, because resolving may contain some heavy
    // computation, so we're preforming it twice. Unfortunately, we have no other options
    // here, until the `juniper::GraphQLType` itself will allow to do it in some cleverer
    // way.
    let resolver_check = parse_quote! {
        ({ #resolver_code } as ::std::option::Option<&#ty>).is_some()
    };

    Some(UnionVariantDefinition {
        ty,
        resolver_code,
        resolver_check,
        enum_path: None,
        span: method_span,
    })
}

/// Parses type of [GraphQL union][1] variant from the return type of trait method.
///
/// If return type is invalid, then returns the [`Span`] to display the corresponding error at.
///
/// [1]: https://spec.graphql.org/June2018/#sec-Unions
fn parse_trait_method_output_type(sig: &syn::Signature) -> Result<syn::Type, Span> {
    let ret_ty = match &sig.output {
        syn::ReturnType::Type(_, ty) => ty.deref(),
        _ => return Err(sig.span()),
    };

    let path = match unparenthesize(ret_ty) {
        syn::Type::Path(syn::TypePath { qself: None, path }) => path,
        _ => return Err(ret_ty.span()),
    };

    let (ident, args) = match path.segments.last() {
        Some(syn::PathSegment {
            ident,
            arguments: syn::PathArguments::AngleBracketed(generic),
        }) => (ident, &generic.args),
        _ => return Err(ret_ty.span()),
    };

    if ident.unraw() != "Option" {
        return Err(ret_ty.span());
    }

    if args.len() != 1 {
        return Err(ret_ty.span());
    }
    let var_ty = match args.first() {
        Some(syn::GenericArgument::Type(inner_ty)) => match unparenthesize(inner_ty) {
            syn::Type::Reference(inner_ty) => {
                if inner_ty.mutability.is_some() {
                    return Err(inner_ty.span());
                }
                unparenthesize(inner_ty.elem.deref()).clone()
            }
            _ => return Err(ret_ty.span()),
        },
        _ => return Err(ret_ty.span()),
    };
    Ok(var_ty)
}

/// Parses trait method input arguments and validates them to be acceptable for resolving into
/// [GraphQL union][1] variant type. Indicates whether method accepts context or not.
///
/// If input arguments are invalid, then returns the [`Span`] to display the corresponding error at.
///
/// [1]: https://spec.graphql.org/June2018/#sec-Unions
fn parse_trait_method_input_args(sig: &syn::Signature) -> Result<bool, Span> {
    match sig.receiver() {
        Some(syn::FnArg::Receiver(rcv)) => {
            if rcv.reference.is_none() || rcv.mutability.is_some() {
                return Err(rcv.span());
            }
        }
        _ => return Err(sig.span()),
    }

    if sig.inputs.len() > 2 {
        return Err(sig.inputs.span());
    }

    let second_arg_ty = match sig.inputs.iter().skip(1).next() {
        Some(syn::FnArg::Typed(arg)) => arg.ty.deref(),
        None => return Ok(false),
        _ => return Err(sig.inputs.span()),
    };
    match unparenthesize(second_arg_ty) {
        syn::Type::Reference(ref_ty) => {
            if ref_ty.mutability.is_some() {
                return Err(ref_ty.span());
            }
        }
        ty => return Err(ty.span()),
    }

    Ok(true)
}
