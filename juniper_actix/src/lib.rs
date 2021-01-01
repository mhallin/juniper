/*!

# juniper_actix

This repository contains the [actix][actix] web server integration for
[Juniper][Juniper], a [GraphQL][GraphQL] implementation for Rust, its inspired and some parts are copied from [juniper_warp][juniper_warp]

## Documentation

For documentation, including guides and examples, check out [Juniper][Juniper].

A basic usage example can also be found in the [API documentation][documentation].

## Examples

Check [examples/actix_server][example] for example code of a working actix
server with GraphQL handlers.

## Links

* [Juniper][Juniper]
* [API Reference][documentation]
* [actix][actix]

## License

This project is under the BSD-2 license.

Check the LICENSE file for details.

[actix]: https://github.com/actix/actix-web
[Juniper]: https://github.com/graphql-rust/juniper
[GraphQL]: http://graphql.org
[documentation]: https://docs.rs/juniper_actix
[example]: https://github.com/graphql-rust/juniper/blob/master/juniper_actix/examples/actix_server.rs
[juniper_warp]: https://github.com/graphql-rust/juniper/juniper_warp
*/

#![deny(missing_docs)]
#![deny(warnings)]
#![doc(html_root_url = "https://docs.rs/juniper_actix/0.1.0")]

use actix_web::{
    error::{ErrorBadRequest, ErrorMethodNotAllowed, ErrorUnsupportedMediaType},
    http::{header::CONTENT_TYPE, Method},
    web, Error, FromRequest, HttpRequest, HttpResponse,
};
use juniper::{
    http::{
        graphiql::graphiql_source, playground::playground_source, GraphQLBatchRequest,
        GraphQLRequest,
    },
    ScalarValue,
};
use serde::Deserialize;

#[derive(Deserialize, Clone, PartialEq, Debug)]
#[serde(deny_unknown_fields)]
struct GetGraphQLRequest {
    query: String,
    #[serde(rename = "operationName")]
    operation_name: Option<String>,
    variables: Option<String>,
}

impl<S> From<GetGraphQLRequest> for GraphQLRequest<S>
where
    S: ScalarValue,
{
    fn from(get_req: GetGraphQLRequest) -> Self {
        let GetGraphQLRequest {
            query,
            operation_name,
            variables,
        } = get_req;
        let variables = match variables {
            Some(variables) => Some(serde_json::from_str(&variables).unwrap()),
            None => None,
        };
        Self::new(query, operation_name, variables)
    }
}

/// Actix Web GraphQL Handler for GET and POST requests
pub async fn graphql_handler<Query, Mutation, Subscription, CtxT, S>(
    schema: &juniper::RootNode<'static, Query, Mutation, Subscription, S>,
    context: &CtxT,
    req: HttpRequest,
    payload: actix_web::web::Payload,
) -> Result<HttpResponse, Error>
where
    Query: juniper::GraphQLTypeAsync<S, Context = CtxT>,
    Query::TypeInfo: Sync,
    Mutation: juniper::GraphQLTypeAsync<S, Context = CtxT>,
    Mutation::TypeInfo: Sync,
    Subscription: juniper::GraphQLSubscriptionType<S, Context = CtxT>,
    Subscription::TypeInfo: Sync,
    S: ScalarValue + Send + Sync,
{
    match *req.method() {
        Method::POST => post_graphql_handler(schema, context, req, payload).await,
        Method::GET => get_graphql_handler(schema, context, req).await,
        _ => Err(ErrorMethodNotAllowed(
            "GraphQL requests can only be sent with GET or POST",
        )),
    }
}
/// Actix GraphQL Handler for GET requests
pub async fn get_graphql_handler<Query, Mutation, Subscription, CtxT, S>(
    schema: &juniper::RootNode<'static, Query, Mutation, Subscription, S>,
    context: &CtxT,
    req: HttpRequest,
) -> Result<HttpResponse, Error>
where
    Query: juniper::GraphQLTypeAsync<S, Context = CtxT>,
    Query::TypeInfo: Sync,
    Mutation: juniper::GraphQLTypeAsync<S, Context = CtxT>,
    Mutation::TypeInfo: Sync,
    Subscription: juniper::GraphQLSubscriptionType<S, Context = CtxT>,
    Subscription::TypeInfo: Sync,
    S: ScalarValue + Send + Sync,
{
    let get_req = web::Query::<GetGraphQLRequest>::from_query(req.query_string())?;
    let req = GraphQLRequest::from(get_req.into_inner());
    let gql_response = req.execute(schema, context).await;
    let body_response = serde_json::to_string(&gql_response)?;
    let mut response = match gql_response.is_ok() {
        true => HttpResponse::Ok(),
        false => HttpResponse::BadRequest(),
    };
    Ok(response
        .content_type("application/json")
        .body(body_response))
}

/// Actix GraphQL Handler for POST requests
pub async fn post_graphql_handler<Query, Mutation, Subscription, CtxT, S>(
    schema: &juniper::RootNode<'static, Query, Mutation, Subscription, S>,
    context: &CtxT,
    req: HttpRequest,
    payload: actix_web::web::Payload,
) -> Result<HttpResponse, Error>
where
    Query: juniper::GraphQLTypeAsync<S, Context = CtxT>,
    Query::TypeInfo: Sync,
    Mutation: juniper::GraphQLTypeAsync<S, Context = CtxT>,
    Mutation::TypeInfo: Sync,
    Subscription: juniper::GraphQLSubscriptionType<S, Context = CtxT>,
    Subscription::TypeInfo: Sync,
    S: ScalarValue + Send + Sync,
{
    let content_type_header = req
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|hv| hv.to_str().ok());
    let req = match content_type_header {
        Some("application/json") => {
            let body = String::from_request(&req, &mut payload.into_inner()).await?;
            serde_json::from_str::<GraphQLBatchRequest<S>>(&body).map_err(ErrorBadRequest)
        }
        Some("application/graphql") => {
            let body = String::from_request(&req, &mut payload.into_inner()).await?;
            Ok(GraphQLBatchRequest::Single(GraphQLRequest::new(
                body, None, None,
            )))
        }
        _ => Err(ErrorUnsupportedMediaType(
            "GraphQL requests should have content type `application/json` or `application/graphql`",
        )),
    }?;
    let gql_batch_response = req.execute(schema, context).await;
    let gql_response = serde_json::to_string(&gql_batch_response)?;
    let mut response = match gql_batch_response.is_ok() {
        true => HttpResponse::Ok(),
        false => HttpResponse::BadRequest(),
    };
    Ok(response.content_type("application/json").body(gql_response))
}

/// Create a handler that replies with an HTML page containing GraphiQL. This does not handle routing, so you can mount it on any endpoint
///
/// For example:
///
/// ```
/// # use juniper_actix::graphiql_handler;
/// # use actix_web::{web, App};
///
/// let app = App::new()
///          .route("/", web::get().to(|| graphiql_handler("/graphql", Some("/graphql/subscriptions"))));
/// ```
#[allow(dead_code)]
pub async fn graphiql_handler(
    graphql_endpoint_url: &str,
    subscriptions_endpoint_url: Option<&'static str>,
) -> Result<HttpResponse, Error> {
    let html = graphiql_source(graphql_endpoint_url, subscriptions_endpoint_url);
    Ok(HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(html))
}

/// Create a handler that replies with an HTML page containing GraphQL Playground. This does not handle routing, so you cant mount it on any endpoint.
pub async fn playground_handler(
    graphql_endpoint_url: &str,
    subscriptions_endpoint_url: Option<&'static str>,
) -> Result<HttpResponse, Error> {
    let html = playground_source(graphql_endpoint_url, subscriptions_endpoint_url);
    Ok(HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(html))
}

/// `juniper_actix` subscriptions handler implementation.
/// Cannot be merged to `juniper_actix` yet as GraphQL over WS[1]
/// is not fully supported in current implementation.
///
/// *Note: this implementation is in an alpha state.*
///
/// [1]: https://github.com/apollographql/subscriptions-transport-ws/blob/master/PROTOCOL.md
#[cfg(feature = "subscriptions")]
pub mod subscriptions {
    use std::{fmt, sync::Arc};

    use actix::{prelude::*, Actor, StreamHandler};
    use actix_web::{
        http::header::{HeaderName, HeaderValue},
        web, HttpRequest, HttpResponse,
    };
    use actix_web_actors::ws;

    use tokio::sync::Mutex;

    use juniper::{
        futures::{
            stream::{SplitSink, SplitStream, StreamExt},
            SinkExt,
        },
        GraphQLSubscriptionType, GraphQLTypeAsync, RootNode, ScalarValue,
    };
    use juniper_graphql_ws::{ArcSchema, ClientMessage, Connection, Init, ServerMessage};

    /// Serves the graphql-ws protocol over a WebSocket connection.
    ///
    /// The `init` argument is used to provide the context and additional configuration for
    /// connections. This can be a `juniper_graphql_ws::ConnectionConfig` if the context and
    /// configuration are already known, or it can be a closure that gets executed asynchronously
    /// when the client sends the ConnectionInit message. Using a closure allows you to perform
    /// authentication based on the parameters provided by the client.
    pub async fn subscriptions_handler<Query, Mutation, Subscription, CtxT, S, I>(
        req: HttpRequest,
        stream: web::Payload,
        root_node: Arc<RootNode<'static, Query, Mutation, Subscription, S>>,
        init: I,
    ) -> Result<HttpResponse, actix_web::Error>
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
        let (s_tx, s_rx) = Connection::new(ArcSchema(root_node), init).split::<Message>();

        let mut resp = ws::start(
            SubscriptionActor {
                graphql_tx: Arc::new(Mutex::new(s_tx)),
                graphql_rx: Arc::new(Mutex::new(s_rx)),
            },
            &req,
            stream,
        )?;

        resp.headers_mut().insert(
            HeaderName::from_static("sec-websocket-protocol"),
            HeaderValue::from_static("graphql-ws"),
        );

        Ok(resp)
    }

    type ConnectionSplitSink<Query, Mutation, Subscription, CtxT, S, I> = Arc<
        Mutex<SplitSink<Connection<ArcSchema<Query, Mutation, Subscription, CtxT, S>, I>, Message>>,
    >;

    type ConnectionSplitStream<Query, Mutation, Subscription, CtxT, S, I> =
        Arc<Mutex<SplitStream<Connection<ArcSchema<Query, Mutation, Subscription, CtxT, S>, I>>>>;

    /// Subscription Actor
    /// coordinates messages between actix_web and juniper_graphql_ws
    /// ws message -> actor -> juniper
    /// juniper -> actor -> ws response
    struct SubscriptionActor<Query, Mutation, Subscription, CtxT, S, I>
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
        graphql_tx: ConnectionSplitSink<Query, Mutation, Subscription, CtxT, S, I>,
        graphql_rx: ConnectionSplitStream<Query, Mutation, Subscription, CtxT, S, I>,
    }

    /// ws message -> actor -> juniper
    impl<Query, Mutation, Subscription, CtxT, S, I>
        StreamHandler<Result<ws::Message, ws::ProtocolError>>
        for SubscriptionActor<Query, Mutation, Subscription, CtxT, S, I>
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
        fn handle(&mut self, msg: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
            let msg = msg.map(|r| Message(r));

            match msg {
                Ok(msg) => {
                    let tx = self.graphql_tx.clone();

                    async move {
                        let mut tx = tx.lock().await;
                        tx.send(msg)
                            .await
                            .expect("Infallible: this should not happen");
                    }
                    .into_actor(self)
                    .wait(ctx);
                }
                Err(_) => {
                    // TODO: trace
                    // ignore the message if there's a transport error
                }
            }
        }
    }

    /// juniper -> actor
    impl<Query, Mutation, Subscription, CtxT, S, I> Actor
        for SubscriptionActor<Query, Mutation, Subscription, CtxT, S, I>
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
        type Context = ws::WebsocketContext<Self>;

        fn started(&mut self, ctx: &mut Self::Context) {
            let stream = self.graphql_rx.clone();
            let addr = ctx.address();

            let fut = async move {
                let mut stream = stream.lock().await;
                while let Some(message) = stream.next().await {
                    // sending the message to self so that it can be forwarded back to the client
                    addr.do_send(ServerMessageWrapper { message });
                }
            }
            .into_actor(self);

            // TODO: trace
            ctx.spawn(fut);
        }

        fn stopped(&mut self, _: &mut Self::Context) {
            // TODO: trace
        }
    }

    /// actor -> websocket response
    impl<Query, Mutation, Subscription, CtxT, S, I> Handler<ServerMessageWrapper<S>>
        for SubscriptionActor<Query, Mutation, Subscription, CtxT, S, I>
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
        type Result = ();

        fn handle(
            &mut self,
            msg: ServerMessageWrapper<S>,
            ctx: &mut ws::WebsocketContext<Self>,
        ) -> Self::Result {
            let msg = serde_json::to_string(&msg.message);

            match msg {
                Ok(msg) => {
                    ctx.text(msg);
                }
                Err(e) => {
                    let reason = ws::CloseReason {
                        code: ws::CloseCode::Error,
                        description: Some(format!("error serializing response: {}", e)),
                    };

                    // TODO: trace
                    ctx.close(Some(reason));
                }
            }
        }
    }

    #[derive(Message)]
    #[rtype(result = "()")]
    struct ServerMessageWrapper<S>
    where
        S: ScalarValue + Send + Sync + 'static,
    {
        message: ServerMessage<S>,
    }

    #[derive(Debug)]
    struct Message(ws::Message);

    impl<S: ScalarValue> std::convert::TryFrom<Message> for ClientMessage<S> {
        type Error = Error;

        fn try_from(msg: Message) -> Result<Self, Self::Error> {
            match msg.0 {
                ws::Message::Text(text) => {
                    serde_json::from_slice(text.as_bytes()).map_err(|e| Error::Serde(e))
                }
                ws::Message::Close(_) => Ok(ClientMessage::ConnectionTerminate),
                _ => Err(Error::UnexpectedClientMessage),
            }
        }
    }

    /// Errors that can happen while handling client messages
    #[derive(Debug)]
    enum Error {
        /// Errors that can happen while deserializing client messages
        Serde(serde_json::Error),

        /// Error for unexpected client messages
        UnexpectedClientMessage,
    }

    impl fmt::Display for Error {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                Self::Serde(e) => write!(f, "serde error: {}", e),
                Self::UnexpectedClientMessage => {
                    write!(f, "unexpected message received from client")
                }
            }
        }
    }

    impl std::error::Error for Error {}
}

#[cfg(test)]
mod tests {
    use actix_web::{dev::ServiceResponse, http, http::header::CONTENT_TYPE, test, App};
    use juniper::{
        futures::stream::StreamExt,
        http::tests::{run_http_test_suite, HttpIntegration, TestResponse},
        tests::fixtures::starwars::schema::{Database, Query},
        EmptyMutation, EmptySubscription, RootNode,
    };

    use super::*;

    type Schema =
        juniper::RootNode<'static, Query, EmptyMutation<Database>, EmptySubscription<Database>>;

    async fn take_response_body_string(resp: &mut ServiceResponse) -> String {
        let (response_body, ..) = resp
            .take_body()
            .map(|body_out| body_out.unwrap().to_vec())
            .into_future()
            .await;
        match response_body {
            Some(response_body) => String::from_utf8(response_body).unwrap(),
            None => String::from(""),
        }
    }

    async fn index(
        req: HttpRequest,
        payload: actix_web::web::Payload,
        schema: web::Data<Schema>,
    ) -> Result<HttpResponse, Error> {
        let context = Database::new();
        graphql_handler(&schema, &context, req, payload).await
    }

    #[actix_web::rt::test]
    async fn graphiql_response_does_not_panic() {
        let result = graphiql_handler("/abcd", None).await;
        assert!(result.is_ok())
    }

    #[actix_web::rt::test]
    async fn graphiql_endpoint_matches() {
        async fn graphql_handler() -> Result<HttpResponse, Error> {
            graphiql_handler("/abcd", None).await
        }
        let mut app =
            test::init_service(App::new().route("/", web::get().to(graphql_handler))).await;
        let req = test::TestRequest::get()
            .uri("/")
            .header("accept", "text/html")
            .to_request();

        let resp = test::call_service(&mut app, req).await;
        assert_eq!(resp.status(), http::StatusCode::OK);
    }

    #[actix_web::rt::test]
    async fn graphiql_endpoint_returns_graphiql_source() {
        async fn graphql_handler() -> Result<HttpResponse, Error> {
            graphiql_handler("/dogs-api/graphql", Some("/dogs-api/subscriptions")).await
        }
        let mut app =
            test::init_service(App::new().route("/", web::get().to(graphql_handler))).await;
        let req = test::TestRequest::get()
            .uri("/")
            .header("accept", "text/html")
            .to_request();

        let mut resp = test::call_service(&mut app, req).await;
        let body = take_response_body_string(&mut resp).await;
        assert_eq!(resp.status(), http::StatusCode::OK);
        assert_eq!(
            resp.headers().get(CONTENT_TYPE).unwrap().to_str().unwrap(),
            "text/html; charset=utf-8"
        );
        assert!(body.contains("<script>var GRAPHQL_URL = '/dogs-api/graphql';</script>"));
        assert!(body.contains(
            "<script>var GRAPHQL_SUBSCRIPTIONS_URL = '/dogs-api/subscriptions';</script>"
        ))
    }

    #[actix_web::rt::test]
    async fn playground_endpoint_matches() {
        async fn graphql_handler() -> Result<HttpResponse, Error> {
            playground_handler("/abcd", None).await
        }
        let mut app =
            test::init_service(App::new().route("/", web::get().to(graphql_handler))).await;
        let req = test::TestRequest::get()
            .uri("/")
            .header("accept", "text/html")
            .to_request();

        let resp = test::call_service(&mut app, req).await;
        assert_eq!(resp.status(), http::StatusCode::OK);
    }

    #[actix_web::rt::test]
    async fn playground_endpoint_returns_playground_source() {
        async fn graphql_handler() -> Result<HttpResponse, Error> {
            playground_handler("/dogs-api/graphql", Some("/dogs-api/subscriptions")).await
        }
        let mut app =
            test::init_service(App::new().route("/", web::get().to(graphql_handler))).await;
        let req = test::TestRequest::get()
            .uri("/")
            .header("accept", "text/html")
            .to_request();

        let mut resp = test::call_service(&mut app, req).await;
        let body = take_response_body_string(&mut resp).await;
        assert_eq!(resp.status(), http::StatusCode::OK);
        assert_eq!(
            resp.headers().get(CONTENT_TYPE).unwrap().to_str().unwrap(),
            "text/html; charset=utf-8"
        );
        assert!(body.contains("GraphQLPlayground.init(root, { endpoint: '/dogs-api/graphql', subscriptionEndpoint: '/dogs-api/subscriptions' })"));
    }

    #[actix_web::rt::test]
    async fn graphql_post_works_json_post() {
        let schema: Schema = RootNode::new(
            Query,
            EmptyMutation::<Database>::new(),
            EmptySubscription::<Database>::new(),
        );

        let req = test::TestRequest::post()
            .header("content-type", "application/json")
            .set_payload(
                r##"{ "variables": null, "query": "{ hero(episode: NEW_HOPE) { name } }" }"##,
            )
            .uri("/")
            .to_request();

        let mut app =
            test::init_service(App::new().data(schema).route("/", web::post().to(index))).await;

        let mut resp = test::call_service(&mut app, req).await;

        assert_eq!(resp.status(), http::StatusCode::OK);
        assert_eq!(
            take_response_body_string(&mut resp).await,
            r#"{"data":{"hero":{"name":"R2-D2"}}}"#
        );
        assert_eq!(
            resp.headers().get("content-type").unwrap(),
            "application/json",
        );
    }

    #[actix_web::rt::test]
    async fn graphql_get_works() {
        let schema: Schema = RootNode::new(
            Query,
            EmptyMutation::<Database>::new(),
            EmptySubscription::<Database>::new(),
        );

        let req = test::TestRequest::get()
            .header("content-type", "application/json")
            .uri("/?query=%7B%20hero%28episode%3A%20NEW_HOPE%29%20%7B%20name%20%7D%20%7D&variables=null")
            .to_request();

        let mut app =
            test::init_service(App::new().data(schema).route("/", web::get().to(index))).await;

        let mut resp = test::call_service(&mut app, req).await;

        assert_eq!(resp.status(), http::StatusCode::OK);
        assert_eq!(
            take_response_body_string(&mut resp).await,
            r#"{"data":{"hero":{"name":"R2-D2"}}}"#
        );
        assert_eq!(
            resp.headers().get("content-type").unwrap(),
            "application/json",
        );
    }

    #[actix_web::rt::test]
    async fn batch_request_works() {
        use juniper::{
            tests::fixtures::starwars::schema::{Database, Query},
            EmptyMutation, EmptySubscription, RootNode,
        };

        let schema: Schema = RootNode::new(
            Query,
            EmptyMutation::<Database>::new(),
            EmptySubscription::<Database>::new(),
        );

        let req = test::TestRequest::post()
            .header("content-type", "application/json")
            .set_payload(
                r##"[
                     { "variables": null, "query": "{ hero(episode: NEW_HOPE) { name } }" },
                     { "variables": null, "query": "{ hero(episode: EMPIRE) { id name } }" }
                 ]"##,
            )
            .uri("/")
            .to_request();

        let mut app =
            test::init_service(App::new().data(schema).route("/", web::post().to(index))).await;

        let mut resp = test::call_service(&mut app, req).await;

        assert_eq!(resp.status(), http::StatusCode::OK);
        assert_eq!(
            take_response_body_string(&mut resp).await,
            r#"[{"data":{"hero":{"name":"R2-D2"}}},{"data":{"hero":{"id":"1000","name":"Luke Skywalker"}}}]"#
        );
        assert_eq!(
            resp.headers().get("content-type").unwrap(),
            "application/json",
        );
    }

    #[test]
    fn batch_request_deserialization_can_fail() {
        let json = r#"blah"#;
        let result: Result<GraphQLBatchRequest, _> = serde_json::from_str(json);

        assert!(result.is_err());
    }

    pub struct TestActixWebIntegration;

    impl TestActixWebIntegration {
        fn make_request(&self, req: test::TestRequest) -> TestResponse {
            actix_web::rt::System::new("request").block_on(async move {
                let schema = Schema::new(
                    Query,
                    EmptyMutation::<Database>::new(),
                    EmptySubscription::<Database>::new(),
                );

                let mut app =
                    test::init_service(App::new().data(schema).route("/", web::to(index))).await;

                let resp = test::call_service(&mut app, req.to_request()).await;
                make_test_response(resp).await
            })
        }
    }

    impl HttpIntegration for TestActixWebIntegration {
        fn get(&self, url: &str) -> TestResponse {
            self.make_request(test::TestRequest::get().uri(url))
        }

        fn post_json(&self, url: &str, body: &str) -> TestResponse {
            self.make_request(
                test::TestRequest::post()
                    .header("content-type", "application/json")
                    .set_payload(body.to_string())
                    .uri(url),
            )
        }

        fn post_graphql(&self, url: &str, body: &str) -> TestResponse {
            self.make_request(
                test::TestRequest::post()
                    .header("content-type", "application/graphql")
                    .set_payload(body.to_string())
                    .uri(url),
            )
        }
    }

    async fn make_test_response(mut resp: ServiceResponse) -> TestResponse {
        let body = take_response_body_string(&mut resp).await;
        let status_code = resp.status().as_u16();
        let content_type = resp.headers().get(CONTENT_TYPE).unwrap();
        TestResponse {
            status_code: status_code as i32,
            body: Some(body),
            content_type: content_type.to_str().unwrap().to_string(),
        }
    }

    #[test]
    fn test_actix_web_integration() {
        run_http_test_suite(&TestActixWebIntegration);
    }
}

#[cfg(feature = "subscriptions")]
#[cfg(test)]
mod subscription_tests {
    use std::time::Duration;

    use actix_web::{test, web, App, Error, HttpRequest, HttpResponse};
    use actix_web_actors::ws;
    use juniper::{
        futures::{SinkExt, StreamExt},
        http::tests::{run_ws_test_suite, WsIntegration, WsIntegrationMessage},
        tests::fixtures::starwars::schema::{Database, Query, Subscription},
        EmptyMutation, LocalBoxFuture,
    };
    use juniper_graphql_ws::ConnectionConfig;
    use tokio::time::timeout;

    use super::subscriptions::subscriptions_handler;

    #[derive(Default)]
    struct TestActixWsIntegration;

    impl TestActixWsIntegration {
        async fn run_async(
            &self,
            messages: Vec<WsIntegrationMessage>,
        ) -> Result<(), anyhow::Error> {
            let mut server = test::start(|| {
                App::new()
                    .data(Schema::new(
                        Query,
                        EmptyMutation::<Database>::new(),
                        Subscription,
                    ))
                    .service(web::resource("/subscriptions").to(subscriptions))
            });
            let mut framed = server.ws_at("/subscriptions").await.unwrap();

            for message in &messages {
                match message {
                    WsIntegrationMessage::Send(body) => {
                        framed
                            .send(ws::Message::Text(body.to_owned()))
                            .await
                            .map_err(|e| anyhow::anyhow!("WS error: {:?}", e))?;
                    }
                    WsIntegrationMessage::Expect(body, message_timeout) => {
                        let frame = timeout(Duration::from_millis(*message_timeout), framed.next())
                            .await
                            .map_err(|_| anyhow::anyhow!("Timed-out waiting for message"))?
                            .ok_or_else(|| anyhow::anyhow!("Empty message received"))?
                            .map_err(|e| anyhow::anyhow!("WS error: {:?}", e))?;

                        match frame {
                            ws::Frame::Text(ref bytes) => {
                                let expected_value =
                                    serde_json::from_str::<serde_json::Value>(body)
                                        .map_err(|e| anyhow::anyhow!("Serde error: {:?}", e))?;

                                let value: serde_json::Value = serde_json::from_slice(bytes)
                                    .map_err(|e| anyhow::anyhow!("Serde error: {:?}", e))?;

                                if value != expected_value {
                                    return Err(anyhow::anyhow!(
                                        "Expected message: {}. Received message: {}",
                                        expected_value,
                                        value,
                                    ));
                                }
                            }
                            _ => return Err(anyhow::anyhow!("Received non-text frame")),
                        }
                    }
                }
            }

            Ok(())
        }
    }

    impl WsIntegration for TestActixWsIntegration {
        fn run(
            &self,
            messages: Vec<WsIntegrationMessage>,
        ) -> LocalBoxFuture<Result<(), anyhow::Error>> {
            Box::pin(self.run_async(messages))
        }
    }

    type Schema = juniper::RootNode<'static, Query, EmptyMutation<Database>, Subscription>;

    async fn subscriptions(
        req: HttpRequest,
        stream: web::Payload,
        schema: web::Data<Schema>,
    ) -> Result<HttpResponse, Error> {
        let context = Database::new();
        let schema = schema.into_inner();
        let config = ConnectionConfig::new(context);

        subscriptions_handler(req, stream, schema, config).await
    }

    #[actix_web::rt::test]
    async fn test_actix_ws_integration() {
        run_ws_test_suite(&mut TestActixWsIntegration::default()).await;
    }
}
