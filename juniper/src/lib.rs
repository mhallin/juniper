/*!

# GraphQL

[GraphQL][graphql] is a data query language developed by Facebook intended to
serve mobile and web application frontends.

*Juniper* makes it possible to write GraphQL servers in Rust that are
type-safe and blazingly fast. We also try to make declaring and resolving
GraphQL schemas as convenient as possible as Rust will allow.

Juniper does not include a web server - instead it provides building blocks to
make integration with existing servers straightforward. It optionally provides a
pre-built integration for the [Iron][iron] and [Rocket] frameworks, including
embedded [Graphiql][graphiql] for easy debugging.

* [Cargo crate](https://crates.io/crates/juniper)
* [API Reference][docsrs]
* [Book][book]: Guides and Examples


## Getting Started

The best place to get started is the [Juniper Book][book], which contains
guides with plenty of examples, covering all features of Juniper.

To get started quickly and get a feel for Juniper, check out the
[Quickstart][book_quickstart] section.

For specific information about macros, types and the Juniper api, the
[API Reference][docsrs] is the best place to look.

You can also check out [src/tests/schema.rs][test_schema_rs] to see a complex
schema including polymorphism with traits and interfaces.
For an example of web framework integration,
see the [rocket][rocket_examples] and [iron][iron_examples] examples folders.


## Features

Juniper supports the full GraphQL query language according to the
[specification][graphql_spec], including interfaces, unions, schema
introspection, and validations.
It does not, however, support the schema language.

As an exception to other GraphQL libraries for other languages, Juniper builds
non-null types by default. A field of type `Vec<Episode>` will be converted into
`[Episode!]!`. The corresponding Rust type for e.g. `[Episode]` would be
`Option<Vec<Option<Episode>>>`.

## Integrations

### Data types

Juniper has automatic integration with some very common Rust crates to make
building schemas a breeze. The types from these crates will be usable in
your Schemas automatically.

* [uuid][uuid]
* [url][url]
* [chrono][chrono]
* [bson][bson]

### Web Frameworks

* [rocket][rocket]
* [iron][iron]


## API Stability

Juniper has not reached 1.0 yet, thus some API instability should be expected.

[graphql]: http://graphql.org
[graphiql]: https://github.com/graphql/graphiql
[iron]: http://ironframework.io
[graphql_spec]: http://facebook.github.io/graphql
[test_schema_rs]: https://github.com/graphql-rust/juniper/blob/master/juniper/src/tests/schema.rs
[tokio]: https://github.com/tokio-rs/tokio
[rocket_examples]: https://github.com/graphql-rust/juniper/tree/master/juniper_rocket/examples
[iron_examples]: https://github.com/graphql-rust/juniper/tree/master/juniper_iron/examples
[Rocket]: https://rocket.rs
[book]: https://graphql-rust.github.io/
[book_quickstart]: https://graphql-rust.github.io/quickstart.html
[docsrs]: https://docs.rs/juniper

[uuid]: https://crates.io/crates/uuid
[url]: https://crates.io/crates/url
[chrono]: https://crates.io/crates/chrono
[bson]: https://crates.io/crates/bson

*/
#![doc(html_root_url = "https://docs.rs/juniper/0.15.1")]
#![warn(missing_docs)]

// Required for using `juniper_codegen` macros inside this crate to resolve absolute `::juniper`
// path correctly, without errors.
extern crate self as juniper;

use std::fmt;

// These are required by the code generated via the `juniper_codegen` macros.
#[doc(hidden)]
pub use {async_trait::async_trait, futures, serde, static_assertions as sa};

#[doc(inline)]
pub use futures::future::{BoxFuture, LocalBoxFuture};

// Depend on juniper_codegen and re-export everything in it.
// This allows users to just depend on juniper and get the derive
// functionality automatically.
pub use juniper_codegen::{
    graphql_interface, graphql_object, graphql_scalar, graphql_subscription, graphql_union,
    GraphQLEnum, GraphQLInputObject, GraphQLObject, GraphQLScalarValue, GraphQLUnion,
};

#[macro_use]
mod value;
#[macro_use]
mod macros;
mod ast;
mod executor;
mod introspection;
pub mod parser;
pub(crate) mod schema;
mod types;
mod util;
mod validation;
// This needs to be public until docs have support for private modules:
// https://github.com/rust-lang/cargo/issues/1520
pub mod http;
pub mod integrations;

#[cfg(all(test, not(feature = "expose-test-schema")))]
mod tests;
#[cfg(feature = "expose-test-schema")]
pub mod tests;

#[cfg(test)]
mod executor_tests;

// Needs to be public because macros use it.
pub use crate::util::to_camel_case;

use crate::{
    executor::{execute_validated_query, get_operation},
    introspection::{INTROSPECTION_QUERY, INTROSPECTION_QUERY_WITHOUT_DESCRIPTIONS},
    parser::{parse_document_source, ParseError, Spanning},
    validation::{validate_input_values, visit_all_rules, ValidatorContext},
};

pub use crate::{
    ast::{FromInputValue, InputValue, Selection, ToInputValue, Type},
    executor::{
        Applies, Context, ExecutionError, ExecutionResult, Executor, FieldError, FieldResult,
        FromContext, IntoFieldError, IntoResolvable, LookAheadArgument, LookAheadMethods,
        LookAheadSelection, LookAheadValue, OwnedExecutor, Registry, ValuesStream, Variables,
    },
    introspection::IntrospectionFormat,
    macros::helper::{
        subscription::{ExtractTypeFromStream, IntoFieldResult},
        AsDynGraphQLValue,
    },
    schema::{
        meta,
        model::{RootNode, SchemaType},
    },
    types::{
        async_await::{DynGraphQLValueAsync, GraphQLTypeAsync, GraphQLValueAsync},
        base::{Arguments, DynGraphQLValue, GraphQLType, GraphQLValue, TypeKind},
        marker::{self, GraphQLInterface, GraphQLUnion},
        nullable::Nullable,
        scalars::{EmptyMutation, EmptySubscription, ID},
        subscriptions::{
            ExecutionOutput, GraphQLSubscriptionType, GraphQLSubscriptionValue,
            SubscriptionConnection, SubscriptionCoordinator,
        },
    },
    validation::RuleError,
    value::{DefaultScalarValue, Object, ParseScalarResult, ParseScalarValue, ScalarValue, Value},
};

/// An error that prevented query execution
#[derive(Debug, PartialEq)]
#[allow(missing_docs)]
pub enum GraphQLError<'a> {
    ParseError(Spanning<ParseError<'a>>),
    ValidationError(Vec<RuleError>),
    NoOperationProvided,
    MultipleOperationsProvided,
    UnknownOperationName,
    IsSubscription,
    NotSubscription,
}

impl<'a> fmt::Display for GraphQLError<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            GraphQLError::ParseError(error) => write!(f, "{}", error),
            GraphQLError::ValidationError(errors) => {
                for error in errors {
                    writeln!(f, "{}", error)?;
                }
                Ok(())
            }
            GraphQLError::NoOperationProvided => write!(f, "No operation provided"),
            GraphQLError::MultipleOperationsProvided => write!(f, "Multiple operations provided"),
            GraphQLError::UnknownOperationName => write!(f, "Unknown operation name"),
            GraphQLError::IsSubscription => write!(f, "Operation is a subscription"),
            GraphQLError::NotSubscription => write!(f, "Operation is not a subscription"),
        }
    }
}

impl<'a> std::error::Error for GraphQLError<'a> {}

/// Execute a query synchronously in a provided schema
pub fn execute_sync<'a, S, QueryT, MutationT, SubscriptionT>(
    document_source: &'a str,
    operation_name: Option<&str>,
    root_node: &'a RootNode<QueryT, MutationT, SubscriptionT, S>,
    variables: &Variables<S>,
    context: &QueryT::Context,
) -> Result<(Value<S>, Vec<ExecutionError<S>>), GraphQLError<'a>>
where
    S: ScalarValue,
    QueryT: GraphQLType<S>,
    MutationT: GraphQLType<S, Context = QueryT::Context>,
    SubscriptionT: GraphQLType<S, Context = QueryT::Context>,
{
    let document = parse_document_source(document_source, &root_node.schema)?;

    {
        let mut ctx = ValidatorContext::new(&root_node.schema, &document);
        visit_all_rules(&mut ctx, &document);

        let errors = ctx.into_errors();
        if !errors.is_empty() {
            return Err(GraphQLError::ValidationError(errors));
        }
    }

    let operation = get_operation(&document, operation_name)?;

    {
        let errors = validate_input_values(variables, operation, &root_node.schema);

        if !errors.is_empty() {
            return Err(GraphQLError::ValidationError(errors));
        }
    }

    execute_validated_query(&document, operation, root_node, variables, context)
}

/// Execute a query in a provided schema
pub async fn execute<'a, S, QueryT, MutationT, SubscriptionT>(
    document_source: &'a str,
    operation_name: Option<&str>,
    root_node: &'a RootNode<'a, QueryT, MutationT, SubscriptionT, S>,
    variables: &Variables<S>,
    context: &QueryT::Context,
) -> Result<(Value<S>, Vec<ExecutionError<S>>), GraphQLError<'a>>
where
    QueryT: GraphQLTypeAsync<S>,
    QueryT::TypeInfo: Sync,
    MutationT: GraphQLTypeAsync<S, Context = QueryT::Context>,
    MutationT::TypeInfo: Sync,
    SubscriptionT: GraphQLType<S, Context = QueryT::Context> + Sync,
    SubscriptionT::TypeInfo: Sync,
    S: ScalarValue + Send + Sync,
{
    let document = parse_document_source(document_source, &root_node.schema)?;

    {
        let mut ctx = ValidatorContext::new(&root_node.schema, &document);
        visit_all_rules(&mut ctx, &document);

        let errors = ctx.into_errors();
        if !errors.is_empty() {
            return Err(GraphQLError::ValidationError(errors));
        }
    }

    let operation = get_operation(&document, operation_name)?;

    {
        let errors = validate_input_values(variables, operation, &root_node.schema);

        if !errors.is_empty() {
            return Err(GraphQLError::ValidationError(errors));
        }
    }

    executor::execute_validated_query_async(&document, operation, root_node, variables, context)
        .await
}

/// Resolve subscription into `ValuesStream`
pub async fn resolve_into_stream<'a, S, QueryT, MutationT, SubscriptionT>(
    document_source: &'a str,
    operation_name: Option<&str>,
    root_node: &'a RootNode<'a, QueryT, MutationT, SubscriptionT, S>,
    variables: &Variables<S>,
    context: &'a QueryT::Context,
) -> Result<(Value<ValuesStream<'a, S>>, Vec<ExecutionError<S>>), GraphQLError<'a>>
where
    QueryT: GraphQLTypeAsync<S>,
    QueryT::TypeInfo: Sync,
    MutationT: GraphQLTypeAsync<S, Context = QueryT::Context>,
    MutationT::TypeInfo: Sync,
    SubscriptionT: GraphQLSubscriptionType<S, Context = QueryT::Context>,
    SubscriptionT::TypeInfo: Sync,
    S: ScalarValue + Send + Sync,
{
    let document: crate::ast::Document<'a, S> =
        parse_document_source(document_source, &root_node.schema)?;

    {
        let mut ctx = ValidatorContext::new(&root_node.schema, &document);
        visit_all_rules(&mut ctx, &document);

        let errors = ctx.into_errors();
        if !errors.is_empty() {
            return Err(GraphQLError::ValidationError(errors));
        }
    }

    let operation = get_operation(&document, operation_name)?;

    {
        let errors = validate_input_values(&variables, operation, &root_node.schema);

        if !errors.is_empty() {
            return Err(GraphQLError::ValidationError(errors));
        }
    }

    executor::resolve_validated_subscription(&document, operation, root_node, variables, context)
        .await
}

/// Execute the reference introspection query in the provided schema
pub fn introspect<'a, S, QueryT, MutationT, SubscriptionT>(
    root_node: &'a RootNode<QueryT, MutationT, SubscriptionT, S>,
    context: &QueryT::Context,
    format: IntrospectionFormat,
) -> Result<(Value<S>, Vec<ExecutionError<S>>), GraphQLError<'a>>
where
    S: ScalarValue,
    QueryT: GraphQLType<S>,
    MutationT: GraphQLType<S, Context = QueryT::Context>,
    SubscriptionT: GraphQLType<S, Context = QueryT::Context>,
{
    execute_sync(
        match format {
            IntrospectionFormat::All => INTROSPECTION_QUERY,
            IntrospectionFormat::WithoutDescriptions => INTROSPECTION_QUERY_WITHOUT_DESCRIPTIONS,
        },
        None,
        root_node,
        &Variables::new(),
        context,
    )
}

impl<'a> From<Spanning<ParseError<'a>>> for GraphQLError<'a> {
    fn from(f: Spanning<ParseError<'a>>) -> GraphQLError<'a> {
        GraphQLError::ParseError(f)
    }
}
