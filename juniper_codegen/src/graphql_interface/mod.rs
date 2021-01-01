//! Code generation for [GraphQL interface][1].
//!
//! [1]: https://spec.graphql.org/June2018/#sec-Interfaces

pub mod attr;

use std::collections::{HashMap, HashSet};

use proc_macro2::TokenStream;
use quote::{format_ident, quote, ToTokens, TokenStreamExt as _};
use syn::{
    parse::{Parse, ParseStream},
    parse_quote,
    spanned::Spanned as _,
    token,
};

use crate::{
    common::{
        gen,
        parse::{
            attr::{err, OptionExt as _},
            GenericsExt as _, ParseBufferExt as _,
        },
        ScalarValueType,
    },
    util::{filter_attrs, get_deprecated, get_doc_comment, span_container::SpanContainer},
};

/// Available metadata (arguments) behind `#[graphql_interface]` attribute placed on a trait
/// definition, when generating code for [GraphQL interface][1] type.
///
/// [1]: https://spec.graphql.org/June2018/#sec-Interfaces
#[derive(Debug, Default)]
struct TraitMeta {
    /// Explicitly specified name of [GraphQL interface][1] type.
    ///
    /// If absent, then Rust trait name is used by default.
    ///
    /// [1]: https://spec.graphql.org/June2018/#sec-Interfaces
    name: Option<SpanContainer<String>>,

    /// Explicitly specified [description][2] of [GraphQL interface][1] type.
    ///
    /// If absent, then Rust doc comment is used as [description][2], if any.
    ///
    /// [1]: https://spec.graphql.org/June2018/#sec-Interfaces
    /// [2]: https://spec.graphql.org/June2018/#sec-Descriptions
    description: Option<SpanContainer<String>>,

    /// Explicitly specified identifier of the enum Rust type behind the trait, being an actual
    /// implementation of a [GraphQL interface][1] type.
    ///
    /// If absent, then `{trait_name}Value` identifier will be used.
    ///
    /// [1]: https://spec.graphql.org/June2018/#sec-Interfaces
    r#enum: Option<SpanContainer<syn::Ident>>,

    /// Explicitly specified identifier of the Rust type alias of the [trait object][2], being an
    /// actual implementation of a [GraphQL interface][1] type.
    ///
    /// Effectively makes code generation to use a [trait object][2] as a [GraphQL interface][1]
    /// type rather than an enum. If absent, then enum is used by default.
    ///
    /// [1]: https://spec.graphql.org/June2018/#sec-Interfaces
    /// [2]: https://doc.rust-lang.org/reference/types/trait-object.html
    r#dyn: Option<SpanContainer<syn::Ident>>,

    /// Explicitly specified Rust types of [GraphQL objects][2] implementing this
    /// [GraphQL interface][1] type.
    ///
    /// [1]: https://spec.graphql.org/June2018/#sec-Interfaces
    /// [2]: https://spec.graphql.org/June2018/#sec-Objects
    implementers: HashSet<SpanContainer<syn::Type>>,

    /// Explicitly specified type of [`Context`] to use for resolving this [GraphQL interface][1]
    /// type with.
    ///
    /// If absent, then unit type `()` is assumed as type of [`Context`].
    ///
    /// [`Context`]: juniper::Context
    /// [1]: https://spec.graphql.org/June2018/#sec-Interfaces
    context: Option<SpanContainer<syn::Type>>,

    /// Explicitly specified type of [`ScalarValue`] to use for resolving this
    /// [GraphQL interface][1] type with.
    ///
    /// If absent, then generated code will be generic over any [`ScalarValue`] type, which, in
    /// turn, requires all [interface][1] implementers to be generic over any [`ScalarValue`] type
    /// too. That's why this type should be specified only if one of the implementers implements
    /// [`GraphQLType`] in a non-generic way over [`ScalarValue`] type.
    ///
    /// [`GraphQLType`]: juniper::GraphQLType
    /// [`ScalarValue`]: juniper::ScalarValue
    /// [1]: https://spec.graphql.org/June2018/#sec-Interfaces
    scalar: Option<SpanContainer<syn::Type>>,

    /// Explicitly specified marker indicating that the Rust trait should be transformed into
    /// [`async_trait`].
    ///
    /// If absent, then trait will be transformed into [`async_trait`] only if it contains async
    /// methods.
    asyncness: Option<SpanContainer<syn::Ident>>,

    /// Explicitly specified external downcasting functions for [GraphQL interface][1] implementers.
    ///
    /// If absent, then macro will downcast to the implementers via enum dispatch or dynamic
    /// dispatch (if the one is chosen). That's why specifying an external resolver function has
    /// sense, when some custom [interface][1] implementer resolving logic is involved.
    ///
    /// Once the downcasting function is specified for some [GraphQL object][2] implementer type, it
    /// cannot be downcast another such function or trait method marked with a
    /// [`MethodMeta::downcast`] marker.
    ///
    /// [1]: https://spec.graphql.org/June2018/#sec-Interfaces
    /// [2]: https://spec.graphql.org/June2018/#sec-Objects
    external_downcasts: HashMap<syn::Type, SpanContainer<syn::ExprPath>>,

    /// Indicator whether the generated code is intended to be used only inside the [`juniper`]
    /// library.
    is_internal: bool,
}

impl Parse for TraitMeta {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut output = Self::default();

        while !input.is_empty() {
            let ident = input.parse_any_ident()?;
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
                "for" | "implementers" => {
                    input.parse::<token::Eq>()?;
                    for impler in input.parse_maybe_wrapped_and_punctuated::<
                        syn::Type, token::Bracket, token::Comma,
                    >()? {
                        let impler_span = impler.span();
                        output
                            .implementers
                            .replace(SpanContainer::new(ident.span(), Some(impler_span), impler))
                            .none_or_else(|_| err::dup_arg(impler_span))?;
                    }
                }
                "dyn" => {
                    input.parse::<token::Eq>()?;
                    let alias = input.parse::<syn::Ident>()?;
                    output
                        .r#dyn
                        .replace(SpanContainer::new(ident.span(), Some(alias.span()), alias))
                        .none_or_else(|_| err::dup_arg(&ident))?
                }
                "enum" => {
                    input.parse::<token::Eq>()?;
                    let alias = input.parse::<syn::Ident>()?;
                    output
                        .r#enum
                        .replace(SpanContainer::new(ident.span(), Some(alias.span()), alias))
                        .none_or_else(|_| err::dup_arg(&ident))?
                }
                "async" => {
                    let span = ident.span();
                    output
                        .asyncness
                        .replace(SpanContainer::new(span, Some(span), ident))
                        .none_or_else(|_| err::dup_arg(span))?;
                }
                "on" => {
                    let ty = input.parse::<syn::Type>()?;
                    input.parse::<token::Eq>()?;
                    let dwncst = input.parse::<syn::ExprPath>()?;
                    let dwncst_spanned = SpanContainer::new(ident.span(), Some(ty.span()), dwncst);
                    let dwncst_span = dwncst_spanned.span_joined();
                    output
                        .external_downcasts
                        .insert(ty, dwncst_spanned)
                        .none_or_else(|_| err::dup_arg(dwncst_span))?
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

impl TraitMeta {
    /// Tries to merge two [`TraitMeta`]s into a single one, reporting about duplicates, if any.
    fn try_merge(self, mut another: Self) -> syn::Result<Self> {
        Ok(Self {
            name: try_merge_opt!(name: self, another),
            description: try_merge_opt!(description: self, another),
            context: try_merge_opt!(context: self, another),
            scalar: try_merge_opt!(scalar: self, another),
            implementers: try_merge_hashset!(implementers: self, another => span_joined),
            r#dyn: try_merge_opt!(r#dyn: self, another),
            r#enum: try_merge_opt!(r#enum: self, another),
            asyncness: try_merge_opt!(asyncness: self, another),
            external_downcasts: try_merge_hashmap!(
                external_downcasts: self, another => span_joined
            ),
            is_internal: self.is_internal || another.is_internal,
        })
    }

    /// Parses [`TraitMeta`] from the given multiple `name`d [`syn::Attribute`]s placed on a trait
    /// definition.
    fn from_attrs(name: &str, attrs: &[syn::Attribute]) -> syn::Result<Self> {
        let mut meta = filter_attrs(name, attrs)
            .map(|attr| attr.parse_args())
            .try_fold(Self::default(), |prev, curr| prev.try_merge(curr?))?;

        if let Some(as_dyn) = &meta.r#dyn {
            if meta.r#enum.is_some() {
                return Err(syn::Error::new(
                    as_dyn.span(),
                    "`dyn` attribute argument is not composable with `enum` attribute argument",
                ));
            }
        }

        if meta.description.is_none() {
            meta.description = get_doc_comment(attrs);
        }

        Ok(meta)
    }
}

/// Available metadata (arguments) behind `#[graphql_interface]` attribute placed on a trait
/// implementation block, when generating code for [GraphQL interface][1] type.
///
/// [1]: https://spec.graphql.org/June2018/#sec-Interfaces
#[derive(Debug, Default)]
struct ImplMeta {
    /// Explicitly specified type of [`ScalarValue`] to use for implementing the
    /// [GraphQL interface][1] type.
    ///
    /// If absent, then generated code will be generic over any [`ScalarValue`] type, which, in
    /// turn, requires all [interface][1] implementers to be generic over any [`ScalarValue`] type
    /// too. That's why this type should be specified only if the implementer itself implements
    /// [`GraphQLType`] in a non-generic way over [`ScalarValue`] type.
    ///
    /// [`GraphQLType`]: juniper::GraphQLType
    /// [`ScalarValue`]: juniper::ScalarValue
    /// [1]: https://spec.graphql.org/June2018/#sec-Interfaces
    scalar: Option<SpanContainer<syn::Type>>,

    /// Explicitly specified marker indicating that the trait implementation block should be
    /// transformed with applying [`async_trait`].
    ///
    /// If absent, then trait will be transformed with applying [`async_trait`] only if it contains
    /// async methods.
    ///
    /// This marker is especially useful when Rust trait contains async default methods, while the
    /// implementation block doesn't.
    asyncness: Option<SpanContainer<syn::Ident>>,

    /// Explicitly specified marker indicating that the implemented [GraphQL interface][1] type is
    /// represented as a [trait object][2] in Rust type system rather then an enum (default mode,
    /// when the marker is absent).
    ///
    /// [2]: https://doc.rust-lang.org/reference/types/trait-object.html
    r#dyn: Option<SpanContainer<syn::Ident>>,
}

impl Parse for ImplMeta {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut output = Self::default();

        while !input.is_empty() {
            let ident = input.parse_any_ident()?;
            match ident.to_string().as_str() {
                "scalar" | "Scalar" | "ScalarValue" => {
                    input.parse::<token::Eq>()?;
                    let scl = input.parse::<syn::Type>()?;
                    output
                        .scalar
                        .replace(SpanContainer::new(ident.span(), Some(scl.span()), scl))
                        .none_or_else(|_| err::dup_arg(&ident))?
                }
                "dyn" => {
                    let span = ident.span();
                    output
                        .r#dyn
                        .replace(SpanContainer::new(span, Some(span), ident))
                        .none_or_else(|_| err::dup_arg(span))?;
                }
                "async" => {
                    let span = ident.span();
                    output
                        .asyncness
                        .replace(SpanContainer::new(span, Some(span), ident))
                        .none_or_else(|_| err::dup_arg(span))?;
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

impl ImplMeta {
    /// Tries to merge two [`ImplMeta`]s into a single one, reporting about duplicates, if any.
    fn try_merge(self, mut another: Self) -> syn::Result<Self> {
        Ok(Self {
            scalar: try_merge_opt!(scalar: self, another),
            r#dyn: try_merge_opt!(r#dyn: self, another),
            asyncness: try_merge_opt!(asyncness: self, another),
        })
    }

    /// Parses [`ImplMeta`] from the given multiple `name`d [`syn::Attribute`]s placed on a trait
    /// implementation block.
    pub fn from_attrs(name: &str, attrs: &[syn::Attribute]) -> syn::Result<Self> {
        filter_attrs(name, attrs)
            .map(|attr| attr.parse_args())
            .try_fold(Self::default(), |prev, curr| prev.try_merge(curr?))
    }
}

/// Available metadata (arguments) behind `#[graphql]` attribute placed on a trait method
/// definition, when generating code for [GraphQL interface][1] type.
///
/// [1]: https://spec.graphql.org/June2018/#sec-Interfaces
#[derive(Debug, Default)]
struct MethodMeta {
    /// Explicitly specified name of a [GraphQL field][1] represented by this trait method.
    ///
    /// If absent, then `camelCased` Rust method name is used by default.
    ///
    /// [1]: https://spec.graphql.org/June2018/#sec-Language.Fields
    name: Option<SpanContainer<syn::LitStr>>,

    /// Explicitly specified [description][2] of this [GraphQL field][1].
    ///
    /// If absent, then Rust doc comment is used as the [description][2], if any.
    ///
    /// [1]: https://spec.graphql.org/June2018/#sec-Language.Fields
    /// [2]: https://spec.graphql.org/June2018/#sec-Descriptions
    description: Option<SpanContainer<syn::LitStr>>,

    /// Explicitly specified [deprecation][2] of this [GraphQL field][1].
    ///
    /// If absent, then Rust `#[deprecated]` attribute is used as the [deprecation][2], if any.
    ///
    /// [1]: https://spec.graphql.org/June2018/#sec-Language.Fields
    /// [2]: https://spec.graphql.org/June2018/#sec-Deprecation
    deprecated: Option<SpanContainer<Option<syn::LitStr>>>,

    /// Explicitly specified marker indicating that this trait method should be omitted by code
    /// generation and not considered in the [GraphQL interface][1] type definition.
    ///
    /// [1]: https://spec.graphql.org/June2018/#sec-Interfaces
    ignore: Option<SpanContainer<syn::Ident>>,

    /// Explicitly specified marker indicating that this trait method doesn't represent a
    /// [GraphQL field][1], but is a downcasting function into the [GraphQL object][2] implementer
    /// type returned by this trait method.
    ///
    /// Once this marker is specified, the [GraphQL object][2] implementer type cannot be downcast
    /// via another trait method or [`TraitMeta::external_downcasts`] function.
    ///
    /// [1]: https://spec.graphql.org/June2018/#sec-Language.Fields
    /// [2]: https://spec.graphql.org/June2018/#sec-Objects
    downcast: Option<SpanContainer<syn::Ident>>,
}

impl Parse for MethodMeta {
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
                        .replace(SpanContainer::new(ident.span(), Some(name.span()), name))
                        .none_or_else(|_| err::dup_arg(&ident))?
                }
                "desc" | "description" => {
                    input.parse::<token::Eq>()?;
                    let desc = input.parse::<syn::LitStr>()?;
                    output
                        .description
                        .replace(SpanContainer::new(ident.span(), Some(desc.span()), desc))
                        .none_or_else(|_| err::dup_arg(&ident))?
                }
                "deprecated" => {
                    let mut reason = None;
                    if input.is_next::<token::Eq>() {
                        input.parse::<token::Eq>()?;
                        reason = Some(input.parse::<syn::LitStr>()?);
                    }
                    output
                        .deprecated
                        .replace(SpanContainer::new(
                            ident.span(),
                            reason.as_ref().map(|r| r.span()),
                            reason,
                        ))
                        .none_or_else(|_| err::dup_arg(&ident))?
                }
                "ignore" | "skip" => output
                    .ignore
                    .replace(SpanContainer::new(ident.span(), None, ident.clone()))
                    .none_or_else(|_| err::dup_arg(&ident))?,
                "downcast" => output
                    .downcast
                    .replace(SpanContainer::new(ident.span(), None, ident.clone()))
                    .none_or_else(|_| err::dup_arg(&ident))?,
                name => {
                    return Err(err::unknown_arg(&ident, name));
                }
            }
            input.try_parse::<token::Comma>()?;
        }

        Ok(output)
    }
}

impl MethodMeta {
    /// Tries to merge two [`MethodMeta`]s into a single one, reporting about duplicates, if any.
    fn try_merge(self, mut another: Self) -> syn::Result<Self> {
        Ok(Self {
            name: try_merge_opt!(name: self, another),
            description: try_merge_opt!(description: self, another),
            deprecated: try_merge_opt!(deprecated: self, another),
            ignore: try_merge_opt!(ignore: self, another),
            downcast: try_merge_opt!(downcast: self, another),
        })
    }

    /// Parses [`MethodMeta`] from the given multiple `name`d [`syn::Attribute`]s placed on a
    /// method definition.
    pub fn from_attrs(name: &str, attrs: &[syn::Attribute]) -> syn::Result<Self> {
        let mut meta = filter_attrs(name, attrs)
            .map(|attr| attr.parse_args())
            .try_fold(Self::default(), |prev, curr| prev.try_merge(curr?))?;

        if let Some(ignore) = &meta.ignore {
            if meta.name.is_some()
                || meta.description.is_some()
                || meta.deprecated.is_some()
                || meta.downcast.is_some()
            {
                return Err(syn::Error::new(
                    ignore.span(),
                    "`ignore` attribute argument is not composable with any other arguments",
                ));
            }
        }

        if let Some(downcast) = &meta.downcast {
            if meta.name.is_some()
                || meta.description.is_some()
                || meta.deprecated.is_some()
                || meta.ignore.is_some()
            {
                return Err(syn::Error::new(
                    downcast.span(),
                    "`downcast` attribute argument is not composable with any other arguments",
                ));
            }
        }

        if meta.description.is_none() {
            meta.description = get_doc_comment(attrs).map(|sc| {
                let span = sc.span_ident();
                sc.map(|desc| syn::LitStr::new(&desc, span))
            });
        }

        if meta.deprecated.is_none() {
            meta.deprecated = get_deprecated(attrs).map(|sc| {
                let span = sc.span_ident();
                sc.map(|depr| depr.reason.map(|rsn| syn::LitStr::new(&rsn, span)))
            });
        }

        Ok(meta)
    }
}

/// Available metadata (arguments) behind `#[graphql]` attribute placed on a trait method argument,
/// when generating code for [GraphQL interface][1] type.
///
/// [1]: https://spec.graphql.org/June2018/#sec-Interfaces
#[derive(Debug, Default)]
struct ArgumentMeta {
    /// Explicitly specified name of a [GraphQL argument][1] represented by this method argument.
    ///
    /// If absent, then `camelCased` Rust argument name is used by default.
    ///
    /// [1]: https://spec.graphql.org/June2018/#sec-Language.Arguments
    name: Option<SpanContainer<syn::LitStr>>,

    /// Explicitly specified [description][2] of this [GraphQL argument][1].
    ///
    /// [1]: https://spec.graphql.org/June2018/#sec-Language.Arguments
    /// [2]: https://spec.graphql.org/June2018/#sec-Descriptions
    description: Option<SpanContainer<syn::LitStr>>,

    /// Explicitly specified [default value][2] of this [GraphQL argument][1].
    ///
    /// If the exact default expression is not specified, then the [`Default::default`] value is
    /// used.
    ///
    /// If absent, then this [GraphQL argument][1] is considered as [required][2].
    ///
    /// [1]: https://spec.graphql.org/June2018/#sec-Language.Arguments
    /// [2]: https://spec.graphql.org/June2018/#sec-Required-Arguments
    default: Option<SpanContainer<Option<syn::Expr>>>,

    /// Explicitly specified marker indicating that this method argument doesn't represent a
    /// [GraphQL argument][1], but is a [`Context`] being injected into a [GraphQL field][2]
    /// resolving function.
    ///
    /// If absent, then the method argument still is considered as [`Context`] if it's named
    /// `context` or `ctx`.
    ///
    /// [`Context`]: juniper::Context
    /// [1]: https://spec.graphql.org/June2018/#sec-Language.Arguments
    /// [2]: https://spec.graphql.org/June2018/#sec-Language.Fields
    context: Option<SpanContainer<syn::Ident>>,

    /// Explicitly specified marker indicating that this method argument doesn't represent a
    /// [GraphQL argument][1], but is a [`Executor`] being injected into a [GraphQL field][2]
    /// resolving function.
    ///
    /// If absent, then the method argument still is considered as [`Context`] if it's named
    /// `executor`.
    ///
    /// [`Executor`]: juniper::Executor
    /// [1]: https://spec.graphql.org/June2018/#sec-Language.Arguments
    /// [2]: https://spec.graphql.org/June2018/#sec-Language.Fields
    executor: Option<SpanContainer<syn::Ident>>,
}

impl Parse for ArgumentMeta {
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
                        .replace(SpanContainer::new(ident.span(), Some(name.span()), name))
                        .none_or_else(|_| err::dup_arg(&ident))?
                }
                "desc" | "description" => {
                    input.parse::<token::Eq>()?;
                    let desc = input.parse::<syn::LitStr>()?;
                    output
                        .description
                        .replace(SpanContainer::new(ident.span(), Some(desc.span()), desc))
                        .none_or_else(|_| err::dup_arg(&ident))?
                }
                "default" => {
                    let mut expr = None;
                    if input.is_next::<token::Eq>() {
                        input.parse::<token::Eq>()?;
                        expr = Some(input.parse::<syn::Expr>()?);
                    } else if input.is_next::<token::Paren>() {
                        let inner;
                        let _ = syn::parenthesized!(inner in input);
                        expr = Some(inner.parse::<syn::Expr>()?);
                    }
                    output
                        .default
                        .replace(SpanContainer::new(
                            ident.span(),
                            expr.as_ref().map(|e| e.span()),
                            expr,
                        ))
                        .none_or_else(|_| err::dup_arg(&ident))?
                }
                "ctx" | "context" | "Context" => {
                    let span = ident.span();
                    output
                        .context
                        .replace(SpanContainer::new(span, Some(span), ident))
                        .none_or_else(|_| err::dup_arg(span))?
                }
                "exec" | "executor" => {
                    let span = ident.span();
                    output
                        .executor
                        .replace(SpanContainer::new(span, Some(span), ident))
                        .none_or_else(|_| err::dup_arg(span))?
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

impl ArgumentMeta {
    /// Tries to merge two [`ArgumentMeta`]s into a single one, reporting about duplicates, if any.
    fn try_merge(self, mut another: Self) -> syn::Result<Self> {
        Ok(Self {
            name: try_merge_opt!(name: self, another),
            description: try_merge_opt!(description: self, another),
            default: try_merge_opt!(default: self, another),
            context: try_merge_opt!(context: self, another),
            executor: try_merge_opt!(executor: self, another),
        })
    }

    /// Parses [`ArgumentMeta`] from the given multiple `name`d [`syn::Attribute`]s placed on a
    /// function argument.
    fn from_attrs(name: &str, attrs: &[syn::Attribute]) -> syn::Result<Self> {
        let meta = filter_attrs(name, attrs)
            .map(|attr| attr.parse_args())
            .try_fold(Self::default(), |prev, curr| prev.try_merge(curr?))?;

        if let Some(context) = &meta.context {
            if meta.name.is_some()
                || meta.description.is_some()
                || meta.default.is_some()
                || meta.executor.is_some()
            {
                return Err(syn::Error::new(
                    context.span(),
                    "`context` attribute argument is not composable with any other arguments",
                ));
            }
        }

        if let Some(executor) = &meta.executor {
            if meta.name.is_some()
                || meta.description.is_some()
                || meta.default.is_some()
                || meta.context.is_some()
            {
                return Err(syn::Error::new(
                    executor.span(),
                    "`executor` attribute argument is not composable with any other arguments",
                ));
            }
        }

        Ok(meta)
    }
}

/// Definition of [GraphQL interface][1] for code generation.
///
/// [1]: https://spec.graphql.org/June2018/#sec-Interfaces
struct Definition {
    /// Rust type that this [GraphQL interface][1] is represented with.
    ///
    /// [1]: https://spec.graphql.org/June2018/#sec-Interfaces
    ty: Type,

    /// Name of this [GraphQL interface][1] in GraphQL schema.
    ///
    /// [1]: https://spec.graphql.org/June2018/#sec-Interfaces
    name: String,

    /// Description of this [GraphQL interface][1] to put into GraphQL schema.
    ///
    /// [1]: https://spec.graphql.org/June2018/#sec-Interfaces
    description: Option<String>,

    /// Rust type of [`Context`] to generate [`GraphQLType`] implementation with for this
    /// [GraphQL interface][1].
    ///
    /// If [`None`] then generated code will use unit type `()` as [`Context`].
    ///
    /// [`GraphQLType`]: juniper::GraphQLType
    /// [`Context`]: juniper::Context
    /// [1]: https://spec.graphql.org/June2018/#sec-Interfaces
    context: Option<syn::Type>,

    /// [`ScalarValue`] parametrization to generate [`GraphQLType`] implementation with for this
    /// [GraphQL interface][1].
    ///
    /// [`GraphQLType`]: juniper::GraphQLType
    /// [`ScalarValue`]: juniper::ScalarValue
    /// [1]: https://spec.graphql.org/June2018/#sec-Interfaces
    scalar: ScalarValueType,

    /// Defined [`Field`]s of this [GraphQL interface][1].
    ///
    /// [1]: https://spec.graphql.org/June2018/#sec-Interfaces
    fields: Vec<Field>,

    /// Defined [`Implementer`]s of this [GraphQL interface][1].
    ///
    /// [1]: https://spec.graphql.org/June2018/#sec-Interfaces
    implementers: Vec<Implementer>,
}

impl Definition {
    /// Returns generated code that panics about unknown field tried to be resolved on this
    /// [GraphQL interface][1].
    ///
    /// [1]: https://spec.graphql.org/June2018/#sec-Interfaces
    #[must_use]
    fn panic_no_field_tokens(&self) -> TokenStream {
        let scalar = &self.scalar;

        quote! {
            panic!(
                "Field `{}` not found on type `{}`",
                field,
                <Self as ::juniper::GraphQLType<#scalar>>::name(info).unwrap(),
            )
        }
    }

    /// Returns generated code implementing [`GraphQLType`] trait for this [GraphQL interface][1].
    ///
    /// [`GraphQLType`]: juniper::GraphQLType
    /// [1]: https://spec.graphql.org/June2018/#sec-Interfaces
    #[must_use]
    fn impl_graphql_type_tokens(&self) -> TokenStream {
        let scalar = &self.scalar;

        let generics = self.ty.impl_generics();
        let (impl_generics, _, where_clause) = generics.split_for_impl();

        let ty = self.ty.ty_tokens();

        let name = &self.name;
        let description = self
            .description
            .as_ref()
            .map(|desc| quote! { .description(#desc) });

        // Sorting is required to preserve/guarantee the order of implementers registered in schema.
        let mut impler_tys: Vec<_> = self.implementers.iter().map(|impler| &impler.ty).collect();
        impler_tys.sort_unstable_by(|a, b| {
            let (a, b) = (quote!(#a).to_string(), quote!(#b).to_string());
            a.cmp(&b)
        });

        let fields_meta = self.fields.iter().map(Field::method_meta_tokens);

        quote! {
            #[automatically_derived]
            impl#impl_generics ::juniper::GraphQLType<#scalar> for #ty #where_clause
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
                    // Ensure all implementer types are registered.
                    #( let _ = registry.get_type::<#impler_tys>(info); )*

                    let fields = [
                        #( #fields_meta, )*
                    ];
                    registry.build_interface_type::<#ty>(info, &fields)
                        #description
                        .into_meta()
                }
            }
        }
    }

    /// Returns generated code implementing [`GraphQLValue`] trait for this [GraphQL interface][1].
    ///
    /// [`GraphQLValue`]: juniper::GraphQLValue
    /// [1]: https://spec.graphql.org/June2018/#sec-Interfaces
    #[must_use]
    fn impl_graphql_value_tokens(&self) -> TokenStream {
        let scalar = &self.scalar;

        let generics = self.ty.impl_generics();
        let (impl_generics, _, where_clause) = generics.split_for_impl();

        let ty = self.ty.ty_tokens();
        let trait_ty = self.ty.trait_ty();
        let context = self.context.clone().unwrap_or_else(|| parse_quote! { () });

        let fields_resolvers = self
            .fields
            .iter()
            .filter_map(|f| f.method_resolve_field_tokens(&trait_ty));
        let async_fields_panic = {
            let names = self
                .fields
                .iter()
                .filter_map(|field| {
                    if field.is_async {
                        Some(&field.name)
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();
            if names.is_empty() {
                None
            } else {
                Some(quote! {
                    #( #names )|* => panic!(
                        "Tried to resolve async field `{}` on type `{}` with a sync resolver",
                        field,
                        <Self as ::juniper::GraphQLType<#scalar>>::name(info).unwrap(),
                    ),
                })
            }
        };
        let no_field_panic = self.panic_no_field_tokens();

        let custom_downcast_checks = self
            .implementers
            .iter()
            .filter_map(|i| i.method_concrete_type_name_tokens(&trait_ty));
        let regular_downcast_check = self.ty.method_concrete_type_name_tokens();

        let custom_downcasts = self
            .implementers
            .iter()
            .filter_map(|i| i.method_resolve_into_type_tokens(&trait_ty));
        let regular_downcast = self.ty.method_resolve_into_type_tokens();

        quote! {
            #[automatically_derived]
            impl#impl_generics ::juniper::GraphQLValue<#scalar> for #ty #where_clause
            {
                type Context = #context;
                type TypeInfo = ();

                fn type_name<'__i>(&self, info: &'__i Self::TypeInfo) -> Option<&'__i str> {
                    <Self as ::juniper::GraphQLType<#scalar>>::name(info)
                }

                fn resolve_field(
                    &self,
                    info: &Self::TypeInfo,
                    field: &str,
                    args: &::juniper::Arguments<#scalar>,
                    executor: &::juniper::Executor<Self::Context, #scalar>,
                ) -> ::juniper::ExecutionResult<#scalar> {
                    match field {
                        #( #fields_resolvers )*
                        #async_fields_panic
                        _ => #no_field_panic,
                    }
                }

                fn concrete_type_name(
                    &self,
                    context: &Self::Context,
                    info: &Self::TypeInfo,
                ) -> String {
                    #( #custom_downcast_checks )*
                    #regular_downcast_check
                }

                fn resolve_into_type(
                    &self,
                    info: &Self::TypeInfo,
                    type_name: &str,
                    _: Option<&[::juniper::Selection<#scalar>]>,
                    executor: &::juniper::Executor<Self::Context, #scalar>,
                ) -> ::juniper::ExecutionResult<#scalar> {
                    #( #custom_downcasts )*
                    #regular_downcast
                }
            }
        }
    }

    /// Returns generated code implementing [`GraphQLValueAsync`] trait for this
    /// [GraphQL interface][1].
    ///
    /// [`GraphQLValueAsync`]: juniper::GraphQLValueAsync
    /// [1]: https://spec.graphql.org/June2018/#sec-Interfaces
    #[must_use]
    fn impl_graphql_value_async_tokens(&self) -> TokenStream {
        let scalar = &self.scalar;

        let generics = self.ty.impl_generics();
        let (impl_generics, _, where_clause) = generics.split_for_impl();
        let mut where_clause = where_clause
            .cloned()
            .unwrap_or_else(|| parse_quote! { where });
        where_clause.predicates.push(parse_quote! { Self: Sync });
        if self.scalar.is_generic() {
            where_clause
                .predicates
                .push(parse_quote! { #scalar: Send + Sync });
        }

        let ty = self.ty.ty_tokens();
        let trait_ty = self.ty.trait_ty();

        let fields_resolvers = self
            .fields
            .iter()
            .map(|f| f.method_resolve_field_async_tokens(&trait_ty));
        let no_field_panic = self.panic_no_field_tokens();

        let custom_downcasts = self
            .implementers
            .iter()
            .filter_map(|i| i.method_resolve_into_type_async_tokens(&trait_ty));
        let regular_downcast = self.ty.method_resolve_into_type_async_tokens();

        quote! {
            #[automatically_derived]
            impl#impl_generics ::juniper::GraphQLValueAsync<#scalar> for #ty #where_clause
            {
                fn resolve_field_async<'b>(
                    &'b self,
                    info: &'b Self::TypeInfo,
                    field: &'b str,
                    args: &'b ::juniper::Arguments<#scalar>,
                    executor: &'b ::juniper::Executor<Self::Context, #scalar>,
                ) -> ::juniper::LocalBoxFuture<'b, ::juniper::ExecutionResult<#scalar>> {
                    match field {
                        #( #fields_resolvers )*
                        _ => #no_field_panic,
                    }
                }

                fn resolve_into_type_async<'b>(
                    &'b self,
                    info: &'b Self::TypeInfo,
                    type_name: &str,
                    _: Option<&'b [::juniper::Selection<'b, #scalar>]>,
                    executor: &'b ::juniper::Executor<'b, 'b, Self::Context, #scalar>
                ) -> ::juniper::LocalBoxFuture<'b, ::juniper::ExecutionResult<#scalar>> {
                    #( #custom_downcasts )*
                    #regular_downcast
                }
            }
        }
    }

    /// Returns generated code implementing [`GraphQLInterface`] trait for this
    /// [GraphQL interface][1].
    ///
    /// [`GraphQLInterface`]: juniper::GraphQLInterface
    /// [1]: https://spec.graphql.org/June2018/#sec-Interfaces
    #[must_use]
    fn impl_graphql_interface_tokens(&self) -> TokenStream {
        let scalar = &self.scalar;

        let generics = self.ty.impl_generics();
        let (impl_generics, _, where_clause) = generics.split_for_impl();

        let ty = self.ty.ty_tokens();

        let impler_tys: Vec<_> = self.implementers.iter().map(|impler| &impler.ty).collect();

        let all_implers_unique = if impler_tys.len() > 1 {
            Some(quote! { ::juniper::sa::assert_type_ne_all!(#( #impler_tys ),*); })
        } else {
            None
        };

        quote! {
            #[automatically_derived]
            impl#impl_generics ::juniper::marker::GraphQLInterface<#scalar> for #ty #where_clause
            {
                fn mark() {
                    #all_implers_unique

                    #( <#impler_tys as ::juniper::marker::GraphQLObjectType<#scalar>>::mark(); )*
                }
            }
        }
    }

    /// Returns generated code implementing [`marker::IsOutputType`] trait for this
    /// [GraphQL interface][1].
    ///
    /// [`marker::IsOutputType`]: juniper::marker::IsOutputType
    /// [1]: https://spec.graphql.org/June2018/#sec-Interfaces
    #[must_use]
    fn impl_output_type_tokens(&self) -> TokenStream {
        let scalar = &self.scalar;

        let generics = self.ty.impl_generics();
        let (impl_generics, _, where_clause) = generics.split_for_impl();

        let ty = self.ty.ty_tokens();

        let fields_marks = self.fields.iter().map(|field| {
            let arguments_marks = field.arguments.iter().filter_map(|arg| {
                let arg_ty = &arg.as_regular()?.ty;
                Some(quote! { <#arg_ty as ::juniper::marker::IsInputType<#scalar>>::mark(); })
            });

            let field_ty = &field.ty;
            let resolved_ty = quote! {
                <#field_ty as ::juniper::IntoResolvable<
                    '_, #scalar, _, <Self as ::juniper::GraphQLValue<#scalar>>::Context,
                >>::Type
            };

            quote! {
                #( #arguments_marks )*
                <#resolved_ty as ::juniper::marker::IsOutputType<#scalar>>::mark();
            }
        });

        let impler_tys = self.implementers.iter().map(|impler| &impler.ty);

        quote! {
            #[automatically_derived]
            impl#impl_generics ::juniper::marker::IsOutputType<#scalar> for #ty #where_clause
            {
                fn mark() {
                    #( #fields_marks )*
                    #( <#impler_tys as ::juniper::marker::IsOutputType<#scalar>>::mark(); )*
                }
            }
        }
    }
}

impl ToTokens for Definition {
    fn to_tokens(&self, into: &mut TokenStream) {
        into.append_all(&[
            self.ty.to_token_stream(),
            self.impl_graphql_interface_tokens(),
            self.impl_output_type_tokens(),
            self.impl_graphql_type_tokens(),
            self.impl_graphql_value_tokens(),
            self.impl_graphql_value_async_tokens(),
        ]);
    }
}

/// Representation of [GraphQL interface][1] field [argument][2] for code generation.
///
/// [1]: https://spec.graphql.org/June2018/#sec-Interfaces
/// [2]: https://spec.graphql.org/June2018/#sec-Language.Arguments
#[derive(Debug)]
struct FieldArgument {
    /// Rust type that this [GraphQL field argument][2] is represented by.
    ///
    /// [2]: https://spec.graphql.org/June2018/#sec-Language.Arguments
    ty: syn::Type,

    /// Name of this [GraphQL field argument][2] in GraphQL schema.
    ///
    /// [2]: https://spec.graphql.org/June2018/#sec-Language.Arguments
    name: String,

    /// [Description][1] of this [GraphQL field argument][2] to put into GraphQL schema.
    ///
    /// [1]: https://spec.graphql.org/June2018/#sec-Descriptions
    /// [2]: https://spec.graphql.org/June2018/#sec-Language.Arguments
    description: Option<String>,

    /// Default value of this [GraphQL field argument][2] in GraphQL schema.
    ///
    /// If outer [`Option`] is [`None`], then this [argument][2] is a [required][3] one.
    ///
    /// If inner [`Option`] is [`None`], then the [`Default::default`] value is used.
    ///
    /// [2]: https://spec.graphql.org/June2018/#sec-Language.Arguments
    /// [3]: https://spec.graphql.org/June2018/#sec-Required-Arguments
    default: Option<Option<syn::Expr>>,
}

/// Possible kinds of Rust trait method arguments for code generation.
#[derive(Debug)]
enum MethodArgument {
    /// Regular [GraphQL field argument][1].
    ///
    /// [1]: https://spec.graphql.org/June2018/#sec-Language.Arguments
    Regular(FieldArgument),

    /// [`Context`] passed into a [GraphQL field][2] resolving method.
    ///
    /// [`Context`]: juniper::Context
    /// [2]: https://spec.graphql.org/June2018/#sec-Language.Fields
    Context(syn::Type),

    /// [`Executor`] passed into a [GraphQL field][2] resolving method.
    ///
    /// [`Executor`]: juniper::Executor
    /// [2]: https://spec.graphql.org/June2018/#sec-Language.Fields
    Executor,
}

impl MethodArgument {
    /// Returns this [`MethodArgument`] as a [`FieldArgument`], if it represents one.
    #[must_use]
    fn as_regular(&self) -> Option<&FieldArgument> {
        if let Self::Regular(arg) = self {
            Some(arg)
        } else {
            None
        }
    }

    /// Returns [`syn::Type`] of this [`MethodArgument::Context`], if it represents one.
    #[must_use]
    fn context_ty(&self) -> Option<&syn::Type> {
        if let Self::Context(ty) = self {
            Some(ty)
        } else {
            None
        }
    }

    /// Returns generated code for the [`GraphQLType::meta`] method, which registers this
    /// [`MethodArgument`] in [`Registry`], if it represents a [`FieldArgument`].
    ///
    /// [`GraphQLType::meta`]: juniper::GraphQLType::meta
    /// [`Registry`]: juniper::Registry
    #[must_use]
    fn method_meta_tokens(&self) -> Option<TokenStream> {
        let arg = self.as_regular()?;

        let (name, ty) = (&arg.name, &arg.ty);

        let description = arg
            .description
            .as_ref()
            .map(|desc| quote! { .description(#desc) });

        let method = if let Some(val) = &arg.default {
            let val = val
                .as_ref()
                .map(|v| quote! { (#v).into() })
                .unwrap_or_else(|| quote! { <#ty as Default>::default() });
            quote! { .arg_with_default::<#ty>(#name, &#val, info) }
        } else {
            quote! { .arg::<#ty>(#name, info) }
        };

        Some(quote! { .argument(registry#method#description) })
    }

    /// Returns generated code for the [`GraphQLValue::resolve_field`] method, which provides the
    /// value of this [`MethodArgument`] to be passed into a trait method call.
    ///
    /// [`GraphQLValue::resolve_field`]: juniper::GraphQLValue::resolve_field
    #[must_use]
    fn method_resolve_field_tokens(&self) -> TokenStream {
        match self {
            Self::Regular(arg) => {
                let (name, ty) = (&arg.name, &arg.ty);
                let err_text = format!(
                    "Internal error: missing argument `{}` - validation must have failed",
                    &name,
                );

                quote! {
                    args.get::<#ty>(#name).expect(#err_text)
                }
            }

            Self::Context(_) => quote! {
                ::juniper::FromContext::from(executor.context())
            },

            Self::Executor => quote! { &executor },
        }
    }
}

/// Representation of [GraphQL interface][1] [field][2] for code generation.
///
/// [1]: https://spec.graphql.org/June2018/#sec-Interfaces
/// [2]: https://spec.graphql.org/June2018/#sec-Language.Fields
#[derive(Debug)]
struct Field {
    /// Rust type that this [GraphQL field][2] is represented by (method return type).
    ///
    /// [2]: https://spec.graphql.org/June2018/#sec-Language.Fields
    ty: syn::Type,

    /// Name of this [GraphQL field][2] in GraphQL schema.
    ///
    /// [2]: https://spec.graphql.org/June2018/#sec-Language.Fields
    name: String,

    /// [Description][1] of this [GraphQL field][2] to put into GraphQL schema.
    ///
    /// [1]: https://spec.graphql.org/June2018/#sec-Descriptions
    /// [2]: https://spec.graphql.org/June2018/#sec-Language.Fields
    description: Option<String>,

    /// [Deprecation][1] of this [GraphQL field][2] to put into GraphQL schema.
    ///
    /// If inner [`Option`] is [`None`], then deprecation has no message attached.
    ///
    /// [1]: https://spec.graphql.org/June2018/#sec-Deprecation
    /// [2]: https://spec.graphql.org/June2018/#sec-Language.Fields
    deprecated: Option<Option<String>>,

    /// Name of Rust trait method representing this [GraphQL field][2].
    ///
    /// [2]: https://spec.graphql.org/June2018/#sec-Language.Fields
    method: syn::Ident,

    /// Rust trait [`MethodArgument`]s required to call the trait method representing this
    /// [GraphQL field][2].
    ///
    /// [2]: https://spec.graphql.org/June2018/#sec-Language.Fields
    arguments: Vec<MethodArgument>,

    /// Indicator whether this [GraphQL field][2] should be resolved asynchronously.
    ///
    /// [2]: https://spec.graphql.org/June2018/#sec-Language.Fields
    is_async: bool,
}

impl Field {
    /// Returns generated code for the [`GraphQLType::meta`] method, which registers this
    /// [`Field`] in [`Registry`].
    ///
    /// [`GraphQLType::meta`]: juniper::GraphQLType::meta
    /// [`Registry`]: juniper::Registry
    #[must_use]
    fn method_meta_tokens(&self) -> TokenStream {
        let (name, ty) = (&self.name, &self.ty);

        let description = self
            .description
            .as_ref()
            .map(|desc| quote! { .description(#desc) });

        let deprecated = self.deprecated.as_ref().map(|reason| {
            let reason = reason
                .as_ref()
                .map(|rsn| quote! { Some(#rsn) })
                .unwrap_or_else(|| quote! { None });
            quote! { .deprecated(#reason) }
        });

        let arguments = self
            .arguments
            .iter()
            .filter_map(MethodArgument::method_meta_tokens);

        quote! {
            registry.field_convert::<#ty, _, Self::Context>(#name, info)
                #( #arguments )*
                #description
                #deprecated
        }
    }

    /// Returns generated code for the [`GraphQLValue::resolve_field`] method, which resolves this
    /// [`Field`] synchronously.
    ///
    /// Returns [`None`] if this [`Field::is_async`].
    ///
    /// [`GraphQLValue::resolve_field`]: juniper::GraphQLValue::resolve_field
    #[must_use]
    fn method_resolve_field_tokens(&self, trait_ty: &syn::Type) -> Option<TokenStream> {
        if self.is_async {
            return None;
        }

        let (name, ty, method) = (&self.name, &self.ty, &self.method);

        let arguments = self
            .arguments
            .iter()
            .map(MethodArgument::method_resolve_field_tokens);

        let resolving_code = gen::sync_resolving_code();

        Some(quote! {
            #name => {
                let res: #ty = <Self as #trait_ty>::#method(self #( , #arguments )*);
                #resolving_code
            }
        })
    }

    /// Returns generated code for the [`GraphQLValueAsync::resolve_field_async`] method, which
    /// resolves this [`Field`] asynchronously.
    ///
    /// [`GraphQLValueAsync::resolve_field_async`]: juniper::GraphQLValueAsync::resolve_field_async
    #[must_use]
    fn method_resolve_field_async_tokens(&self, trait_ty: &syn::Type) -> TokenStream {
        let (name, ty, method) = (&self.name, &self.ty, &self.method);

        let arguments = self
            .arguments
            .iter()
            .map(MethodArgument::method_resolve_field_tokens);

        let mut fut = quote! { <Self as #trait_ty>::#method(self #( , #arguments )*) };
        if !self.is_async {
            fut = quote! { ::juniper::futures::future::ready(#fut) };
        }

        let resolving_code = gen::async_resolving_code(Some(ty));

        quote! {
            #name => {
                let fut = #fut;
                #resolving_code
            }
        }
    }
}

/// Representation of custom downcast into an [`Implementer`] from a [GraphQL interface][1] type for
/// code generation.
///
/// [1]: https://spec.graphql.org/June2018/#sec-Interfaces
#[derive(Clone, Debug)]
enum ImplementerDowncast {
    /// Downcast is performed via a method of trait describing a [GraphQL interface][1].
    ///
    /// [1]: https://spec.graphql.org/June2018/#sec-Interfaces
    Method {
        /// Name of trait method which performs this [`ImplementerDowncast`].
        name: syn::Ident,

        /// Indicator whether the trait method accepts a [`Context`] as its second argument.
        ///
        /// [`Context`]: juniper::Context
        with_context: bool,
    },

    /// Downcast is performed via some external function.
    External {
        /// Path of the external function to be called with.
        path: syn::ExprPath,
    },
}

/// Representation of [GraphQL interface][1] implementer for code generation.
///
/// [1]: https://spec.graphql.org/June2018/#sec-Interfaces
#[derive(Clone, Debug)]
struct Implementer {
    /// Rust type that this [GraphQL interface][1] [`Implementer`] is represented by.
    ///
    /// [1]: https://spec.graphql.org/June2018/#sec-Interfaces
    ty: syn::Type,

    /// Custom [`ImplementerDowncast`] for this [`Implementer`].
    ///
    /// If absent, then [`Implementer`] is downcast from an enum variant or a trait object.
    downcast: Option<ImplementerDowncast>,

    /// Rust type of [`Context`] that this [GraphQL interface][1] [`Implementer`] requires for
    /// downcasting.
    ///
    /// It's available only when code generation happens for Rust traits and a trait method contains
    /// context argument.
    ///
    /// [`Context`]: juniper::Context
    /// [1]: https://spec.graphql.org/June2018/#sec-Interfaces
    context_ty: Option<syn::Type>,

    /// [`ScalarValue`] parametrization of this [`Implementer`].
    ///
    /// [`ScalarValue`]: juniper::ScalarValue
    scalar: ScalarValueType,
}

impl Implementer {
    /// Returns generated code of downcasting this [`Implementer`] via custom
    /// [`ImplementerDowncast`].
    ///
    /// Returns [`None`] if there is no custom [`Implementer::downcast`].
    #[must_use]
    fn downcast_call_tokens(
        &self,
        trait_ty: &syn::Type,
        ctx: Option<syn::Expr>,
    ) -> Option<TokenStream> {
        let ctx = ctx.unwrap_or_else(|| parse_quote! { executor.context() });
        let mut ctx_arg = Some(quote! { , ::juniper::FromContext::from(#ctx) });

        let fn_path = match self.downcast.as_ref()? {
            ImplementerDowncast::Method { name, with_context } => {
                if !with_context {
                    ctx_arg = None;
                }
                quote! { <Self as #trait_ty>::#name }
            }
            ImplementerDowncast::External { path } => {
                quote! { #path }
            }
        };

        Some(quote! {
            #fn_path(self #ctx_arg)
        })
    }

    /// Returns generated code for the [`GraphQLValue::concrete_type_name`] method, which returns
    /// name of the GraphQL type represented by this [`Implementer`].
    ///
    /// Returns [`None`] if there is no custom [`Implementer::downcast`].
    ///
    /// [`GraphQLValue::concrete_type_name`]: juniper::GraphQLValue::concrete_type_name
    #[must_use]
    fn method_concrete_type_name_tokens(&self, trait_ty: &syn::Type) -> Option<TokenStream> {
        self.downcast.as_ref()?;

        let ty = &self.ty;
        let scalar = &self.scalar;

        let downcast = self.downcast_call_tokens(trait_ty, Some(parse_quote! { context }));

        // Doing this may be quite an expensive, because resolving may contain some heavy
        // computation, so we're preforming it twice. Unfortunately, we have no other options here,
        // until the `juniper::GraphQLType` itself will allow to do it in some cleverer way.
        Some(quote! {
            if (#downcast as ::std::option::Option<&#ty>).is_some() {
                return <#ty as ::juniper::GraphQLType<#scalar>>::name(info).unwrap().to_string();
            }
        })
    }

    /// Returns generated code for the [`GraphQLValue::resolve_into_type`] method, which downcasts
    /// the [GraphQL interface][1] type into this [`Implementer`] synchronously.
    ///
    /// Returns [`None`] if there is no custom [`Implementer::downcast`].
    ///
    /// [`GraphQLValue::resolve_into_type`]: juniper::GraphQLValue::resolve_into_type
    /// [1]: https://spec.graphql.org/June2018/#sec-Interfaces
    #[must_use]
    fn method_resolve_into_type_tokens(&self, trait_ty: &syn::Type) -> Option<TokenStream> {
        self.downcast.as_ref()?;

        let ty = &self.ty;
        let scalar = &self.scalar;

        let downcast = self.downcast_call_tokens(trait_ty, None);

        let resolving_code = gen::sync_resolving_code();

        Some(quote! {
            if type_name == <#ty as ::juniper::GraphQLType<#scalar>>::name(info).unwrap() {
                let res = #downcast;
                return #resolving_code;
            }
        })
    }

    /// Returns generated code for the [`GraphQLValueAsync::resolve_into_type_async`][0] method,
    /// which downcasts the [GraphQL interface][1] type into this [`Implementer`] asynchronously.
    ///
    /// Returns [`None`] if there is no custom [`Implementer::downcast`].
    ///
    /// [0]: juniper::GraphQLValueAsync::resolve_into_type_async
    /// [1]: https://spec.graphql.org/June2018/#sec-Interfaces
    #[must_use]
    fn method_resolve_into_type_async_tokens(&self, trait_ty: &syn::Type) -> Option<TokenStream> {
        self.downcast.as_ref()?;

        let ty = &self.ty;
        let scalar = &self.scalar;

        let downcast = self.downcast_call_tokens(trait_ty, None);

        let resolving_code = gen::async_resolving_code(None);

        Some(quote! {
            if type_name == <#ty as ::juniper::GraphQLType<#scalar>>::name(info).unwrap() {
                let fut = ::juniper::futures::future::ready(#downcast);
                return #resolving_code;
            }
        })
    }
}

/// Representation of Rust enum implementing [GraphQL interface][1] type for code generation.
///
/// [1]: https://spec.graphql.org/June2018/#sec-Interfaces
struct EnumType {
    /// Name of this [`EnumType`] to generate it with.
    ident: syn::Ident,

    /// [`syn::Visibility`] of this [`EnumType`] to generate it with.
    visibility: syn::Visibility,

    /// Rust types of all [GraphQL interface][1] implements to represent variants of this
    /// [`EnumType`].
    ///
    /// [1]: https://spec.graphql.org/June2018/#sec-Interfaces
    variants: Vec<syn::Type>,

    /// Name of the trait describing the [GraphQL interface][1] represented by this [`EnumType`].
    ///
    /// [1]: https://spec.graphql.org/June2018/#sec-Interfaces
    trait_ident: syn::Ident,

    /// [`syn::Generics`] of the trait describing the [GraphQL interface][1] represented by this
    /// [`EnumType`].
    ///
    /// [1]: https://spec.graphql.org/June2018/#sec-Interfaces
    trait_generics: syn::Generics,

    /// Associated types of the trait describing the [GraphQL interface][1] represented by this
    /// [`EnumType`].
    trait_types: Vec<(syn::Ident, syn::Generics)>,

    /// Associated constants of the trait describing the [GraphQL interface][1] represented by this
    /// [`EnumType`].
    ///
    /// [1]: https://spec.graphql.org/June2018/#sec-Interfaces
    trait_consts: Vec<(syn::Ident, syn::Type)>,

    /// Methods of the trait describing the [GraphQL interface][1] represented by this [`EnumType`].
    ///
    /// [1]: https://spec.graphql.org/June2018/#sec-Interfaces
    trait_methods: Vec<syn::Signature>,

    /// [`ScalarValue`] parametrization to generate [`GraphQLType`] implementation with for this
    /// [`EnumType`].
    ///
    /// [`GraphQLType`]: juniper::GraphQLType
    /// [`ScalarValue`]: juniper::ScalarValue
    scalar: ScalarValueType,
}

impl EnumType {
    /// Constructs new [`EnumType`] out of the given parameters.
    #[must_use]
    fn new(
        r#trait: &syn::ItemTrait,
        meta: &TraitMeta,
        implers: &[Implementer],
        scalar: ScalarValueType,
    ) -> Self {
        Self {
            ident: meta
                .r#enum
                .as_ref()
                .map(SpanContainer::as_ref)
                .cloned()
                .unwrap_or_else(|| format_ident!("{}Value", r#trait.ident)),
            visibility: r#trait.vis.clone(),
            variants: implers.iter().map(|impler| impler.ty.clone()).collect(),
            trait_ident: r#trait.ident.clone(),
            trait_generics: r#trait.generics.clone(),
            trait_types: r#trait
                .items
                .iter()
                .filter_map(|i| {
                    if let syn::TraitItem::Type(ty) = i {
                        Some((ty.ident.clone(), ty.generics.clone()))
                    } else {
                        None
                    }
                })
                .collect(),
            trait_consts: r#trait
                .items
                .iter()
                .filter_map(|i| {
                    if let syn::TraitItem::Const(cnst) = i {
                        Some((cnst.ident.clone(), cnst.ty.clone()))
                    } else {
                        None
                    }
                })
                .collect(),
            trait_methods: r#trait
                .items
                .iter()
                .filter_map(|i| {
                    if let syn::TraitItem::Method(m) = i {
                        Some(m.sig.clone())
                    } else {
                        None
                    }
                })
                .collect(),
            scalar,
        }
    }

    /// Returns name of a single variant of this [`EnumType`] by the given underlying [`syn::Type`]
    /// of the variant.
    #[must_use]
    fn variant_ident(ty: &syn::Type) -> &syn::Ident {
        if let syn::Type::Path(p) = ty {
            &p.path.segments.last().unwrap().ident
        } else {
            unreachable!("GraphQL object has unexpected type `{}`", quote! { #ty })
        }
    }

    /// Indicates whether this [`EnumType`] has non-exhaustive phantom variant to hold type
    /// parameters.
    #[must_use]
    fn has_phantom_variant(&self) -> bool {
        !self.trait_generics.params.is_empty()
    }

    /// Returns generate code for dispatching non-exhaustive phantom variant of this [`EnumType`]
    /// in `match` expressions.
    ///
    /// Returns [`None`] if this [`EnumType`] is exhaustive.
    #[must_use]
    fn non_exhaustive_match_arm_tokens(&self) -> Option<TokenStream> {
        if self.has_phantom_variant() || self.variants.is_empty() {
            Some(quote! { _ => unreachable!(), })
        } else {
            None
        }
    }

    /// Returns prepared [`syn::Generics`] for [`GraphQLType`] trait (and similar) implementation
    /// for this [`EnumType`].
    ///
    /// [`GraphQLType`]: juniper::GraphQLType
    #[must_use]
    fn impl_generics(&self) -> syn::Generics {
        let mut generics = self.trait_generics.clone();

        let scalar = &self.scalar;
        if self.scalar.is_implicit_generic() {
            generics.params.push(parse_quote! { #scalar });
        }
        if self.scalar.is_generic() {
            generics
                .make_where_clause()
                .predicates
                .push(parse_quote! { #scalar: ::juniper::ScalarValue });
        }

        generics
    }

    /// Returns full type signature of the original trait describing the [GraphQL interface][1] for
    /// this [`EnumType`].
    ///
    /// [1]: https://spec.graphql.org/June2018/#sec-Interfaces
    #[must_use]
    fn trait_ty(&self) -> syn::Type {
        let ty = &self.trait_ident;
        let (_, generics, _) = self.trait_generics.split_for_impl();

        parse_quote! { #ty#generics }
    }

    /// Returns generated code of the full type signature of this [`EnumType`].
    #[must_use]
    fn ty_tokens(&self) -> TokenStream {
        let ty = &self.ident;
        let (_, generics, _) = self.trait_generics.split_for_impl();

        quote! { #ty#generics }
    }

    /// Returns generate code of the Rust type definitions of this [`EnumType`].
    ///
    /// If the [`EnumType::trait_generics`] are not empty, then they are contained in the generated
    /// enum too.
    #[must_use]
    fn type_definition_tokens(&self) -> TokenStream {
        let enum_ty = &self.ident;
        let generics = &self.trait_generics;
        let vis = &self.visibility;

        let doc = format!(
            "Type implementing [GraphQL interface][1] represented by `{}` trait.\
             \n\n\
             [1]: https://spec.graphql.org/June2018/#sec-Interfaces",
            self.trait_ident,
        );

        let variants = self.variants.iter().map(|ty| {
            let variant = Self::variant_ident(ty);
            let doc = format!(
                "`{}` implementer of this GraphQL interface.",
                quote! { #ty },
            );

            quote! {
                #[doc = #doc]
                #variant(#ty),
            }
        });

        let phantom_variant = if self.has_phantom_variant() {
            let ty_params = generics.params.iter().map(|p| {
                let ty = match p {
                    syn::GenericParam::Type(ty) => {
                        let ident = &ty.ident;
                        quote! { #ident }
                    }
                    syn::GenericParam::Lifetime(lt) => {
                        let lifetime = &lt.lifetime;
                        quote! { &#lifetime () }
                    }
                    syn::GenericParam::Const(_) => unimplemented!(),
                };
                quote! {
                    ::std::marker::PhantomData<::std::sync::atomic::AtomicPtr<Box<#ty>>>
                }
            });

            Some(quote! {
                #[doc(hidden)]
                __Phantom(#( #ty_params ),*),
            })
        } else {
            None
        };

        quote! {
            #[automatically_derived]
            #[doc = #doc]
            #vis enum #enum_ty#generics {
                #( #variants )*
                #phantom_variant
            }
        }
    }

    /// Returns generated code implementing [`From`] trait for this [`EnumType`] from its
    /// [`EnumType::variants`].
    fn impl_from_tokens(&self) -> impl Iterator<Item = TokenStream> + '_ {
        let enum_ty = &self.ident;
        let (impl_generics, generics, where_clause) = self.trait_generics.split_for_impl();

        self.variants.iter().map(move |ty| {
            let variant = Self::variant_ident(ty);

            quote! {
                #[automatically_derived]
                impl#impl_generics From<#ty> for #enum_ty#generics #where_clause {
                    fn from(v: #ty) -> Self {
                        Self::#variant(v)
                    }
                }
            }
        })
    }

    /// Returns generated code implementing the original trait describing the [GraphQL interface][1]
    /// for this [`EnumType`].
    ///
    /// [1]: https://spec.graphql.org/June2018/#sec-Interfaces
    #[must_use]
    fn impl_trait_tokens(&self) -> TokenStream {
        let enum_ty = &self.ident;

        let trait_ident = &self.trait_ident;
        let (impl_generics, generics, where_clause) = self.trait_generics.split_for_impl();

        let var_ty = self.variants.first();

        let assoc_types = self.trait_types.iter().map(|(ty, ty_gen)| {
            quote! {
                type #ty#ty_gen = <#var_ty as #trait_ident#generics>::#ty#ty_gen;
            }
        });

        let assoc_consts = self.trait_consts.iter().map(|(ident, ty)| {
            quote! {
                const #ident: #ty = <#var_ty as #trait_ident#generics>::#ident;
            }
        });

        let methods = self.trait_methods.iter().map(|sig| {
            let method = &sig.ident;

            let mut sig = sig.clone();
            let mut args = vec![];
            for (n, arg) in sig.inputs.iter_mut().enumerate() {
                match arg {
                    syn::FnArg::Receiver(_) => {}
                    syn::FnArg::Typed(a) => {
                        if !matches!(&*a.pat, syn::Pat::Ident(_)) {
                            let ident = format_ident!("__arg{}", n);
                            a.pat = parse_quote! { #ident };
                        }
                        args.push(a.pat.clone());
                    }
                }
            }

            let and_await = if sig.asyncness.is_some() {
                Some(quote! { .await })
            } else {
                None
            };

            let match_arms = self.variants.iter().map(|ty| {
                let variant = Self::variant_ident(ty);
                let args = args.clone();

                quote! {
                    Self::#variant(v) =>
                        <#ty as #trait_ident#generics>::#method(v #( , #args )* )#and_await,
                }
            });
            let non_exhaustive_match_arm = self.non_exhaustive_match_arm_tokens();

            quote! {
                #sig {
                    match self {
                        #( #match_arms )*
                        #non_exhaustive_match_arm
                    }
                }
            }
        });

        let mut impl_tokens = quote! {
            #[automatically_derived]
            impl#impl_generics #trait_ident#generics for #enum_ty#generics #where_clause {
                #( #assoc_types )*

                #( #assoc_consts )*

                #( #methods )*
            }
        };

        if self.trait_methods.iter().any(|sig| sig.asyncness.is_some()) {
            let mut ast: syn::ItemImpl = parse_quote! { #impl_tokens };
            inject_async_trait(
                &mut ast.attrs,
                ast.items.iter_mut().filter_map(|i| {
                    if let syn::ImplItem::Method(m) = i {
                        Some(&mut m.sig)
                    } else {
                        None
                    }
                }),
                &ast.generics,
            );
            impl_tokens = quote! { #ast };
        }

        impl_tokens
    }

    /// Returns generated code for the [`GraphQLValue::concrete_type_name`] method, which returns
    /// name of the underlying [`Implementer`] GraphQL type contained in this [`EnumType`].
    ///
    /// [`GraphQLValue::concrete_type_name`]: juniper::GraphQLValue::concrete_type_name
    #[must_use]
    fn method_concrete_type_name_tokens(&self) -> TokenStream {
        let scalar = &self.scalar;

        let match_arms = self.variants.iter().map(|ty| {
            let variant = Self::variant_ident(ty);

            quote! {
                Self::#variant(v) => <
                    #ty as ::juniper::GraphQLValue<#scalar>
                >::concrete_type_name(v, context, info),
            }
        });
        let non_exhaustive_match_arm = self.non_exhaustive_match_arm_tokens();

        quote! {
            match self {
                #( #match_arms )*
                #non_exhaustive_match_arm
            }
        }
    }

    /// Returns generated code for the [`GraphQLValue::resolve_into_type`] method, which downcasts
    /// this [`EnumType`] into its underlying [`Implementer`] type synchronously.
    ///
    /// [`GraphQLValue::resolve_into_type`]: juniper::GraphQLValue::resolve_into_type
    #[must_use]
    fn method_resolve_into_type_tokens(&self) -> TokenStream {
        let resolving_code = gen::sync_resolving_code();

        let match_arms = self.variants.iter().map(|ty| {
            let variant = Self::variant_ident(ty);

            quote! {
                Self::#variant(res) => #resolving_code,
            }
        });
        let non_exhaustive_match_arm = self.non_exhaustive_match_arm_tokens();

        quote! {
            match self {
                #( #match_arms )*
                #non_exhaustive_match_arm
            }
        }
    }

    /// Returns generated code for the [`GraphQLValueAsync::resolve_into_type_async`][0] method,
    /// which downcasts this [`EnumType`] into its underlying [`Implementer`] type asynchronously.
    ///
    /// [0]: juniper::GraphQLValueAsync::resolve_into_type_async
    #[must_use]
    fn method_resolve_into_type_async_tokens(&self) -> TokenStream {
        let resolving_code = gen::async_resolving_code(None);

        let match_arms = self.variants.iter().map(|ty| {
            let variant = Self::variant_ident(ty);

            quote! {
                Self::#variant(v) => {
                    let fut = ::juniper::futures::future::ready(v);
                    #resolving_code
                }
            }
        });
        let non_exhaustive_match_arm = self.non_exhaustive_match_arm_tokens();

        quote! {
            match self {
                #( #match_arms )*
                #non_exhaustive_match_arm
            }
        }
    }
}

impl ToTokens for EnumType {
    fn to_tokens(&self, into: &mut TokenStream) {
        into.append_all(&[self.type_definition_tokens()]);
        into.append_all(self.impl_from_tokens());
        into.append_all(&[self.impl_trait_tokens()]);
    }
}

/// Representation of Rust [trait object][2] implementing [GraphQL interface][1] type for code
/// generation.
///
/// [1]: https://spec.graphql.org/June2018/#sec-Interfaces
/// [2]: https://doc.rust-lang.org/reference/types/trait-object.html
struct TraitObjectType {
    /// Name of this [`TraitObjectType`] to generate it with.
    ident: syn::Ident,

    /// [`syn::Visibility`] of this [`TraitObjectType`] to generate it with.
    visibility: syn::Visibility,

    /// Name of the trait describing the [GraphQL interface][1] represented by this
    /// [`TraitObjectType`].
    ///
    /// [1]: https://spec.graphql.org/June2018/#sec-Interfaces
    trait_ident: syn::Ident,

    /// [`syn::Generics`] of the trait describing the [GraphQL interface][1] represented by this
    /// [`TraitObjectType`].
    ///
    /// [1]: https://spec.graphql.org/June2018/#sec-Interfaces
    trait_generics: syn::Generics,

    /// [`ScalarValue`] parametrization of this [`TraitObjectType`] to generate it with.
    ///
    /// [`ScalarValue`]: juniper::ScalarValue
    scalar: ScalarValueType,

    /// Rust type of [`Context`] to generate this [`TraitObjectType`] with.
    ///
    /// If [`None`] then generated code will use unit type `()` as [`Context`].
    ///
    /// [`Context`]: juniper::Context
    context: Option<syn::Type>,
}

impl TraitObjectType {
    /// Constructs new [`TraitObjectType`] out of the given parameters.
    #[must_use]
    fn new(
        r#trait: &syn::ItemTrait,
        meta: &TraitMeta,
        scalar: ScalarValueType,
        context: Option<syn::Type>,
    ) -> Self {
        Self {
            ident: meta.r#dyn.as_ref().unwrap().as_ref().clone(),
            visibility: r#trait.vis.clone(),
            trait_ident: r#trait.ident.clone(),
            trait_generics: r#trait.generics.clone(),
            scalar,
            context,
        }
    }

    /// Returns prepared [`syn::Generics`] for [`GraphQLType`] trait (and similar) implementation
    /// for this [`TraitObjectType`].
    ///
    /// [`GraphQLType`]: juniper::GraphQLType
    #[must_use]
    fn impl_generics(&self) -> syn::Generics {
        let mut generics = self.trait_generics.clone();

        generics.params.push(parse_quote! { '__obj });

        let scalar = &self.scalar;
        if scalar.is_implicit_generic() {
            generics.params.push(parse_quote! { #scalar });
        }
        if scalar.is_generic() {
            generics
                .make_where_clause()
                .predicates
                .push(parse_quote! { #scalar: ::juniper::ScalarValue });
        }

        generics
    }

    /// Returns full type signature of the original trait describing the [GraphQL interface][1] for
    /// this [`TraitObjectType`].
    ///
    /// [1]: https://spec.graphql.org/June2018/#sec-Interfaces
    #[must_use]
    fn trait_ty(&self) -> syn::Type {
        let ty = &self.trait_ident;

        let mut generics = self.trait_generics.clone();
        if !self.scalar.is_explicit_generic() {
            let scalar = &self.scalar;
            generics.params.push(parse_quote! { #scalar });
        }
        let (_, generics, _) = generics.split_for_impl();

        parse_quote! { #ty#generics }
    }

    /// Returns generated code of the full type signature of this [`TraitObjectType`].
    #[must_use]
    fn ty_tokens(&self) -> TokenStream {
        let ty = &self.trait_ident;

        let mut generics = self.trait_generics.clone();
        generics.remove_defaults();
        generics.move_bounds_to_where_clause();
        if !self.scalar.is_explicit_generic() {
            let scalar = &self.scalar;
            generics.params.push(parse_quote! { #scalar });
        }
        let ty_params = &generics.params;

        let context = self.context.clone().unwrap_or_else(|| parse_quote! { () });

        quote! {
            dyn #ty<#ty_params, Context = #context, TypeInfo = ()> + '__obj + Send + Sync
        }
    }

    /// Returns generated code for the [`GraphQLValue::concrete_type_name`] method, which returns
    /// name of the underlying [`Implementer`] GraphQL type contained in this [`TraitObjectType`].
    ///
    /// [`GraphQLValue::concrete_type_name`]: juniper::GraphQLValue::concrete_type_name
    #[must_use]
    fn method_concrete_type_name_tokens(&self) -> TokenStream {
        quote! {
            self.as_dyn_graphql_value().concrete_type_name(context, info)
        }
    }

    /// Returns generated code for the [`GraphQLValue::resolve_into_type`] method, which downcasts
    /// this [`TraitObjectType`] into its underlying [`Implementer`] type synchronously.
    ///
    /// [`GraphQLValue::resolve_into_type`]: juniper::GraphQLValue::resolve_into_type
    #[must_use]
    fn method_resolve_into_type_tokens(&self) -> TokenStream {
        let resolving_code = gen::sync_resolving_code();

        quote! {
            let res = self.as_dyn_graphql_value();
            #resolving_code
        }
    }

    /// Returns generated code for the [`GraphQLValueAsync::resolve_into_type_async`][0] method,
    /// which downcasts this [`TraitObjectType`] into its underlying [`Implementer`] type
    /// asynchronously.
    ///
    /// [0]: juniper::GraphQLValueAsync::resolve_into_type_async
    #[must_use]
    fn method_resolve_into_type_async_tokens(&self) -> TokenStream {
        let resolving_code = gen::async_resolving_code(None);

        quote! {
            let fut = ::juniper::futures::future::ready(self.as_dyn_graphql_value_async());
            #resolving_code
        }
    }
}

impl ToTokens for TraitObjectType {
    fn to_tokens(&self, into: &mut TokenStream) {
        let dyn_ty = &self.ident;
        let vis = &self.visibility;

        let doc = format!(
            "Helper alias for the `{}` [trait object][2] implementing [GraphQL interface][1].\
             \n\n\
             [1]: https://spec.graphql.org/June2018/#sec-Interfaces\n\
             [2]: https://doc.rust-lang.org/reference/types/trait-object.html",
            self.trait_ident,
        );

        let trait_ident = &self.trait_ident;

        let mut generics = self.trait_generics.clone();
        if !self.scalar.is_explicit_generic() {
            let scalar_ty = self.scalar.generic_ty();
            let default_ty = self.scalar.default_ty();
            generics
                .params
                .push(parse_quote! { #scalar_ty = #default_ty });
        }

        let (mut ty_params_left, mut ty_params_right) = (None, None);
        if !generics.params.is_empty() {
            // We should preserve defaults for left side.
            generics.move_bounds_to_where_clause();
            let params = &generics.params;
            ty_params_left = Some(quote! { , #params });

            generics.remove_defaults();
            let params = &generics.params;
            ty_params_right = Some(quote! { #params, });
        };

        let context = self.context.clone().unwrap_or_else(|| parse_quote! { () });

        let dyn_alias = quote! {
            #[automatically_derived]
            #[doc = #doc]
            #vis type #dyn_ty<'a #ty_params_left> =
                dyn #trait_ident<#ty_params_right Context = #context, TypeInfo = ()> +
                    'a + Send + Sync;
        };

        into.append_all(&[dyn_alias]);
    }
}

/// Representation of possible Rust types implementing [GraphQL interface][1] type for code
/// generation.
///
/// [1]: https://spec.graphql.org/June2018/#sec-Interfaces
enum Type {
    /// [GraphQL interface][1] type implementation as Rust enum.
    ///
    /// [1]: https://spec.graphql.org/June2018/#sec-Interfaces
    Enum(Box<EnumType>),

    /// [GraphQL interface][1] type implementation as Rust [trait object][2].
    ///
    /// [1]: https://spec.graphql.org/June2018/#sec-Interfaces
    /// [2]: https://doc.rust-lang.org/reference/types/trait-object.html
    TraitObject(Box<TraitObjectType>),
}

impl Type {
    /// Returns prepared [`syn::Generics`] for [`GraphQLType`] trait (and similar) implementation
    /// for this [`Type`].
    ///
    /// [`GraphQLType`]: juniper::GraphQLType
    #[must_use]
    fn impl_generics(&self) -> syn::Generics {
        match self {
            Self::Enum(e) => e.impl_generics(),
            Self::TraitObject(o) => o.impl_generics(),
        }
    }

    /// Returns full type signature of the original trait describing the [GraphQL interface][1] for
    /// this [`Type`].
    ///
    /// [1]: https://spec.graphql.org/June2018/#sec-Interfaces
    #[must_use]
    fn trait_ty(&self) -> syn::Type {
        match self {
            Self::Enum(e) => e.trait_ty(),
            Self::TraitObject(o) => o.trait_ty(),
        }
    }

    /// Returns generated code of the full type signature of this [`Type`].
    #[must_use]
    fn ty_tokens(&self) -> TokenStream {
        match self {
            Self::Enum(e) => e.ty_tokens(),
            Self::TraitObject(o) => o.ty_tokens(),
        }
    }

    /// Returns generated code for the [`GraphQLValue::concrete_type_name`] method, which returns
    /// name of the underlying [`Implementer`] GraphQL type contained in this [`Type`].
    ///
    /// [`GraphQLValue::concrete_type_name`]: juniper::GraphQLValue::concrete_type_name
    #[must_use]
    fn method_concrete_type_name_tokens(&self) -> TokenStream {
        match self {
            Self::Enum(e) => e.method_concrete_type_name_tokens(),
            Self::TraitObject(o) => o.method_concrete_type_name_tokens(),
        }
    }

    /// Returns generated code for the [`GraphQLValue::resolve_into_type`] method, which downcasts
    /// this [`Type`] into its underlying [`Implementer`] type synchronously.
    ///
    /// [`GraphQLValue::resolve_into_type`]: juniper::GraphQLValue::resolve_into_type
    #[must_use]
    fn method_resolve_into_type_tokens(&self) -> TokenStream {
        match self {
            Self::Enum(e) => e.method_resolve_into_type_tokens(),
            Self::TraitObject(o) => o.method_resolve_into_type_tokens(),
        }
    }

    /// Returns generated code for the [`GraphQLValueAsync::resolve_into_type_async`][0] method,
    /// which downcasts this [`Type`] into its underlying [`Implementer`] type asynchronously.
    ///
    /// [0]: juniper::GraphQLValueAsync::resolve_into_type_async
    fn method_resolve_into_type_async_tokens(&self) -> TokenStream {
        match self {
            Self::Enum(e) => e.method_resolve_into_type_async_tokens(),
            Self::TraitObject(o) => o.method_resolve_into_type_async_tokens(),
        }
    }
}

impl ToTokens for Type {
    fn to_tokens(&self, into: &mut TokenStream) {
        match self {
            Self::Enum(e) => e.to_tokens(into),
            Self::TraitObject(o) => o.to_tokens(into),
        }
    }
}

/// Injects [`async_trait`] implementation into the given trait definition or trait implementation
/// block, correctly restricting type and lifetime parameters with `'async_trait` lifetime, if
/// required.
fn inject_async_trait<'m, M>(attrs: &mut Vec<syn::Attribute>, methods: M, generics: &syn::Generics)
where
    M: IntoIterator<Item = &'m mut syn::Signature>,
{
    attrs.push(parse_quote! { #[::juniper::async_trait] });

    for method in methods.into_iter() {
        if method.asyncness.is_some() {
            let where_clause = &mut method.generics.make_where_clause().predicates;
            for p in &generics.params {
                let ty_param = match p {
                    syn::GenericParam::Type(t) => {
                        let ty_param = &t.ident;
                        quote! { #ty_param }
                    }
                    syn::GenericParam::Lifetime(l) => {
                        let ty_param = &l.lifetime;
                        quote! { #ty_param }
                    }
                    syn::GenericParam::Const(_) => continue,
                };
                where_clause.push(parse_quote! { #ty_param: 'async_trait });
            }
        }
    }
}
