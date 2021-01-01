/*!

# juniper_warp

This repository contains the [warp][warp] web server integration for
[Juniper][Juniper], a [GraphQL][GraphQL] implementation for Rust.

## Documentation

For documentation, including guides and examples, check out [Juniper][Juniper].

A basic usage example can also be found in the [Api documentation][documentation].

## Examples

Check [examples/warp_server][example] for example code of a working warp
server with GraphQL handlers.

## Links

* [Juniper][Juniper]
* [Api Reference][documentation]
* [warp][warp]

## License

This project is under the BSD-2 license.

Check the LICENSE file for details.

[warp]: https://github.com/seanmonstar/warp
[Juniper]: https://github.com/graphql-rust/juniper
[GraphQL]: http://graphql.org
[documentation]: https://docs.rs/juniper_warp
[example]: https://github.com/graphql-rust/juniper/blob/master/juniper_warp/examples/warp_server

*/

#![deny(missing_docs)]
#![deny(warnings)]
#![doc(html_root_url = "https://docs.rs/juniper_warp/0.2.0")]

use anyhow::anyhow;
use bytes::Bytes;
use futures::{FutureExt as _, TryFutureExt};
use juniper::{
    http::{GraphQLBatchRequest, GraphQLRequest},
    ScalarValue,
};
use std::{collections::HashMap, str, sync::Arc};
use tokio::task;
use warp::{body, filters::BoxedFilter, http, query, Filter};

/// Make a filter for graphql queries/mutations.
///
/// The `schema` argument is your juniper schema.
///
/// The `context_extractor` argument should be a filter that provides the GraphQL context required by the schema.
///
/// In order to avoid blocking, this helper will use the `tokio_threadpool` threadpool created by hyper to resolve GraphQL requests.
///
/// Example:
///
/// ```
/// # use std::sync::Arc;
/// # use warp::Filter;
/// # use juniper::{graphql_object, EmptyMutation, EmptySubscription, RootNode};
/// # use juniper_warp::make_graphql_filter;
/// #
/// type UserId = String;
/// # #[derive(Debug)]
/// struct AppState(Vec<i64>);
/// struct ExampleContext(Arc<AppState>, UserId);
///
/// struct QueryRoot;
///
/// #[graphql_object(context = ExampleContext)]
/// impl QueryRoot {
///     fn say_hello(context: &ExampleContext) -> String {
///         format!(
///             "good morning {}, the app state is {:?}",
///             context.1,
///             context.0
///         )
///     }
/// }
///
/// let schema = RootNode::new(QueryRoot, EmptyMutation::new(), EmptySubscription::new());
///
/// let app_state = Arc::new(AppState(vec![3, 4, 5]));
/// let app_state = warp::any().map(move || app_state.clone());
///
/// let context_extractor = warp::any()
///     .and(warp::header::<String>("authorization"))
///     .and(app_state)
///     .map(|auth_header: String, app_state: Arc<AppState>| {
///         let user_id = auth_header; // we believe them
///         ExampleContext(app_state, user_id)
///     })
///     .boxed();
///
/// let graphql_filter = make_graphql_filter(schema, context_extractor);
///
/// let graphql_endpoint = warp::path("graphql")
///     .and(warp::post())
///     .and(graphql_filter);
/// ```
pub fn make_graphql_filter<Query, Mutation, Subscription, CtxT, S>(
    schema: juniper::RootNode<'static, Query, Mutation, Subscription, S>,
    context_extractor: BoxedFilter<(CtxT,)>,
) -> BoxedFilter<(http::Response<Vec<u8>>,)>
where
    Query: juniper::GraphQLTypeAsync<S, Context = CtxT> + Send + 'static,
    Query::TypeInfo: Send + Sync,
    Mutation: juniper::GraphQLTypeAsync<S, Context = CtxT> + Send + 'static,
    Mutation::TypeInfo: Send + Sync,
    Subscription: juniper::GraphQLSubscriptionType<S, Context = CtxT> + Send + 'static,
    Subscription::TypeInfo: Send + Sync,
    CtxT: Send + Sync + 'static,
    S: ScalarValue + Send + Sync + 'static,
{
    let schema = Arc::new(schema);
    let post_json_schema = schema.clone();
    let post_graphql_schema = schema.clone();

    let handle_post_json_request = move |context: CtxT, req: GraphQLBatchRequest<S>| {
        let schema = post_json_schema.clone();

        tokio::task::spawn_local(async move {
            let resp = req.execute(&schema, &context).await;

            Ok::<_, warp::Rejection>(build_response(
                serde_json::to_vec(&resp)
                    .map(|json| (json, resp.is_ok()))
                    .map_err(Into::into),
            ))
        })
        .unwrap_or_else(|e| Err(warp::reject::custom(JoinError(e))))
    };
    let post_json_filter = warp::post()
        .and(context_extractor.clone())
        .and(body::json())
        .and_then(handle_post_json_request);

    let handle_post_graphql_request = move |context: CtxT, body: Bytes| {
        let schema = post_graphql_schema.clone();
        tokio::task::spawn_local(
            async move {
                let query = str::from_utf8(body.as_ref()).map_err(|e| {
                    anyhow!("Request body query is not a valid UTF-8 string: {}", e)
                })?;
                let req = GraphQLRequest::new(query.into(), None, None);

                let resp = req.execute(&schema, &context).await;

                Ok((serde_json::to_vec(&resp)?, resp.is_ok()))
            }
            .then(|res| async { Ok::<_, warp::Rejection>(build_response(res)) }),
        )
        .unwrap_or_else(|e| Err(warp::reject::custom(JoinError(e))))
    };
    let post_graphql_filter = warp::post()
        .and(context_extractor.clone())
        .and(body::bytes())
        .and_then(handle_post_graphql_request);

    let handle_get_request = move |context: CtxT, mut qry: HashMap<String, String>| {
        let schema = schema.clone();
        tokio::task::spawn_local(
            async move {
                let req = GraphQLRequest::new(
                    qry.remove("query").ok_or_else(|| {
                        anyhow!("Missing GraphQL query string in query parameters")
                    })?,
                    qry.remove("operation_name"),
                    qry.remove("variables")
                        .map(|vs| serde_json::from_str(&vs))
                        .transpose()?,
                );

                let resp = req.execute(&schema, &context).await;

                Ok((serde_json::to_vec(&resp)?, resp.is_ok()))
            }
            .then(|res| async move { Ok::<_, warp::Rejection>(build_response(res)) }),
        )
        .unwrap_or_else(|e| Err(warp::reject::custom(JoinError(e))))
    };
    let get_filter = warp::get()
        .and(context_extractor)
        .and(query::query())
        .and_then(handle_get_request);

    get_filter
        .or(post_json_filter)
        .unify()
        .or(post_graphql_filter)
        .unify()
        .boxed()
}

/// Make a synchronous filter for graphql endpoint.
pub fn make_graphql_filter_sync<Query, Mutation, Subscription, CtxT, S>(
    schema: juniper::RootNode<'static, Query, Mutation, Subscription, S>,
    context_extractor: BoxedFilter<(CtxT,)>,
) -> BoxedFilter<(http::Response<Vec<u8>>,)>
where
    Query: juniper::GraphQLType<S, Context = CtxT, TypeInfo = ()> + Send + Sync + 'static,
    Mutation: juniper::GraphQLType<S, Context = CtxT, TypeInfo = ()> + Send + Sync + 'static,
    Subscription: juniper::GraphQLType<S, Context = CtxT, TypeInfo = ()> + Send + Sync + 'static,
    CtxT: Send + Sync + 'static,
    S: ScalarValue + Send + Sync + 'static,
{
    let schema = Arc::new(schema);
    let post_json_schema = schema.clone();
    let post_graphql_schema = schema.clone();

    let handle_post_json_request = move |context: CtxT, req: GraphQLBatchRequest<S>| {
        let schema = post_json_schema.clone();
        async move {
            let res = task::spawn_blocking(move || {
                let resp = req.execute_sync(&schema, &context);
                Ok((serde_json::to_vec(&resp)?, resp.is_ok()))
            })
            .await?;

            Ok(build_response(res))
        }
        .map_err(|e: task::JoinError| warp::reject::custom(JoinError(e)))
    };
    let post_json_filter = warp::post()
        .and(context_extractor.clone())
        .and(body::json())
        .and_then(handle_post_json_request);

    let handle_post_graphql_request = move |context: CtxT, body: Bytes| {
        let schema = post_graphql_schema.clone();
        async move {
            let res = task::spawn_blocking(move || {
                let query = str::from_utf8(body.as_ref())
                    .map_err(|e| anyhow!("Request body is not a valid UTF-8 string: {}", e))?;
                let req = GraphQLRequest::new(query.into(), None, None);

                let resp = req.execute_sync(&schema, &context);
                Ok((serde_json::to_vec(&resp)?, resp.is_ok()))
            })
            .await?;

            Ok(build_response(res))
        }
        .map_err(|e: task::JoinError| warp::reject::custom(JoinError(e)))
    };
    let post_graphql_filter = warp::post()
        .and(context_extractor.clone())
        .and(body::bytes())
        .and_then(handle_post_graphql_request);

    let handle_get_request = move |context: CtxT, mut qry: HashMap<String, String>| {
        let schema = schema.clone();
        async move {
            let res = task::spawn_blocking(move || {
                let req = GraphQLRequest::new(
                    qry.remove("query").ok_or_else(|| {
                        anyhow!("Missing GraphQL query string in query parameters")
                    })?,
                    qry.remove("operation_name"),
                    qry.remove("variables")
                        .map(|vs| serde_json::from_str(&vs))
                        .transpose()?,
                );

                let resp = req.execute_sync(&schema, &context);
                Ok((serde_json::to_vec(&resp)?, resp.is_ok()))
            })
            .await?;

            Ok(build_response(res))
        }
        .map_err(|e: task::JoinError| warp::reject::custom(JoinError(e)))
    };
    let get_filter = warp::get()
        .and(context_extractor)
        .and(query::query())
        .and_then(handle_get_request);

    get_filter
        .or(post_json_filter)
        .unify()
        .or(post_graphql_filter)
        .unify()
        .boxed()
}

/// Error raised by `tokio_threadpool` if the thread pool has been shutdown.
///
/// Wrapper type is needed as inner type does not implement `warp::reject::Reject`.
#[derive(Debug)]
pub struct JoinError(task::JoinError);

impl warp::reject::Reject for JoinError {}

fn build_response(response: Result<(Vec<u8>, bool), anyhow::Error>) -> http::Response<Vec<u8>> {
    match response {
        Ok((body, is_ok)) => http::Response::builder()
            .status(if is_ok { 200 } else { 400 })
            .header("content-type", "application/json")
            .body(body)
            .expect("response is valid"),
        Err(_) => http::Response::builder()
            .status(http::StatusCode::INTERNAL_SERVER_ERROR)
            .body(Vec::new())
            .expect("status code is valid"),
    }
}

/// Create a filter that replies with an HTML page containing GraphiQL. This does not handle routing, so you can mount it on any endpoint.
///
/// For example:
///
/// ```
/// # use warp::Filter;
/// # use juniper_warp::graphiql_filter;
/// #
/// let graphiql_route = warp::path("graphiql").and(graphiql_filter("/graphql",
/// None));
/// ```
///
/// Or with subscriptions support, provide the subscriptions endpoint URL:
///
/// ```
/// # use warp::Filter;
/// # use juniper_warp::graphiql_filter;
/// #
/// let graphiql_route = warp::path("graphiql").and(graphiql_filter("/graphql",
/// Some("ws://localhost:8080/subscriptions")));
/// ```
pub fn graphiql_filter(
    graphql_endpoint_url: &'static str,
    subscriptions_endpoint: Option<&'static str>,
) -> warp::filters::BoxedFilter<(http::Response<Vec<u8>>,)> {
    warp::any()
        .map(move || graphiql_response(graphql_endpoint_url, subscriptions_endpoint))
        .boxed()
}

fn graphiql_response(
    graphql_endpoint_url: &'static str,
    subscriptions_endpoint: Option<&'static str>,
) -> http::Response<Vec<u8>> {
    http::Response::builder()
        .header("content-type", "text/html;charset=utf-8")
        .body(
            juniper::http::graphiql::graphiql_source(graphql_endpoint_url, subscriptions_endpoint)
                .into_bytes(),
        )
        .expect("response is valid")
}

/// Create a filter that replies with an HTML page containing GraphQL Playground. This does not handle routing, so you can mount it on any endpoint.
pub fn playground_filter(
    graphql_endpoint_url: &'static str,
    subscriptions_endpoint_url: Option<&'static str>,
) -> warp::filters::BoxedFilter<(http::Response<Vec<u8>>,)> {
    warp::any()
        .map(move || playground_response(graphql_endpoint_url, subscriptions_endpoint_url))
        .boxed()
}

fn playground_response(
    graphql_endpoint_url: &'static str,
    subscriptions_endpoint_url: Option<&'static str>,
) -> http::Response<Vec<u8>> {
    http::Response::builder()
        .header("content-type", "text/html;charset=utf-8")
        .body(
            juniper::http::playground::playground_source(
                graphql_endpoint_url,
                subscriptions_endpoint_url,
            )
            .into_bytes(),
        )
        .expect("response is valid")
}

/// `juniper_warp` subscriptions handler implementation.
/// Cannot be merged to `juniper_warp` yet as GraphQL over WS[1]
/// is not fully supported in current implementation.
///
/// *Note: this implementation is in an alpha state.*
///
/// [1]: https://github.com/apollographql/subscriptions-transport-ws/blob/master/PROTOCOL.md
#[cfg(feature = "subscriptions")]
pub mod subscriptions {
    use juniper::{
        futures::{
            future::{self, Either},
            sink::SinkExt,
            stream::StreamExt,
        },
        GraphQLSubscriptionType, GraphQLTypeAsync, RootNode, ScalarValue,
    };
    use juniper_graphql_ws::{ArcSchema, ClientMessage, Connection, Init};
    use std::{convert::Infallible, fmt, sync::Arc};

    struct Message(warp::ws::Message);

    impl<S: ScalarValue> std::convert::TryFrom<Message> for ClientMessage<S> {
        type Error = serde_json::Error;

        fn try_from(msg: Message) -> serde_json::Result<Self> {
            serde_json::from_slice(msg.0.as_bytes())
        }
    }

    /// Errors that can happen while serving a connection.
    #[derive(Debug)]
    pub enum Error {
        /// Errors that can happen in Warp while serving a connection.
        Warp(warp::Error),

        /// Errors that can happen while serializing outgoing messages. Note that errors that occur
        /// while deserializing incoming messages are handled internally by the protocol.
        Serde(serde_json::Error),
    }

    impl fmt::Display for Error {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                Self::Warp(e) => write!(f, "warp error: {}", e),
                Self::Serde(e) => write!(f, "serde error: {}", e),
            }
        }
    }

    impl std::error::Error for Error {}

    impl From<warp::Error> for Error {
        fn from(err: warp::Error) -> Self {
            Self::Warp(err)
        }
    }

    impl From<Infallible> for Error {
        fn from(_err: Infallible) -> Self {
            unreachable!()
        }
    }

    /// Serves the graphql-ws protocol over a WebSocket connection.
    ///
    /// The `init` argument is used to provide the context and additional configuration for
    /// connections. This can be a `juniper_graphql_ws::ConnectionConfig` if the context and
    /// configuration are already known, or it can be a closure that gets executed asynchronously
    /// when the client sends the ConnectionInit message. Using a closure allows you to perform
    /// authentication based on the parameters provided by the client.
    pub async fn serve_graphql_ws<Query, Mutation, Subscription, CtxT, S, I>(
        websocket: warp::ws::WebSocket,
        root_node: Arc<RootNode<'static, Query, Mutation, Subscription, S>>,
        init: I,
    ) -> Result<(), Error>
    where
        Query: GraphQLTypeAsync<S, Context = CtxT> + Send + 'static,
        Query::TypeInfo: Send + Sync,
        Mutation: GraphQLTypeAsync<S, Context = CtxT> + Send + 'static,
        Mutation::TypeInfo: Send + Sync,
        Subscription: GraphQLSubscriptionType<S, Context = CtxT> + Send + 'static,
        Subscription::TypeInfo: Send + Sync,
        CtxT: Unpin + 'static,
        S: ScalarValue + Send + Sync + 'static,
        I: Init<S, CtxT> + Send,
    {
        let (ws_tx, ws_rx) = websocket.split();
        let (s_tx, s_rx) = Connection::new(ArcSchema(root_node), init).split();

        let ws_rx = ws_rx.map(|r| r.map(|msg| Message(msg)));
        let s_rx = s_rx.map(|msg| {
            serde_json::to_string(&msg)
                .map(|t| warp::ws::Message::text(t))
                .map_err(|e| Error::Serde(e))
        });

        match future::select(
            ws_rx.forward(s_tx.sink_err_into()),
            s_rx.forward(ws_tx.sink_err_into()),
        )
        .await
        {
            Either::Left((r, _)) => r.map_err(|e| e.into()),
            Either::Right((r, _)) => r,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use warp::{http, test::request};

    #[test]
    fn graphiql_response_does_not_panic() {
        graphiql_response("/abcd", None);
    }

    #[tokio::test]
    async fn graphiql_endpoint_matches() {
        let filter = warp::get()
            .and(warp::path("graphiql"))
            .and(graphiql_filter("/graphql", None));
        let result = request()
            .method("GET")
            .path("/graphiql")
            .header("accept", "text/html")
            .filter(&filter)
            .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn graphiql_endpoint_returns_graphiql_source() {
        let filter = warp::get()
            .and(warp::path("dogs-api"))
            .and(warp::path("graphiql"))
            .and(graphiql_filter("/dogs-api/graphql", None));
        let response = request()
            .method("GET")
            .path("/dogs-api/graphiql")
            .header("accept", "text/html")
            .reply(&filter)
            .await;

        assert_eq!(response.status(), http::StatusCode::OK);
        assert_eq!(
            response.headers().get("content-type").unwrap(),
            "text/html;charset=utf-8"
        );
        let body = String::from_utf8(response.body().to_vec()).unwrap();

        assert!(body.contains("<script>var GRAPHQL_URL = '/dogs-api/graphql';</script>"));
    }

    #[tokio::test]
    async fn graphiql_endpoint_with_subscription_matches() {
        let filter = warp::get().and(warp::path("graphiql")).and(graphiql_filter(
            "/graphql",
            Some("ws:://localhost:8080/subscriptions"),
        ));
        let result = request()
            .method("GET")
            .path("/graphiql")
            .header("accept", "text/html")
            .filter(&filter)
            .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn playground_endpoint_matches() {
        let filter = warp::get()
            .and(warp::path("playground"))
            .and(playground_filter("/graphql", Some("/subscripitons")));

        let result = request()
            .method("GET")
            .path("/playground")
            .header("accept", "text/html")
            .filter(&filter)
            .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn playground_endpoint_returns_playground_source() {
        let filter = warp::get()
            .and(warp::path("dogs-api"))
            .and(warp::path("playground"))
            .and(playground_filter(
                "/dogs-api/graphql",
                Some("/dogs-api/subscriptions"),
            ));
        let response = request()
            .method("GET")
            .path("/dogs-api/playground")
            .header("accept", "text/html")
            .reply(&filter)
            .await;

        assert_eq!(response.status(), http::StatusCode::OK);
        assert_eq!(
            response.headers().get("content-type").unwrap(),
            "text/html;charset=utf-8"
        );
        let body = String::from_utf8(response.body().to_vec()).unwrap();

        assert!(body.contains("GraphQLPlayground.init(root, { endpoint: '/dogs-api/graphql', subscriptionEndpoint: '/dogs-api/subscriptions' })"));
    }

    #[tokio::test]
    async fn graphql_handler_works_json_post() {
        use juniper::{
            tests::fixtures::starwars::schema::{Database, Query},
            EmptyMutation, EmptySubscription, RootNode,
        };

        type Schema =
            juniper::RootNode<'static, Query, EmptyMutation<Database>, EmptySubscription<Database>>;

        let schema: Schema = RootNode::new(
            Query,
            EmptyMutation::<Database>::new(),
            EmptySubscription::<Database>::new(),
        );

        let state = warp::any().map(Database::new);
        let filter = warp::path("graphql2").and(make_graphql_filter(schema, state.boxed()));

        let response = request()
            .method("POST")
            .path("/graphql2")
            .header("accept", "application/json")
            .header("content-type", "application/json")
            .body(r##"{ "variables": null, "query": "{ hero(episode: NEW_HOPE) { name } }" }"##)
            .reply(&filter)
            .await;

        assert_eq!(response.status(), http::StatusCode::OK);
        assert_eq!(
            response.headers().get("content-type").unwrap(),
            "application/json",
        );
        assert_eq!(
            String::from_utf8(response.body().to_vec()).unwrap(),
            r#"{"data":{"hero":{"name":"R2-D2"}}}"#
        );
    }

    #[tokio::test]
    async fn batch_requests_work() {
        use juniper::{
            tests::fixtures::starwars::schema::{Database, Query},
            EmptyMutation, EmptySubscription, RootNode,
        };

        type Schema =
            juniper::RootNode<'static, Query, EmptyMutation<Database>, EmptySubscription<Database>>;

        let schema: Schema = RootNode::new(
            Query,
            EmptyMutation::<Database>::new(),
            EmptySubscription::<Database>::new(),
        );

        let state = warp::any().map(Database::new);
        let filter = warp::path("graphql2").and(make_graphql_filter(schema, state.boxed()));

        let response = request()
            .method("POST")
            .path("/graphql2")
            .header("accept", "application/json")
            .header("content-type", "application/json")
            .body(
                r##"[
                     { "variables": null, "query": "{ hero(episode: NEW_HOPE) { name } }" },
                     { "variables": null, "query": "{ hero(episode: EMPIRE) { id name } }" }
                 ]"##,
            )
            .reply(&filter)
            .await;

        assert_eq!(response.status(), http::StatusCode::OK);
        assert_eq!(
            String::from_utf8(response.body().to_vec()).unwrap(),
            r#"[{"data":{"hero":{"name":"R2-D2"}}},{"data":{"hero":{"id":"1000","name":"Luke Skywalker"}}}]"#
        );
        assert_eq!(
            response.headers().get("content-type").unwrap(),
            "application/json",
        );
    }

    #[test]
    fn batch_request_deserialization_can_fail() {
        let json = r#"blah"#;
        let result: Result<GraphQLBatchRequest, _> = serde_json::from_str(json);

        assert!(result.is_err());
    }
}

#[cfg(test)]
mod tests_http_harness {
    use juniper::{
        http::tests::{run_http_test_suite, HttpIntegration, TestResponse},
        tests::fixtures::starwars::schema::{Database, Query},
        EmptyMutation, EmptySubscription, RootNode,
    };
    use warp::{
        filters::{path, BoxedFilter},
        Filter,
    };

    use super::*;

    struct TestWarpIntegration {
        filter: BoxedFilter<(http::Response<Vec<u8>>,)>,
    }

    impl TestWarpIntegration {
        fn new(is_sync: bool) -> Self {
            let schema = RootNode::new(
                Query,
                EmptyMutation::<Database>::new(),
                EmptySubscription::<Database>::new(),
            );
            let state = warp::any().map(move || Database::new());

            let filter = path::end().and(if is_sync {
                make_graphql_filter_sync(schema, state.boxed())
            } else {
                make_graphql_filter(schema, state.boxed())
            });
            Self {
                filter: filter.boxed(),
            }
        }

        fn make_request(&self, req: warp::test::RequestBuilder) -> TestResponse {
            let mut rt = tokio::runtime::Runtime::new().expect("Failed to create tokio::Runtime");
            make_test_response(rt.block_on(async move {
                req.filter(&self.filter).await.unwrap_or_else(|rejection| {
                    let code = if rejection.is_not_found() {
                        http::StatusCode::NOT_FOUND
                    } else if let Some(body::BodyDeserializeError { .. }) = rejection.find() {
                        http::StatusCode::BAD_REQUEST
                    } else {
                        http::StatusCode::INTERNAL_SERVER_ERROR
                    };
                    http::Response::builder()
                        .status(code)
                        .header("content-type", "application/json")
                        .body(Vec::new())
                        .unwrap()
                })
            }))
        }
    }

    impl HttpIntegration for TestWarpIntegration {
        fn get(&self, url: &str) -> TestResponse {
            use percent_encoding::{utf8_percent_encode, AsciiSet, CONTROLS};
            use url::Url;

            /// https://url.spec.whatwg.org/#query-state
            const QUERY_ENCODE_SET: &AsciiSet =
                &CONTROLS.add(b' ').add(b'"').add(b'#').add(b'<').add(b'>');

            let url = Url::parse(&format!("http://localhost:3000{}", url)).expect("url to parse");

            let url: String = utf8_percent_encode(url.query().unwrap_or(""), QUERY_ENCODE_SET)
                .into_iter()
                .collect::<Vec<_>>()
                .join("");

            self.make_request(
                warp::test::request()
                    .method("GET")
                    .path(&format!("/?{}", url)),
            )
        }

        fn post_json(&self, url: &str, body: &str) -> TestResponse {
            self.make_request(
                warp::test::request()
                    .method("POST")
                    .header("content-type", "application/json; charset=utf-8")
                    .path(url)
                    .body(body),
            )
        }

        fn post_graphql(&self, url: &str, body: &str) -> TestResponse {
            self.make_request(
                warp::test::request()
                    .method("POST")
                    .header("content-type", "application/graphql; charset=utf-8")
                    .path(url)
                    .body(body),
            )
        }
    }

    fn make_test_response(resp: http::Response<Vec<u8>>) -> TestResponse {
        TestResponse {
            status_code: resp.status().as_u16() as i32,
            body: Some(String::from_utf8(resp.body().to_owned()).unwrap()),
            content_type: resp
                .headers()
                .get("content-type")
                .expect("missing content-type header in warp response")
                .to_str()
                .expect("invalid content-type string")
                .to_owned(),
        }
    }

    #[test]
    fn test_warp_integration() {
        run_http_test_suite(&TestWarpIntegration::new(false));
    }

    #[test]
    fn test_sync_warp_integration() {
        run_http_test_suite(&TestWarpIntegration::new(true));
    }
}
