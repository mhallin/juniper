//! This crate supplies custom derive implementations for the
//! [juniper](https://github.com/graphql-rust/juniper) crate.
//!
//! You should not depend on juniper_codegen directly.
//! You only need the `juniper` crate.

#![doc(html_root_url = "https://docs.rs/juniper_codegen/0.14.1")]
#![recursion_limit = "1024"]

extern crate proc_macro;

mod derive_enum;
mod derive_input_object;
mod derive_object;
mod derive_scalar_value;
mod impl_object;
mod util;

use proc_macro::TokenStream;
use proc_macro_error::*;

#[proc_macro_derive(GraphQLEnum, attributes(graphql))]
pub fn derive_enum(input: TokenStream) -> TokenStream {
    let ast = syn::parse::<syn::DeriveInput>(input).unwrap();
    let gen = derive_enum::impl_enum(&ast, false);
    gen.into()
}

#[proc_macro_derive(GraphQLEnumInternal, attributes(graphql))]
#[doc(hidden)]
pub fn derive_enum_internal(input: TokenStream) -> TokenStream {
    let ast = syn::parse::<syn::DeriveInput>(input).unwrap();
    let gen = derive_enum::impl_enum(&ast, true);
    gen.into()
}

#[proc_macro_derive(GraphQLInputObject, attributes(graphql))]
pub fn derive_input_object(input: TokenStream) -> TokenStream {
    let ast = syn::parse::<syn::DeriveInput>(input).unwrap();
    let gen = derive_input_object::impl_input_object(&ast, false);
    gen.into()
}

#[proc_macro_derive(GraphQLInputObjectInternal, attributes(graphql))]
#[doc(hidden)]
pub fn derive_input_object_internal(input: TokenStream) -> TokenStream {
    let ast = syn::parse::<syn::DeriveInput>(input).unwrap();
    let gen = derive_input_object::impl_input_object(&ast, true);
    gen.into()
}

#[proc_macro_derive(GraphQLObject, attributes(graphql))]
pub fn derive_object(input: TokenStream) -> TokenStream {
    let ast = syn::parse::<syn::DeriveInput>(input).unwrap();
    let gen = derive_object::build_derive_object(ast, false);
    gen.into()
}

/// This custom derive macro implements the #[derive(GraphQLScalarValue)]
/// derive.
///
/// This can be used for two purposes.
///
/// ## Transparent Newtype Wrapper
///
/// Sometimes, you want to create a custerm scalar type by wrapping
/// an existing type. In Rust, this is often called the "newtype" pattern.
/// Thanks to this custom derive, this becomes really easy:
///
/// ```rust
/// // Deriving GraphQLScalar is all that is required.
/// #[derive(juniper::GraphQLScalarValue)]
/// struct UserId(String);
///
/// #[derive(juniper::GraphQLObject)]
/// struct User {
///   id: UserId,
/// }
/// ```
///
/// The type can also be customized.
///
/// ```rust
/// /// Doc comments are used for the GraphQL type description.
/// #[derive(juniper::GraphQLScalarValue)]
/// #[graphql(
///    transparent,
///    // Set a custom GraphQL name.
///    name= "MyUserId",
///    // A description can also specified in the attribute.
///    // This will the doc comment, if one exists.
///    description = "...",
/// )]
/// struct UserId(String);
/// ```
///
/// ### Base ScalarValue Enum
///
/// TODO: write documentation.
///
#[proc_macro_derive(GraphQLScalarValue, attributes(graphql))]
pub fn derive_scalar_value(input: TokenStream) -> TokenStream {
    let ast = syn::parse::<syn::DeriveInput>(input).unwrap();
    let gen = derive_scalar_value::impl_scalar_value(&ast, false);
    gen.into()
}

#[deprecated(note = "ScalarValue has been renamed to GraphQLScalarValue")]
#[proc_macro_derive(ScalarValue)]
pub fn derive_scalar_value_deprecated(input: TokenStream) -> TokenStream {
    derive_scalar_value(input)
}

#[proc_macro_derive(GraphQLScalarValueInternal)]
#[doc(hidden)]
pub fn derive_scalar_value_internal(input: TokenStream) -> TokenStream {
    let ast = syn::parse::<syn::DeriveInput>(input).unwrap();
    let gen = derive_scalar_value::impl_scalar_value(&ast, true);
    gen.into()
}

/**
The `object` proc macro is the primary way of defining GraphQL resolvers
that can not be implemented with the GraphQLObject derive.

It enables you to write GraphQL field resolvers for a type by declaring a
regular Rust `impl` block. Under the hood, the procedural macro implements
the GraphQLType trait.

`object` comes with many features that allow customization of
your fields, all of which are detailed below.

### Getting Started

This simple example will show you the most basic use of `object`.
More advanced use cases are introduced step by step.

```
// So we can declare it as a plain struct without any members.
struct Query;

// We prefix the impl Block with the procedural macro.
#[juniper::object]
impl Query {

    // A **warning**: only GraphQL fields can be specified in this impl block.
    // If you want to define normal methods on the struct,
    // you have to do so in a separate, normal `impl` block.


    // This defines a simple, static field which does not require any context.
    // You can return any value that implements the `GraphQLType` trait.
    // This trait is implemented for:
    //  - basic scalar types like bool, &str, String, i32, f64
    //  - GraphQL compatible wrappers like Option<_>, Vec<_>.
    //  - types which use the `#derive[juniper::GraphQLObject]`
    //  - `object` structs.
    //
    // An important note regarding naming:
    // By default, field names will be converted to camel case.
    // For your GraphQL queries, the field will be available as `apiVersion`.
    //
    // You can also manually customize the field name if required. (See below)
    fn api_version() -> &'static str {
        "0.1"
    }

    // This field takes two arguments.
    // GraphQL arguments are just regular function parameters.
    // **Note**: in Juniper, arguments are non-nullable by default.
    //           for optional arguments, you have to specify them with Option<T>.
    fn add(a: f64, b: f64, c: Option<f64>) -> f64 {
        a + b + c.unwrap_or(0.0)
    }
}
```

## Accessing self

```
struct Person {
    first_name: String,
    last_name: String,
}

impl Person {
    // The full name method is useful outside of GraphQL,
    // so we define it as a normal method.
    fn build_full_name(&self) -> String {
        format!("{} {}", self.first_name, self.last_name)
    }
}

#[juniper::object]
impl Person {
    fn first_name(&self) -> &str {
        &self.first_name
    }

    fn last_name(&self) -> &str {
        &self.last_name
    }

    fn full_name(&self) -> String {
        self.build_full_name()
    }
}
```

## Context (+ Executor)

You can specify a context that will be available across
all your resolvers during query execution.

The Context can be injected into your resolvers by just
specifying an argument with the same type as the context
(but as a reference).

```

# #[derive(juniper::GraphQLObject)] struct User { id: i32 }
# struct DbPool;
# impl DbPool { fn user(&self, id: i32) -> Option<User> { unimplemented!() } }

struct Context {
    db: DbPool,
}

// Mark our struct for juniper.
impl juniper::Context for Context {}

struct Query;

#[juniper::object(
    // Here we specify the context type for this object.
    Context = Context,
)]
impl Query {
    // Context is injected by specifying a argument
    // as a reference to the Context.
    fn user(context: &Context, id: i32) -> Option<User> {
        context.db.user(id)
    }

    // You can also gain access to the executor, which
    // allows you to do look aheads.
    fn with_executor(executor: &Executor) -> bool {
        let info = executor.look_ahead();
        // ...
        true
    }
}

```

## Customization (Documentation, Renaming, ...)

```
struct InternalQuery;

// Doc comments can be used to specify graphql documentation.
/// GRAPHQL DOCUMENTATION.
/// More info for GraphQL users....
#[juniper::object(
    // You can rename the type for GraphQL by specifying the name here.
    name = "Query",
    // You can also specify a description here.
    // If present, doc comments will be ignored.
    description = "...",
)]
impl InternalQuery {
    // Documentation doc comments also work on fields.
    /// GraphQL description...
    fn field_with_description() -> bool { true }

    // Fields can also be customized with the #[graphql] attribute.
    #[graphql(
        // overwrite the public name
        name = "actualFieldName",
        // Can be used instead of doc comments.
        description = "field description",
    )]
    fn internal_name() -> bool { true }

    // Fields can be deprecated too.
    #[graphql(
        deprecated = "deprecatin info...",
        // Note: just "deprecated," without a description works too.
    )]
    fn deprecated_field_simple() -> bool { true }


    // Customizing field arguments can be done like so:
    fn args(
        #[graphql(
            description = "Argument description....",

            // You can specify default values.
            // A default can be any valid expression that yields the right type.
            default = true,

            // You can also give the argument a different name in your GraphQL schema
            name = newName
        )] arg1: bool,

        #[graphql(
            description = "arg2 description...",
            default = false,
        )] arg2: bool,
    ) -> bool {
        arg1 && arg2
    }
}
```

## Lifetimes, Generics and custom Scalars

Lifetimes work just like you'd expect.


```
struct WithLifetime<'a> {
    value: &'a str,
}

#[juniper::object]
impl<'a> WithLifetime<'a> {
    fn value(&self) -> &str {
        self.value
    }
}

```

Juniper has support for custom scalars.
Mostly you will only need the default scalar type juniper::DefaultScalarValue.

You can easily specify a custom scalar though.


```

# type MyCustomScalar = juniper::DefaultScalarValue;

struct Query;

#[juniper::object(
    Scalar = MyCustomScalar,
)]
impl Query {
    // ...
}
```

## Raw identifiers

You can use [raw identifiers](https://doc.rust-lang.org/stable/edition-guide/rust-2018/module-system/raw-identifiers.html)
if you want a GrahpQL field that happens to be a Rust keyword:

```
struct User {
    r#type: String,
}

#[juniper::object]
impl User {
    fn r#type(&self) -> &str {
        &self.r#type
    }
}
```

*/
#[proc_macro_attribute]
#[proc_macro_error]
pub fn object(args: TokenStream, input: TokenStream) -> TokenStream {
    let gen = impl_object::build_object(args, input, false);
    gen.into()
}

/// A proc macro for defining a GraphQL object.
#[doc(hidden)]
#[proc_macro_attribute]
#[proc_macro_error]
pub fn object_internal(args: TokenStream, input: TokenStream) -> TokenStream {
    let gen = impl_object::build_object(args, input, true);
    gen.into()
}
