#![allow(clippy::collapsible_if)]

use crate::{
    result::GraphQLScope,
    util::{self, span_container::SpanContainer},
};
use proc_macro2::TokenStream;
use quote::quote;
use syn::spanned::Spanned;

#[derive(Debug)]
struct ScalarCodegenInput {
    impl_for_type: Option<syn::PathSegment>,
    custom_data_type: Option<syn::PathSegment>,
    custom_data_type_is_struct: bool,
    resolve_body: Option<syn::Block>,
    from_input_value_arg: Option<syn::Ident>,
    from_input_value_body: Option<syn::Block>,
    from_input_value_result: Option<syn::Type>,
    from_str_arg: Option<syn::Ident>,
    from_str_body: Option<syn::Block>,
    from_str_result: Option<syn::Type>,
}

fn get_first_method_arg(
    inputs: syn::punctuated::Punctuated<syn::FnArg, syn::Token![,]>,
) -> Option<syn::Ident> {
    if let Some(fn_arg) = inputs.first() {
        if let syn::FnArg::Typed(pat_type) = fn_arg {
            if let syn::Pat::Ident(pat_ident) = &*pat_type.pat {
                return Some(pat_ident.ident.clone());
            }
        }
    }

    None
}

fn get_method_return_type(output: syn::ReturnType) -> Option<syn::Type> {
    match output {
        syn::ReturnType::Type(_, return_type) => Some(*return_type),
        _ => None,
    }
}

// Find the enum type by inspecting the type parameter on the return value
fn get_enum_type(return_type: &Option<syn::Type>) -> Option<syn::PathSegment> {
    if let Some(return_type) = return_type {
        match return_type {
            syn::Type::Path(type_path) => {
                let path_segment = type_path
                    .path
                    .segments
                    .iter()
                    .find(|ps| match ps.arguments {
                        syn::PathArguments::AngleBracketed(_) => true,
                        _ => false,
                    });

                if let Some(path_segment) = path_segment {
                    match &path_segment.arguments {
                        syn::PathArguments::AngleBracketed(generic_args) => {
                            let generic_type_arg =
                                generic_args.args.iter().find(|generic_type_arg| {
                                    matches!(generic_type_arg, syn::GenericArgument::Type(_))
                                });

                            if let Some(syn::GenericArgument::Type(syn::Type::Path(type_path))) =
                                generic_type_arg
                            {
                                if let Some(path_segment) = type_path.path.segments.first() {
                                    return Some(path_segment.clone());
                                }
                            }
                        }
                        _ => (),
                    }
                }
            }
            _ => (),
        }
    }

    None
}

impl syn::parse::Parse for ScalarCodegenInput {
    fn parse(input: syn::parse::ParseStream) -> syn::parse::Result<Self> {
        let mut impl_for_type: Option<syn::PathSegment> = None;
        let mut enum_data_type: Option<syn::PathSegment> = None;
        let mut resolve_body: Option<syn::Block> = None;
        let mut from_input_value_arg: Option<syn::Ident> = None;
        let mut from_input_value_body: Option<syn::Block> = None;
        let mut from_input_value_result: Option<syn::Type> = None;
        let mut from_str_arg: Option<syn::Ident> = None;
        let mut from_str_body: Option<syn::Block> = None;
        let mut from_str_result: Option<syn::Type> = None;

        let parse_custom_scalar_value_impl: syn::ItemImpl = input.parse()?;
        // To implement a custom scalar for a struct, it's required to
        // specify a generic type and a type bound
        let custom_data_type_is_struct: bool =
            !parse_custom_scalar_value_impl.generics.params.is_empty();

        if let syn::Type::Path(type_path) = *parse_custom_scalar_value_impl.self_ty {
            if let Some(path_segment) = type_path.path.segments.first() {
                impl_for_type = Some(path_segment.clone());
            }
        }

        for impl_item in parse_custom_scalar_value_impl.items {
            match impl_item {
                syn::ImplItem::Method(method) => match method.sig.ident.to_string().as_str() {
                    "resolve" => {
                        resolve_body = Some(method.block);
                    }
                    "from_input_value" => {
                        from_input_value_arg = get_first_method_arg(method.sig.inputs);
                        from_input_value_result = get_method_return_type(method.sig.output);
                        from_input_value_body = Some(method.block);
                    }
                    "from_str" => {
                        from_str_arg = get_first_method_arg(method.sig.inputs);
                        from_str_result = get_method_return_type(method.sig.output);

                        if !custom_data_type_is_struct {
                            enum_data_type = get_enum_type(&from_str_result);
                        }

                        from_str_body = Some(method.block);
                    }
                    _ => (),
                },
                _ => (),
            };
        }

        let custom_data_type = if custom_data_type_is_struct {
            impl_for_type.clone()
        } else {
            enum_data_type
        };

        Ok(ScalarCodegenInput {
            impl_for_type,
            custom_data_type,
            custom_data_type_is_struct,
            resolve_body,
            from_input_value_arg,
            from_input_value_body,
            from_input_value_result,
            from_str_arg,
            from_str_body,
            from_str_result,
        })
    }
}

/// Generate code for the juniper::graphql_scalar proc macro.
pub fn build_scalar(
    attributes: TokenStream,
    body: TokenStream,
    error: GraphQLScope,
) -> syn::Result<TokenStream> {
    let body_span = body.span();

    let attrs = syn::parse2::<util::FieldAttributes>(attributes)?;
    let input = syn::parse2::<ScalarCodegenInput>(body)?;

    let impl_for_type = input.impl_for_type.ok_or_else(|| {
        error.custom_error(
            body_span,
            "unable to find target for implementation target for `GraphQLScalar`",
        )
    })?;
    let custom_data_type = input
        .custom_data_type
        .ok_or_else(|| error.custom_error(body_span, "unable to find custom scalar data type"))?;
    let resolve_body = input
        .resolve_body
        .ok_or_else(|| error.custom_error(body_span, "unable to find body of `resolve` method"))?;
    let from_input_value_arg = input.from_input_value_arg.ok_or_else(|| {
        error.custom_error(
            body_span,
            "unable to find argument for `from_input_value` method",
        )
    })?;
    let from_input_value_body = input.from_input_value_body.ok_or_else(|| {
        error.custom_error(
            body_span,
            "unable to find body of `from_input_value` method",
        )
    })?;
    let from_input_value_result = input.from_input_value_result.ok_or_else(|| {
        error.custom_error(
            body_span,
            "unable to find return type of `from_input_value` method",
        )
    })?;
    let from_str_arg = input.from_str_arg.ok_or_else(|| {
        error.custom_error(body_span, "unable to find argument for `from_str` method")
    })?;
    let from_str_body = input
        .from_str_body
        .ok_or_else(|| error.custom_error(body_span, "unable to find body of `from_str` method"))?;
    let from_str_result = input.from_str_result.ok_or_else(|| {
        error.custom_error(body_span, "unable to find return type of `from_str` method")
    })?;

    let name = attrs
        .name
        .map(SpanContainer::into_inner)
        .unwrap_or_else(|| impl_for_type.ident.to_string());
    let description = match attrs.description {
        Some(val) => quote!(.description(#val)),
        None => quote!(),
    };
    let async_generic_type = match input.custom_data_type_is_struct {
        true => quote!(__S),
        _ => quote!(#custom_data_type),
    };
    let async_generic_type_decl = match input.custom_data_type_is_struct {
        true => quote!(<#async_generic_type>),
        _ => quote!(),
    };
    let generic_type = match input.custom_data_type_is_struct {
        true => quote!(S),
        _ => quote!(#custom_data_type),
    };
    let generic_type_decl = match input.custom_data_type_is_struct {
        true => quote!(<#generic_type>),
        _ => quote!(),
    };
    let generic_type_bound = match input.custom_data_type_is_struct {
        true => quote!(where #generic_type: ::juniper::ScalarValue,),
        _ => quote!(),
    };

    let _async = quote!(
        impl#async_generic_type_decl ::juniper::GraphQLValueAsync<#async_generic_type> for #impl_for_type
        where
            Self: Sync,
            Self::TypeInfo: Sync,
            #async_generic_type: ::juniper::ScalarValue + Send + Sync,
        {
            fn resolve_async<'a>(
                &'a self,
                info: &'a Self::TypeInfo,
                selection_set: Option<&'a [::juniper::Selection<#async_generic_type>]>,
                executor: &'a ::juniper::Executor<Self::Context, #async_generic_type>,
            ) -> ::juniper::LocalBoxFuture<'a, ::juniper::ExecutionResult<#async_generic_type>> {
                use ::juniper::futures::future;
                let v = ::juniper::GraphQLValue::resolve(self, info, selection_set, executor);
                Box::pin(future::ready(v))
            }
        }
    );

    let content = quote!(
        #_async

        impl#generic_type_decl ::juniper::marker::IsInputType<#generic_type> for #impl_for_type
            #generic_type_bound { }

        impl#generic_type_decl ::juniper::marker::IsOutputType<#generic_type> for #impl_for_type
            #generic_type_bound { }

        impl#generic_type_decl ::juniper::GraphQLType<#generic_type> for #impl_for_type
        #generic_type_bound
        {
            fn name(_: &Self::TypeInfo) -> Option<&'static str> {
                Some(#name)
            }

            fn meta<'r>(
                info: &Self::TypeInfo,
                registry: &mut ::juniper::Registry<'r, #generic_type>,
            ) -> ::juniper::meta::MetaType<'r, #generic_type>
            where
                #generic_type: 'r,
            {
                registry.build_scalar_type::<Self>(info)
                    #description
                    .into_meta()
            }
        }

        impl#generic_type_decl ::juniper::GraphQLValue<#generic_type> for #impl_for_type
        #generic_type_bound
        {
            type Context = ();
            type TypeInfo = ();

            fn type_name<'__i>(&self, info: &'__i Self::TypeInfo) -> Option<&'__i str> {
                <Self as ::juniper::GraphQLType<#generic_type>>::name(info)
            }

            fn resolve(
                &self,
                info: &(),
                selection: Option<&[::juniper::Selection<#generic_type>]>,
                executor: &::juniper::Executor<Self::Context, #generic_type>,
            ) -> ::juniper::ExecutionResult<#generic_type> {
                Ok(#resolve_body)
            }
        }

        impl#generic_type_decl ::juniper::ToInputValue<#generic_type> for #impl_for_type
        #generic_type_bound
        {
            fn to_input_value(&self) -> ::juniper::InputValue<#generic_type> {
                let v = #resolve_body;
                ::juniper::ToInputValue::to_input_value(&v)
            }
        }

        impl#generic_type_decl ::juniper::FromInputValue<#generic_type> for #impl_for_type
        #generic_type_bound
        {
            fn from_input_value(#from_input_value_arg: &::juniper::InputValue<#generic_type>) -> #from_input_value_result {
                #from_input_value_body
            }
        }

        impl#generic_type_decl ::juniper::ParseScalarValue<#generic_type> for #impl_for_type
        #generic_type_bound
            {
                fn from_str<'a>(
                    #from_str_arg: ::juniper::parser::ScalarToken<'a>,
                ) -> #from_str_result {
                #from_str_body
            }
        }
    );

    Ok(content)
}
