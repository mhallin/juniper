//! Code generation for [GraphQL union][1].
//!
//! [1]: https://spec.graphql.org/June2018/#sec-Unions

pub mod attr;
pub mod derive;

use std::collections::HashMap;

use proc_macro2::{Span, TokenStream};
use quote::{quote, ToTokens, TokenStreamExt as _};
use syn::{
    parse::{Parse, ParseStream},
    parse_quote,
    spanned::Spanned as _,
    token,
};

use crate::{
    common::parse::{
        attr::{err, OptionExt as _},
        ParseBufferExt as _,
    },
    util::{filter_attrs, get_doc_comment, span_container::SpanContainer},
};

/// Helper alias for the type of [`UnionMeta::external_resolvers`] field.
type UnionMetaResolvers = HashMap<syn::Type, SpanContainer<syn::ExprPath>>;

/// Available metadata (arguments) behind `#[graphql]` (or `#[graphql_union]`) attribute when
/// generating code for [GraphQL union][1] type.
///
/// [1]: https://spec.graphql.org/June2018/#sec-Unions
#[derive(Debug, Default)]
struct UnionMeta {
    /// Explicitly specified name of [GraphQL union][1] type.
    ///
    /// If absent, then Rust type name is used by default.
    ///
    /// [1]: https://spec.graphql.org/June2018/#sec-Unions
    pub name: Option<SpanContainer<String>>,

    /// Explicitly specified [description][2] of [GraphQL union][1] type.
    ///
    /// If absent, then Rust doc comment is used as [description][2], if any.
    ///
    /// [1]: https://spec.graphql.org/June2018/#sec-Unions
    /// [2]: https://spec.graphql.org/June2018/#sec-Descriptions
    pub description: Option<SpanContainer<String>>,

    /// Explicitly specified type of `juniper::Context` to use for resolving this [GraphQL union][1]
    /// type with.
    ///
    /// If absent, then unit type `()` is assumed as type of `juniper::Context`.
    ///
    /// [1]: https://spec.graphql.org/June2018/#sec-Unions
    pub context: Option<SpanContainer<syn::Type>>,

    /// Explicitly specified type of `juniper::ScalarValue` to use for resolving this
    /// [GraphQL union][1] type with.
    ///
    /// If absent, then generated code will be generic over any `juniper::ScalarValue` type, which,
    /// in turn, requires all [union][1] variants to be generic over any `juniper::ScalarValue` type
    /// too. That's why this type should be specified only if one of the variants implements
    /// `juniper::GraphQLType` in a non-generic way over `juniper::ScalarValue` type.
    ///
    /// [1]: https://spec.graphql.org/June2018/#sec-Unions
    pub scalar: Option<SpanContainer<syn::Type>>,

    /// Explicitly specified external resolver functions for [GraphQL union][1] variants.
    ///
    /// If absent, then macro will try to auto-infer all the possible variants from the type
    /// declaration, if possible. That's why specifying an external resolver function has sense,
    /// when some custom [union][1] variant resolving logic is involved, or variants cannot be
    /// inferred.
    ///
    /// [1]: https://spec.graphql.org/June2018/#sec-Unions
    pub external_resolvers: UnionMetaResolvers,

    /// Indicator whether the generated code is intended to be used only inside the `juniper`
    /// library.
    pub is_internal: bool,
}

impl Parse for UnionMeta {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut output = Self::default();

        while !input.is_empty() {
            let ident = input.parse::<syn::Ident>()?;
            match ident.to_string().as_str() {
                "name" => {
                    input.parse::<token::Eq>()?;
                    let name = input.parse::<syn::LitStr>()?;
                    output
                        .name
                        .replace(SpanContainer::new(
                            ident.span(),
                            Some(name.span()),
                            name.value(),
                        ))
                        .none_or_else(|_| err::dup_arg(&ident))?
                }
                "desc" | "description" => {
                    input.parse::<token::Eq>()?;
                    let desc = input.parse::<syn::LitStr>()?;
                    output
                        .description
                        .replace(SpanContainer::new(
                            ident.span(),
                            Some(desc.span()),
                            desc.value(),
                        ))
                        .none_or_else(|_| err::dup_arg(&ident))?
                }
                "ctx" | "context" | "Context" => {
                    input.parse::<token::Eq>()?;
                    let ctx = input.parse::<syn::Type>()?;
                    output
                        .context
                        .replace(SpanContainer::new(ident.span(), Some(ctx.span()), ctx))
                        .none_or_else(|_| err::dup_arg(&ident))?
                }
                "scalar" | "Scalar" | "ScalarValue" => {
                    input.parse::<token::Eq>()?;
                    let scl = input.parse::<syn::Type>()?;
                    output
                        .scalar
                        .replace(SpanContainer::new(ident.span(), Some(scl.span()), scl))
                        .none_or_else(|_| err::dup_arg(&ident))?
                }
                "on" => {
                    let ty = input.parse::<syn::Type>()?;
                    input.parse::<token::Eq>()?;
                    let rslvr = input.parse::<syn::ExprPath>()?;
                    let rslvr_spanned = SpanContainer::new(ident.span(), Some(ty.span()), rslvr);
                    let rslvr_span = rslvr_spanned.span_joined();
                    output
                        .external_resolvers
                        .insert(ty, rslvr_spanned)
                        .none_or_else(|_| err::dup_arg(rslvr_span))?
                }
                "internal" => {
                    output.is_internal = true;
                }
                name => {
                    return Err(err::unknown_arg(&ident, name));
                }
            }
            input.try_parse::<token::Comma>()?;
        }

        Ok(output)
    }
}

impl UnionMeta {
    /// Tries to merge two [`UnionMeta`]s into a single one, reporting about duplicates, if any.
    fn try_merge(self, mut another: Self) -> syn::Result<Self> {
        Ok(Self {
            name: try_merge_opt!(name: self, another),
            description: try_merge_opt!(description: self, another),
            context: try_merge_opt!(context: self, another),
            scalar: try_merge_opt!(scalar: self, another),
            external_resolvers: try_merge_hashmap!(
                external_resolvers: self, another => span_joined
            ),
            is_internal: self.is_internal || another.is_internal,
        })
    }

    /// Parses [`UnionMeta`] from the given multiple `name`d [`syn::Attribute`]s placed on a type
    /// definition.
    pub fn from_attrs(name: &str, attrs: &[syn::Attribute]) -> syn::Result<Self> {
        let mut meta = filter_attrs(name, attrs)
            .map(|attr| attr.parse_args())
            .try_fold(Self::default(), |prev, curr| prev.try_merge(curr?))?;

        if meta.description.is_none() {
            meta.description = get_doc_comment(attrs);
        }

        Ok(meta)
    }
}

/// Available metadata (arguments) behind `#[graphql]` (or `#[graphql_union]`) attribute when
/// generating code for [GraphQL union][1]'s variant.
///
/// [1]: https://spec.graphql.org/June2018/#sec-Unions
#[derive(Debug, Default)]
struct UnionVariantMeta {
    /// Explicitly specified marker for the variant/field being ignored and not included into
    /// [GraphQL union][1].
    ///
    /// [1]: https://spec.graphql.org/June2018/#sec-Unions
    pub ignore: Option<SpanContainer<syn::Ident>>,

    /// Explicitly specified external resolver function for this [GraphQL union][1] variant.
    ///
    /// If absent, then macro will generate the code which just returns the variant inner value.
    /// Usually, specifying an external resolver function has sense, when some custom resolving
    /// logic is involved.
    ///
    /// [1]: https://spec.graphql.org/June2018/#sec-Unions
    pub external_resolver: Option<SpanContainer<syn::ExprPath>>,
}

impl Parse for UnionVariantMeta {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut output = Self::default();

        while !input.is_empty() {
            let ident = input.parse::<syn::Ident>()?;
            match ident.to_string().as_str() {
                "ignore" | "skip" => output
                    .ignore
                    .replace(SpanContainer::new(ident.span(), None, ident.clone()))
                    .none_or_else(|_| err::dup_arg(&ident))?,
                "with" => {
                    input.parse::<token::Eq>()?;
                    let rslvr = input.parse::<syn::ExprPath>()?;
                    output
                        .external_resolver
                        .replace(SpanContainer::new(ident.span(), Some(rslvr.span()), rslvr))
                        .none_or_else(|_| err::dup_arg(&ident))?
                }
                name => {
                    return Err(err::unknown_arg(&ident, name));
                }
            }
            input.try_parse::<token::Comma>()?;
        }

        Ok(output)
    }
}

impl UnionVariantMeta {
    /// Tries to merge two [`UnionVariantMeta`]s into a single one, reporting about duplicates, if
    /// any.
    fn try_merge(self, mut another: Self) -> syn::Result<Self> {
        Ok(Self {
            ignore: try_merge_opt!(ignore: self, another),
            external_resolver: try_merge_opt!(external_resolver: self, another),
        })
    }

    /// Parses [`UnionVariantMeta`] from the given multiple `name`d [`syn::Attribute`]s placed on a
    /// variant/field/method definition.
    pub fn from_attrs(name: &str, attrs: &[syn::Attribute]) -> syn::Result<Self> {
        filter_attrs(name, attrs)
            .map(|attr| attr.parse_args())
            .try_fold(Self::default(), |prev, curr| prev.try_merge(curr?))
    }
}

/// Definition of [GraphQL union][1] variant for code generation.
///
/// [1]: https://spec.graphql.org/June2018/#sec-Unions
struct UnionVariantDefinition {
    /// Rust type that this [GraphQL union][1] variant resolves into.
    ///
    /// [1]: https://spec.graphql.org/June2018/#sec-Unions
    pub ty: syn::Type,

    /// Rust code for value resolution of this [GraphQL union][1] variant.
    ///
    /// [1]: https://spec.graphql.org/June2018/#sec-Unions
    pub resolver_code: syn::Expr,

    /// Rust code for checking whether [GraphQL union][1] should be resolved into this variant.
    ///
    /// [1]: https://spec.graphql.org/June2018/#sec-Unions
    pub resolver_check: syn::Expr,

    /// Rust enum variant path that this [GraphQL union][1] variant is associated with.
    ///
    /// It's available only when code generation happens for Rust enums.
    ///
    /// [1]: https://spec.graphql.org/June2018/#sec-Unions
    pub enum_path: Option<TokenStream>,

    /// Rust type of `juniper::Context` that this [GraphQL union][1] variant requires for
    /// resolution.
    ///
    /// It's available only when code generation happens for Rust traits and a trait method contains
    /// context argument.
    ///
    /// [1]: https://spec.graphql.org/June2018/#sec-Unions
    pub context_ty: Option<syn::Type>,

    /// [`Span`] that points to the Rust source code which defines this [GraphQL union][1] variant.
    ///
    /// [1]: https://spec.graphql.org/June2018/#sec-Unions
    pub span: Span,
}

/// Definition of [GraphQL union][1] for code generation.
///
/// [1]: https://spec.graphql.org/June2018/#sec-Unions
struct UnionDefinition {
    /// Name of this [GraphQL union][1] in GraphQL schema.
    ///
    /// [1]: https://spec.graphql.org/June2018/#sec-Unions
    pub name: String,

    /// Rust type that this [GraphQL union][1] is represented with.
    ///
    /// [1]: https://spec.graphql.org/June2018/#sec-Unions
    pub ty: syn::Type,

    /// Generics of the Rust type that this [GraphQL union][1] is implemented for.
    ///
    /// [1]: https://spec.graphql.org/June2018/#sec-Unions
    pub generics: syn::Generics,

    /// Indicator whether code should be generated for a trait object, rather than for a regular
    /// Rust type.
    pub is_trait_object: bool,

    /// Description of this [GraphQL union][1] to put into GraphQL schema.
    ///
    /// [1]: https://spec.graphql.org/June2018/#sec-Unions
    pub description: Option<String>,

    /// Rust type of `juniper::Context` to generate `juniper::GraphQLType` implementation with
    /// for this [GraphQL union][1].
    ///
    /// If [`None`] then generated code will use unit type `()` as `juniper::Context`.
    ///
    /// [1]: https://spec.graphql.org/June2018/#sec-Unions
    pub context: Option<syn::Type>,

    /// Rust type of `juniper::ScalarValue` to generate `juniper::GraphQLType` implementation with
    /// for this [GraphQL union][1].
    ///
    /// If [`None`] then generated code will be generic over any `juniper::ScalarValue` type, which,
    /// in turn, requires all [union][1] variants to be generic over any `juniper::ScalarValue` type
    /// too. That's why this type should be specified only if one of the variants implements
    /// `juniper::GraphQLType` in a non-generic way over `juniper::ScalarValue` type.
    ///
    /// [1]: https://spec.graphql.org/June2018/#sec-Unions
    pub scalar: Option<syn::Type>,

    /// Variants definitions of this [GraphQL union][1].
    ///
    /// [1]: https://spec.graphql.org/June2018/#sec-Unions
    pub variants: Vec<UnionVariantDefinition>,
}

impl ToTokens for UnionDefinition {
    fn to_tokens(&self, into: &mut TokenStream) {
        let name = &self.name;
        let ty = &self.ty;

        let context = self
            .context
            .as_ref()
            .map(|ctx| quote! { #ctx })
            .unwrap_or_else(|| quote! { () });

        let scalar = self
            .scalar
            .as_ref()
            .map(|scl| quote! { #scl })
            .unwrap_or_else(|| quote! { __S });

        let description = self
            .description
            .as_ref()
            .map(|desc| quote! { .description(#desc) });

        let var_types: Vec<_> = self.variants.iter().map(|var| &var.ty).collect();

        let all_variants_unique = if var_types.len() > 1 {
            Some(quote! { ::juniper::sa::assert_type_ne_all!(#(#var_types),*); })
        } else {
            None
        };

        let match_names = self.variants.iter().map(|var| {
            let var_ty = &var.ty;
            let var_check = &var.resolver_check;
            quote! {
                if #var_check {
                    return <#var_ty as ::juniper::GraphQLType<#scalar>>::name(info)
                        .unwrap().to_string();
                }
            }
        });

        let match_resolves: Vec<_> = self.variants.iter().map(|var| &var.resolver_code).collect();
        let resolve_into_type = self.variants.iter().zip(match_resolves.iter()).map(|(var, expr)| {
            let var_ty = &var.ty;

            let get_name = quote! { (<#var_ty as ::juniper::GraphQLType<#scalar>>::name(info)) };
            quote! {
                if type_name == #get_name.unwrap() {
                    return ::juniper::IntoResolvable::into(
                        { #expr },
                        executor.context(),
                    )
                    .and_then(|res| match res {
                        Some((ctx, r)) => executor.replaced_context(ctx).resolve_with_ctx(info, &r),
                        None => Ok(::juniper::Value::null()),
                    });
                }
            }
        });
        let resolve_into_type_async =
            self.variants
                .iter()
                .zip(match_resolves.iter())
                .map(|(var, expr)| {
                    let var_ty = &var.ty;

                    let get_name = quote! {
                        (<#var_ty as ::juniper::GraphQLType<#scalar>>::name(info))
                    };
                    quote! {
                        if type_name == #get_name.unwrap() {
                            let res = ::juniper::IntoResolvable::into(
                                { #expr },
                                executor.context(),
                            );
                            return Box::pin(async move {
                                match res? {
                                    Some((ctx, r)) => {
                                        let subexec = executor.replaced_context(ctx);
                                        subexec.resolve_with_ctx_async(info, &r).await
                                    },
                                    None => Ok(::juniper::Value::null()),
                                }
                            });
                        }
                    }
                });

        let (_, ty_generics, _) = self.generics.split_for_impl();

        let mut ext_generics = self.generics.clone();
        if self.is_trait_object {
            ext_generics.params.push(parse_quote! { '__obj });
        }
        if self.scalar.is_none() {
            ext_generics.params.push(parse_quote! { #scalar });
            ext_generics
                .make_where_clause()
                .predicates
                .push(parse_quote! { #scalar: ::juniper::ScalarValue });
        }
        let (ext_impl_generics, _, where_clause) = ext_generics.split_for_impl();

        let mut where_async = where_clause
            .cloned()
            .unwrap_or_else(|| parse_quote! { where });
        where_async.predicates.push(parse_quote! { Self: Sync });
        if self.scalar.is_none() {
            where_async
                .predicates
                .push(parse_quote! { #scalar: Send + Sync });
        }

        let mut ty_full = quote! { #ty#ty_generics };
        if self.is_trait_object {
            ty_full = quote! { dyn #ty_full + '__obj + Send + Sync };
        }

        let type_impl = quote! {
            #[automatically_derived]
            impl#ext_impl_generics ::juniper::GraphQLType<#scalar> for #ty_full
                #where_clause
            {
                fn name(_ : &Self::TypeInfo) -> Option<&'static str> {
                    Some(#name)
                }

                fn meta<'r>(
                    info: &Self::TypeInfo,
                    registry: &mut ::juniper::Registry<'r, #scalar>
                ) -> ::juniper::meta::MetaType<'r, #scalar>
                where #scalar: 'r,
                {
                    let types = [
                        #( registry.get_type::<#var_types>(info), )*
                    ];
                    registry.build_union_type::<#ty_full>(info, &types)
                    #description
                    .into_meta()
                }
            }
        };

        let value_impl = quote! {
            #[automatically_derived]
            impl#ext_impl_generics ::juniper::GraphQLValue<#scalar> for #ty_full
                #where_clause
            {
                type Context = #context;
                type TypeInfo = ();

                fn type_name<'__i>(&self, info: &'__i Self::TypeInfo) -> Option<&'__i str> {
                    <Self as ::juniper::GraphQLType<#scalar>>::name(info)
                }

                fn concrete_type_name(
                    &self,
                    context: &Self::Context,
                    info: &Self::TypeInfo,
                ) -> String {
                    #( #match_names )*
                    panic!(
                        "GraphQL union {} cannot be resolved into any of its variants in its \
                         current state",
                        #name,
                    );
                }

                fn resolve_into_type(
                    &self,
                    info: &Self::TypeInfo,
                    type_name: &str,
                    _: Option<&[::juniper::Selection<#scalar>]>,
                    executor: &::juniper::Executor<Self::Context, #scalar>,
                ) -> ::juniper::ExecutionResult<#scalar> {
                    let context = executor.context();
                    #( #resolve_into_type )*
                    panic!(
                        "Concrete type {} is not handled by instance resolvers on GraphQL union {}",
                        type_name, #name,
                    );
                }
            }
        };

        let value_async_impl = quote! {
            #[automatically_derived]
            impl#ext_impl_generics ::juniper::GraphQLValueAsync<#scalar> for #ty_full
                #where_async
            {
                fn resolve_into_type_async<'b>(
                    &'b self,
                    info: &'b Self::TypeInfo,
                    type_name: &str,
                    _: Option<&'b [::juniper::Selection<'b, #scalar>]>,
                    executor: &'b ::juniper::Executor<'b, 'b, Self::Context, #scalar>
                ) -> ::juniper::LocalBoxFuture<'b, ::juniper::ExecutionResult<#scalar>> {
                    let context = executor.context();
                    #( #resolve_into_type_async )*
                    panic!(
                        "Concrete type {} is not handled by instance resolvers on GraphQL union {}",
                        type_name, #name,
                    );
                }
            }
        };

        let output_type_impl = quote! {
            #[automatically_derived]
            impl#ext_impl_generics ::juniper::marker::IsOutputType<#scalar> for #ty_full
                #where_clause
            {
                fn mark() {
                    #( <#var_types as ::juniper::marker::IsOutputType<#scalar>>::mark(); )*
                }
            }
        };

        let union_impl = quote! {
            #[automatically_derived]
            impl#ext_impl_generics ::juniper::marker::GraphQLUnion<#scalar> for #ty_full
                #where_clause
            {
                fn mark() {
                    #all_variants_unique

                    #( <#var_types as ::juniper::marker::GraphQLObjectType<#scalar>>::mark(); )*
                }
            }
        };

        into.append_all(&[
            union_impl,
            output_type_impl,
            type_impl,
            value_impl,
            value_async_impl,
        ]);
    }
}

/// Emerges [`UnionMeta::external_resolvers`] into the given [GraphQL union][1] `variants`.
///
/// If duplication happens, then resolving code is overwritten with the one from
/// `external_resolvers`.
///
/// [1]: https://spec.graphql.org/June2018/#sec-Unions
fn emerge_union_variants_from_meta(
    variants: &mut Vec<UnionVariantDefinition>,
    external_resolvers: UnionMetaResolvers,
) {
    if external_resolvers.is_empty() {
        return;
    }

    for (ty, rslvr) in external_resolvers {
        let span = rslvr.span_joined();

        let resolver_fn = rslvr.into_inner();
        let resolver_code = parse_quote! {
            #resolver_fn(self, ::juniper::FromContext::from(context))
        };
        // Doing this may be quite an expensive, because resolving may contain some heavy
        // computation, so we're preforming it twice. Unfortunately, we have no other options here,
        // until the `juniper::GraphQLType` itself will allow to do it in some cleverer way.
        let resolver_check = parse_quote! {
            ({ #resolver_code } as ::std::option::Option<&#ty>).is_some()
        };

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
                context_ty: None,
                span,
            })
        }
    }
}

/// Checks whether all [GraphQL union][1] `variants` represent a different Rust type.
///
/// # Notice
///
/// This is not an optimal implementation, as it's possible to bypass this check by using a full
/// qualified path instead (`crate::Test` vs `Test`). Since this requirement is mandatory, the
/// static assertion [`assert_type_ne_all!`][2] is used to enforce this requirement in the generated
/// code. However, due to the bad error message this implementation should stay and provide
/// guidance.
///
/// [1]: https://spec.graphql.org/June2018/#sec-Unions
/// [2]: https://docs.rs/static_assertions/latest/static_assertions/macro.assert_type_ne_all.html
fn all_variants_different(variants: &[UnionVariantDefinition]) -> bool {
    let mut types: Vec<_> = variants.iter().map(|var| &var.ty).collect();
    types.dedup();
    types.len() == variants.len()
}
