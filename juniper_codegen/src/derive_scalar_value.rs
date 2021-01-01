use crate::{
    common::parse::ParseBufferExt as _,
    result::GraphQLScope,
    util::{self, span_container::SpanContainer},
};
use proc_macro2::TokenStream;
use quote::quote;
use syn::{spanned::Spanned, token, Data, Fields, Ident, Variant};

#[derive(Debug, Default)]
struct TransparentAttributes {
    transparent: Option<bool>,
    name: Option<String>,
    description: Option<String>,
    scalar: Option<syn::Type>,
}

impl syn::parse::Parse for TransparentAttributes {
    fn parse(input: syn::parse::ParseStream) -> syn::parse::Result<Self> {
        let mut output = Self {
            transparent: None,
            name: None,
            description: None,
            scalar: None,
        };

        while !input.is_empty() {
            let ident: syn::Ident = input.parse()?;
            match ident.to_string().as_str() {
                "name" => {
                    input.parse::<token::Eq>()?;
                    let val = input.parse::<syn::LitStr>()?;
                    output.name = Some(val.value());
                }
                "description" => {
                    input.parse::<token::Eq>()?;
                    let val = input.parse::<syn::LitStr>()?;
                    output.description = Some(val.value());
                }
                "transparent" => {
                    output.transparent = Some(true);
                }
                "scalar" | "Scalar" => {
                    input.parse::<token::Eq>()?;
                    let val = input.parse::<syn::Type>()?;
                    output.scalar = Some(val);
                }
                _ => return Err(syn::Error::new(ident.span(), "unknown attribute")),
            }
            input.try_parse::<token::Comma>()?;
        }

        Ok(output)
    }
}

impl TransparentAttributes {
    fn from_attrs(attrs: &[syn::Attribute]) -> syn::parse::Result<Self> {
        match util::find_graphql_attr(attrs) {
            Some(attr) => {
                let mut parsed: TransparentAttributes = attr.parse_args()?;
                if parsed.description.is_none() {
                    parsed.description =
                        util::get_doc_comment(attrs).map(SpanContainer::into_inner);
                }
                Ok(parsed)
            }
            None => Ok(Default::default()),
        }
    }
}

pub fn impl_scalar_value(ast: &syn::DeriveInput, error: GraphQLScope) -> syn::Result<TokenStream> {
    let ident = &ast.ident;

    match ast.data {
        Data::Enum(ref enum_data) => impl_scalar_enum(ident, enum_data, error),
        Data::Struct(ref struct_data) => impl_scalar_struct(ast, struct_data, error),
        Data::Union(_) => Err(error.custom_error(ast.span(), "may not be applied to unions")),
    }
}

fn impl_scalar_struct(
    ast: &syn::DeriveInput,
    data: &syn::DataStruct,
    error: GraphQLScope,
) -> syn::Result<TokenStream> {
    let field = match data.fields {
        syn::Fields::Unnamed(ref fields) if fields.unnamed.len() == 1 => {
            fields.unnamed.first().unwrap()
        }
        _ => {
            return Err(error.custom_error(
                data.fields.span(),
                "requires exact one field, e.g., Test(i32)",
            ))
        }
    };
    let ident = &ast.ident;
    let attrs = TransparentAttributes::from_attrs(&ast.attrs)?;
    let inner_ty = &field.ty;
    let name = attrs.name.unwrap_or_else(|| ident.to_string());

    let description = match attrs.description {
        Some(val) => quote!( .description( #val ) ),
        None => quote!(),
    };

    let scalar = attrs
        .scalar
        .as_ref()
        .map(|s| quote!( #s ))
        .unwrap_or_else(|| quote!(__S));

    let impl_generics = attrs
        .scalar
        .as_ref()
        .map(|_| quote!())
        .unwrap_or_else(|| quote!(<__S>));

    let _async = quote!(
        impl#impl_generics ::juniper::GraphQLValueAsync<#scalar> for #ident
        where
            Self: Sync,
            Self::TypeInfo: Sync,
            #scalar: ::juniper::ScalarValue + Send + Sync,
        {
            fn resolve_async<'a>(
                &'a self,
                info: &'a Self::TypeInfo,
                selection_set: Option<&'a [::juniper::Selection<#scalar>]>,
                executor: &'a ::juniper::Executor<Self::Context, #scalar>,
            ) -> ::juniper::LocalBoxFuture<'a, ::juniper::ExecutionResult<#scalar>> {
                use ::juniper::futures::future;
                let v = ::juniper::GraphQLValue::<#scalar>::resolve(self, info, selection_set, executor);
                Box::pin(future::ready(v))
            }
        }
    );

    let content = quote!(
        #_async

        impl#impl_generics ::juniper::GraphQLType<#scalar> for #ident
        where
            #scalar: ::juniper::ScalarValue,
        {
            fn name(_: &Self::TypeInfo) -> Option<&'static str> {
                Some(#name)
            }

            fn meta<'r>(
                info: &Self::TypeInfo,
                registry: &mut ::juniper::Registry<'r, #scalar>,
            ) -> ::juniper::meta::MetaType<'r, #scalar>
            where
                #scalar: 'r,
            {
                registry.build_scalar_type::<Self>(info)
                    #description
                    .into_meta()
            }
        }

        impl#impl_generics ::juniper::GraphQLValue<#scalar> for #ident
        where
            #scalar: ::juniper::ScalarValue,
        {
            type Context = ();
            type TypeInfo = ();

            fn type_name<'__i>(&self, info: &'__i Self::TypeInfo) -> Option<&'__i str> {
                <Self as ::juniper::GraphQLType<#scalar>>::name(info)
            }

            fn resolve(
                &self,
                info: &(),
                selection: Option<&[::juniper::Selection<#scalar>]>,
                executor: &::juniper::Executor<Self::Context, #scalar>,
            ) -> ::juniper::ExecutionResult<#scalar> {
                ::juniper::GraphQLValue::<#scalar>::resolve(&self.0, info, selection, executor)
            }
        }

        impl#impl_generics ::juniper::ToInputValue<#scalar> for #ident
        where
            #scalar: ::juniper::ScalarValue,
        {
            fn to_input_value(&self) -> ::juniper::InputValue<#scalar> {
                ::juniper::ToInputValue::<#scalar>::to_input_value(&self.0)
            }
        }

        impl#impl_generics ::juniper::FromInputValue<#scalar> for #ident
        where
            #scalar: ::juniper::ScalarValue,
        {
            fn from_input_value(v: &::juniper::InputValue<#scalar>) -> Option<#ident> {
                let inner: #inner_ty = ::juniper::FromInputValue::<#scalar>::from_input_value(v)?;
                Some(#ident(inner))
            }
        }

        impl#impl_generics ::juniper::ParseScalarValue<#scalar> for #ident
        where
            #scalar: ::juniper::ScalarValue,
        {
            fn from_str<'a>(
                value: ::juniper::parser::ScalarToken<'a>,
            ) -> ::juniper::ParseScalarResult<'a, #scalar> {
                <#inner_ty as ::juniper::ParseScalarValue<#scalar>>::from_str(value)
            }
        }

        impl#impl_generics ::juniper::marker::IsOutputType<#scalar> for #ident
            where #scalar: ::juniper::ScalarValue,
        { }
        impl#impl_generics ::juniper::marker::IsInputType<#scalar> for #ident
            where #scalar: ::juniper::ScalarValue,
        { }
    );

    Ok(content)
}

fn impl_scalar_enum(
    ident: &syn::Ident,
    data: &syn::DataEnum,
    error: GraphQLScope,
) -> syn::Result<TokenStream> {
    let froms = data
        .variants
        .iter()
        .map(|v| derive_from_variant(v, ident, &error))
        .collect::<Result<Vec<_>, _>>()?;

    let serialize = derive_serialize(data.variants.iter(), ident);

    let display = derive_display(data.variants.iter(), ident);

    Ok(quote! {
        #(#froms)*

        #serialize
        #display
    })
}

fn derive_display<'a, I>(variants: I, ident: &Ident) -> TokenStream
where
    I: Iterator<Item = &'a Variant>,
{
    let arms = variants.map(|v| {
        let variant = &v.ident;
        quote!(#ident::#variant(ref v) => write!(f, "{}", v),)
    });

    quote! {
        impl std::fmt::Display for #ident {
            fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                match *self {
                    #(#arms)*
                }
            }
        }
    }
}

fn derive_serialize<'a, I>(variants: I, ident: &Ident) -> TokenStream
where
    I: Iterator<Item = &'a Variant>,
{
    let arms = variants.map(|v| {
        let variant = &v.ident;
        quote!(#ident::#variant(ref v) => v.serialize(serializer),)
    });

    quote! {
        impl ::juniper::serde::Serialize for #ident {
            fn serialize<S>(&self, serializer: S) -> ::std::result::Result<S::Ok, S::Error>
            where S: ::juniper::serde::Serializer
            {
                match *self {
                    #(#arms)*
                }
            }
        }
    }
}

fn derive_from_variant(
    variant: &Variant,
    ident: &Ident,
    error: &GraphQLScope,
) -> syn::Result<TokenStream> {
    let ty = match variant.fields {
        Fields::Unnamed(ref u) if u.unnamed.len() == 1 => &u.unnamed.first().unwrap().ty,

        _ => {
            return Err(error.custom_error(
                variant.fields.span(),
                "requires exact one field, e.g., Test(i32)",
            ))
        }
    };

    let variant = &variant.ident;

    Ok(quote! {
        impl ::std::convert::From<#ty> for #ident {
            fn from(t: #ty) -> Self {
                #ident::#variant(t)
            }
        }

        impl<'a> ::std::convert::From<&'a #ident> for std::option::Option<&'a #ty> {
            fn from(t: &'a #ident) -> Self {
                match *t {
                    #ident::#variant(ref t) => std::option::Option::Some(t),
                    _ => std::option::Option::None
                }
            }
        }

        impl ::std::convert::From<#ident> for std::option::Option<#ty> {
            fn from(t: #ident) -> Self {
                match t {
                    #ident::#variant(t) => std::option::Option::Some(t),
                    _ => std::option::Option::None
                }
            }
        }
    })
}
