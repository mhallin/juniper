use crate::util;
use proc_macro::TokenStream;
use proc_macro_error::*;
use quote::quote;

/// Generate code for the juniper::object macro.
pub fn build_object(args: TokenStream, body: TokenStream, is_internal: bool) -> TokenStream {
    let impl_attrs = match syn::parse::<util::ObjectAttributes>(args) {
        Ok(attrs) => attrs,
        Err(e) => {
            panic!("Invalid attributes:\n{}", e);
        }
    };

    let item = match syn::parse::<syn::Item>(body) {
        Ok(item) => item,
        Err(err) => {
            panic!("Parsing error:\n{}", err);
        }
    };
    let mut _impl = match item {
        syn::Item::Impl(_impl) => _impl,
        _ => {
            panic!("#[juniper::object] can only be applied to impl blocks");
        }
    };

    match _impl.trait_ {
        Some((_, ref path, _)) => {
            let name = path
                .segments
                .iter()
                .map(|segment| segment.ident.to_string())
                .collect::<Vec<_>>()
                .join(".");
            if !(name == "GraphQLObject" || name == "juniper.GraphQLObject") {
                panic!("The impl block must implement the 'GraphQLObject' trait");
            }
        }
        None => {
            // panic!("The impl block must implement the 'GraphQLObject' trait");
        }
    }

    let name = match impl_attrs.name.as_ref() {
        Some(type_name) => type_name.clone(),
        None => {
            let error_msg = "Could not determine a name for the object type: specify one with #[juniper::object(name = \"SomeName\")";

            let path = match &*_impl.self_ty {
                syn::Type::Path(ref type_path) => &type_path.path,
                syn::Type::Reference(ref reference) => match &*reference.elem {
                    syn::Type::Path(ref type_path) => &type_path.path,
                    syn::Type::TraitObject(ref trait_obj) => {
                        match trait_obj.bounds.iter().nth(0).unwrap() {
                            syn::TypeParamBound::Trait(ref trait_bound) => &trait_bound.path,
                            _ => panic!(error_msg),
                        }
                    }
                    _ => panic!(error_msg),
                },
                _ => panic!(error_msg),
            };

            path.segments.iter().last().unwrap().ident.to_string()
        }
    };

    let target_type = *_impl.self_ty.clone();

    let description = impl_attrs
        .description
        .or(util::get_doc_comment(&_impl.attrs));

    let mut definition = util::GraphQLTypeDefiniton {
        name,
        _type: target_type.clone(),
        context: impl_attrs.context,
        scalar: impl_attrs.scalar,
        description,
        fields: Vec::new(),
        generics: _impl.generics.clone(),
        interfaces: if impl_attrs.interfaces.len() > 0 {
            Some(impl_attrs.interfaces)
        } else {
            None
        },
        include_type_generics: false,
        generic_scalar: false,
    };

    for item in _impl.items {
        match item {
            syn::ImplItem::Method(method) => {
                let _type = match &method.sig.output {
                    syn::ReturnType::Type(_, ref t) => (**t).clone(),
                    syn::ReturnType::Default => {
                        panic!(
                            "Invalid field method {}: must return a value",
                            method.sig.ident
                        );
                    }
                };

                let mut attrs = match util::FieldAttributes::from_attrs(
                    method.attrs,
                    util::FieldAttributeParseMode::Impl,
                ) {
                    Ok(attrs) => attrs,
                    Err(err) => abort!(
                        err.span(),
                        "Invalid #[graphql(...)] attribute on field {}:\n{}",
                        method.sig.ident,
                        err
                    ),
                };

                let mut args = Vec::new();
                let mut resolve_parts = Vec::new();

                for arg in method.sig.inputs {
                    match arg {
                        syn::FnArg::Receiver(rec) => {
                            if rec.reference.is_none() || rec.mutability.is_some() {
                                panic!(
                                    "Invalid method receiver {}(self, ...): did you mean '&self'?",
                                    method.sig.ident
                                );
                            }
                        }
                        syn::FnArg::Typed(ref captured) => {
                            let (arg_ident, is_mut) = match &*captured.pat {
                                syn::Pat::Ident(ref pat_ident) => {
                                    (&pat_ident.ident, pat_ident.mutability.is_some())
                                }
                                _ => {
                                    panic!("Invalid token for function argument");
                                }
                            };
                            let arg_ident_name = arg_ident.to_string();

                            if let Some(field_arg) = util::parse_argument_attrs(&captured) {
                                // We insert with `arg_ident_name` as the key because the argument
                                // might have been renamed in the param attribute and we need to
                                // look it up for making `final_name` further down.
                                attrs.arguments.insert(arg_ident_name.clone(), field_arg);
                            }

                            let context_type = definition.context.as_ref();

                            // Check for executor arguments.
                            if util::type_is_identifier_ref(&captured.ty, "Executor") {
                                resolve_parts.push(quote!(let #arg_ident = executor;));
                            }
                            // Make sure executor is specified as a reference.
                            else if util::type_is_identifier(&captured.ty, "Executor") {
                                panic!("Invalid executor argument: to access the Executor, you need to specify the type as a reference.\nDid you mean &Executor?");
                            }
                            // Check for context arg.
                            else if context_type
                                .clone()
                                .map(|ctx| util::type_is_ref_of(&captured.ty, ctx))
                                .unwrap_or(false)
                            {
                                resolve_parts.push(quote!( let #arg_ident = executor.context(); ));
                            }
                            // Make sure the user does not specify the Context
                            //  without a reference. (&Context)
                            else if context_type
                                .clone()
                                .map(|ctx| ctx == &*captured.ty)
                                .unwrap_or(false)
                            {
                                panic!(
                                    "Invalid context argument: to access the context, you need to specify the type as a reference.\nDid you mean &{}?",
                                    quote!(captured.ty),
                                );
                            } else {
                                // Regular argument.

                                let ty = &captured.ty;

                                let final_name = attrs
                                    .argument(&arg_ident_name)
                                    .and_then(|arg| arg.name.as_ref())
                                    .map(|name| util::to_camel_case(&name.to_string()))
                                    .unwrap_or_else(|| util::to_camel_case(&arg_ident_name));

                                let expect_text = format!("Internal error: missing argument {} - validation must have failed", &final_name);
                                let mut_modifier = if is_mut { quote!(mut) } else { quote!() };
                                resolve_parts.push(quote!(
                                    let #mut_modifier #arg_ident = args
                                        .get::<#ty>(#final_name)
                                        .expect(#expect_text);
                                ));
                                args.push(util::GraphQLTypeDefinitionFieldArg {
                                    description: attrs.argument(&arg_ident_name).and_then(|arg| {
                                        arg.description.as_ref().map(|d| d.value())
                                    }),
                                    default: attrs
                                        .argument(&arg_ident_name)
                                        .and_then(|arg| arg.default.clone()),
                                    _type: ty.clone(),
                                    name: final_name,
                                })
                            }
                        }
                    }
                }

                let body = &method.block;
                let return_ty = &method.sig.output;
                let resolver_code = quote!(
                    (|| #return_ty {
                        #( #resolve_parts )*
                        #body
                    })()
                );

                let ident = &method.sig.ident;
                let name = attrs
                    .name
                    .unwrap_or_else(|| util::to_camel_case(&ident.to_string()));

                definition.fields.push(util::GraphQLTypeDefinitionField {
                    name,
                    _type,
                    args,
                    description: attrs.description,
                    deprecation: attrs.deprecation,
                    resolver_code,
                });
            }
            _ => {
                panic!("Invalid item for GraphQL Object: only type declarations and methods are allowed");
            }
        }
    }
    let juniper_crate_name = if is_internal { "crate" } else { "juniper" };
    definition.into_tokens(juniper_crate_name).into()
}
