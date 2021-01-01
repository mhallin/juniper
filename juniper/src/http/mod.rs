//! Utilities for building HTTP endpoints in a library-agnostic manner

pub mod graphiql;
pub mod playground;

use serde::{
    de,
    ser::{self, SerializeMap},
    Deserialize, Serialize,
};

use crate::{
    ast::InputValue,
    executor::{ExecutionError, ValuesStream},
    value::{DefaultScalarValue, ScalarValue},
    FieldError, GraphQLError, GraphQLSubscriptionType, GraphQLType, GraphQLTypeAsync, RootNode,
    Value, Variables,
};

/// The expected structure of the decoded JSON document for either POST or GET requests.
///
/// For POST, you can use Serde to deserialize the incoming JSON data directly
/// into this struct - it derives Deserialize for exactly this reason.
///
/// For GET, you will need to parse the query string and extract "query",
/// "operationName", and "variables" manually.
#[derive(Deserialize, Clone, Serialize, PartialEq, Debug)]
pub struct GraphQLRequest<S = DefaultScalarValue>
where
    S: ScalarValue,
{
    query: String,
    #[serde(rename = "operationName")]
    operation_name: Option<String>,
    #[serde(bound(deserialize = "InputValue<S>: Deserialize<'de> + Serialize"))]
    variables: Option<InputValue<S>>,
}

impl<S> GraphQLRequest<S>
where
    S: ScalarValue,
{
    /// Returns the `operation_name` associated with this request.
    pub fn operation_name(&self) -> Option<&str> {
        self.operation_name.as_deref()
    }

    fn variables(&self) -> Variables<S> {
        self.variables
            .as_ref()
            .and_then(|iv| {
                iv.to_object_value().map(|o| {
                    o.into_iter()
                        .map(|(k, v)| (k.to_owned(), v.clone()))
                        .collect()
                })
            })
            .unwrap_or_default()
    }

    /// Construct a new GraphQL request from parts
    pub fn new(
        query: String,
        operation_name: Option<String>,
        variables: Option<InputValue<S>>,
    ) -> Self {
        GraphQLRequest {
            query,
            operation_name,
            variables,
        }
    }

    /// Execute a GraphQL request synchronously using the specified schema and context
    ///
    /// This is a simple wrapper around the `execute_sync` function exposed at the
    /// top level of this crate.
    pub fn execute_sync<'a, QueryT, MutationT, SubscriptionT>(
        &'a self,
        root_node: &'a RootNode<QueryT, MutationT, SubscriptionT, S>,
        context: &QueryT::Context,
    ) -> GraphQLResponse<'a, S>
    where
        S: ScalarValue,
        QueryT: GraphQLType<S>,
        MutationT: GraphQLType<S, Context = QueryT::Context>,
        SubscriptionT: GraphQLType<S, Context = QueryT::Context>,
    {
        GraphQLResponse(crate::execute_sync(
            &self.query,
            self.operation_name(),
            root_node,
            &self.variables(),
            context,
        ))
    }

    /// Execute a GraphQL request using the specified schema and context
    ///
    /// This is a simple wrapper around the `execute` function exposed at the
    /// top level of this crate.
    pub async fn execute<'a, QueryT, MutationT, SubscriptionT>(
        &'a self,
        root_node: &'a RootNode<'a, QueryT, MutationT, SubscriptionT, S>,
        context: &'a QueryT::Context,
    ) -> GraphQLResponse<'a, S>
    where
        QueryT: GraphQLTypeAsync<S>,
        QueryT::TypeInfo: Sync,
        MutationT: GraphQLTypeAsync<S, Context = QueryT::Context>,
        MutationT::TypeInfo: Sync,
        SubscriptionT: GraphQLType<S, Context = QueryT::Context> + Sync,
        SubscriptionT::TypeInfo: Sync,
        S: ScalarValue + Send + Sync,
    {
        let op = self.operation_name();
        let vars = &self.variables();
        let res = crate::execute(&self.query, op, root_node, vars, context).await;
        GraphQLResponse(res)
    }
}

/// Resolve a GraphQL subscription into `Value<ValuesStream<S>` using the
/// specified schema and context.
/// This is a wrapper around the `resolve_into_stream` function exposed at the top
/// level of this crate.
pub async fn resolve_into_stream<'req, 'rn, 'ctx, 'a, QueryT, MutationT, SubscriptionT, S>(
    req: &'req GraphQLRequest<S>,
    root_node: &'rn RootNode<'a, QueryT, MutationT, SubscriptionT, S>,
    context: &'ctx QueryT::Context,
) -> Result<(Value<ValuesStream<'a, S>>, Vec<ExecutionError<S>>), GraphQLError<'a>>
where
    'req: 'a,
    'rn: 'a,
    'ctx: 'a,
    QueryT: GraphQLTypeAsync<S>,
    QueryT::TypeInfo: Sync,
    MutationT: GraphQLTypeAsync<S, Context = QueryT::Context>,
    MutationT::TypeInfo: Sync,
    SubscriptionT: GraphQLSubscriptionType<S, Context = QueryT::Context>,
    SubscriptionT::TypeInfo: Sync,
    S: ScalarValue + Send + Sync,
{
    let op = req.operation_name();
    let vars = req.variables();

    crate::resolve_into_stream(&req.query, op, root_node, &vars, context).await
}

/// Simple wrapper around the result from executing a GraphQL query
///
/// This struct implements Serialize, so you can simply serialize this
/// to JSON and send it over the wire. Use the `is_ok` method to determine
/// whether to send a 200 or 400 HTTP status code.
#[derive(Debug)]
pub struct GraphQLResponse<'a, S = DefaultScalarValue>(
    Result<(Value<S>, Vec<ExecutionError<S>>), GraphQLError<'a>>,
);

impl<'a, S> GraphQLResponse<'a, S>
where
    S: ScalarValue,
{
    /// Constructs new `GraphQLResponse` using the given result
    pub fn from_result(r: Result<(Value<S>, Vec<ExecutionError<S>>), GraphQLError<'a>>) -> Self {
        Self(r)
    }

    /// Constructs an error response outside of the normal execution flow
    pub fn error(error: FieldError<S>) -> Self {
        GraphQLResponse(Ok((Value::null(), vec![ExecutionError::at_origin(error)])))
    }

    /// Was the request successful or not?
    ///
    /// Note that there still might be errors in the response even though it's
    /// considered OK. This is by design in GraphQL.
    pub fn is_ok(&self) -> bool {
        self.0.is_ok()
    }
}

impl<'a, T> Serialize for GraphQLResponse<'a, T>
where
    T: Serialize + ScalarValue,
    Value<T>: Serialize,
    ExecutionError<T>: Serialize,
    GraphQLError<'a>: Serialize,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        match self.0 {
            Ok((ref res, ref err)) => {
                let mut map = serializer.serialize_map(None)?;

                map.serialize_key("data")?;
                map.serialize_value(res)?;

                if !err.is_empty() {
                    map.serialize_key("errors")?;
                    map.serialize_value(err)?;
                }

                map.end()
            }
            Err(ref err) => {
                let mut map = serializer.serialize_map(Some(1))?;
                map.serialize_key("errors")?;
                map.serialize_value(err)?;
                map.end()
            }
        }
    }
}

/// Simple wrapper around GraphQLRequest to allow the handling of Batch requests.
#[derive(Debug, Deserialize, PartialEq)]
#[serde(untagged)]
#[serde(bound = "InputValue<S>: Deserialize<'de>")]
pub enum GraphQLBatchRequest<S = DefaultScalarValue>
where
    S: ScalarValue,
{
    /// A single operation request.
    Single(GraphQLRequest<S>),

    /// A batch operation request.
    ///
    /// Empty batch is considered as invalid value, so cannot be deserialized.
    #[serde(deserialize_with = "deserialize_non_empty_vec")]
    Batch(Vec<GraphQLRequest<S>>),
}

fn deserialize_non_empty_vec<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: de::Deserializer<'de>,
    T: Deserialize<'de>,
{
    use de::Error as _;

    let v = Vec::<T>::deserialize(deserializer)?;
    if v.is_empty() {
        Err(D::Error::invalid_length(0, &"a positive integer"))
    } else {
        Ok(v)
    }
}

impl<S> GraphQLBatchRequest<S>
where
    S: ScalarValue,
{
    /// Execute a GraphQL batch request synchronously using the specified schema and context
    ///
    /// This is a simple wrapper around the `execute_sync` function exposed in GraphQLRequest.
    pub fn execute_sync<'a, QueryT, MutationT, SubscriptionT>(
        &'a self,
        root_node: &'a RootNode<QueryT, MutationT, SubscriptionT, S>,
        context: &QueryT::Context,
    ) -> GraphQLBatchResponse<'a, S>
    where
        QueryT: GraphQLType<S>,
        MutationT: GraphQLType<S, Context = QueryT::Context>,
        SubscriptionT: GraphQLType<S, Context = QueryT::Context>,
    {
        match *self {
            Self::Single(ref req) => {
                GraphQLBatchResponse::Single(req.execute_sync(root_node, context))
            }
            Self::Batch(ref reqs) => GraphQLBatchResponse::Batch(
                reqs.iter()
                    .map(|req| req.execute_sync(root_node, context))
                    .collect(),
            ),
        }
    }

    /// Executes a GraphQL request using the specified schema and context
    ///
    /// This is a simple wrapper around the `execute` function exposed in
    /// GraphQLRequest
    pub async fn execute<'a, QueryT, MutationT, SubscriptionT>(
        &'a self,
        root_node: &'a RootNode<'a, QueryT, MutationT, SubscriptionT, S>,
        context: &'a QueryT::Context,
    ) -> GraphQLBatchResponse<'a, S>
    where
        QueryT: GraphQLTypeAsync<S>,
        QueryT::TypeInfo: Sync,
        MutationT: GraphQLTypeAsync<S, Context = QueryT::Context>,
        MutationT::TypeInfo: Sync,
        SubscriptionT: GraphQLSubscriptionType<S, Context = QueryT::Context>,
        SubscriptionT::TypeInfo: Sync,
        S: Send + Sync,
    {
        match self {
            Self::Single(req) => {
                let resp = req.execute(root_node, context).await;
                GraphQLBatchResponse::Single(resp)
            }
            Self::Batch(reqs) => {
                let resps = futures::future::join_all(
                    reqs.iter().map(|req| req.execute(root_node, context)),
                )
                .await;
                GraphQLBatchResponse::Batch(resps)
            }
        }
    }

    /// The operation names of the request.
    pub fn operation_names(&self) -> Vec<Option<&str>> {
        match self {
            Self::Single(req) => vec![req.operation_name()],
            Self::Batch(reqs) => reqs.iter().map(|req| req.operation_name()).collect(),
        }
    }
}

/// Simple wrapper around the result (GraphQLResponse) from executing a GraphQLBatchRequest
///
/// This struct implements Serialize, so you can simply serialize this
/// to JSON and send it over the wire. use the `is_ok` to determine
/// wheter to send a 200 or 400 HTTP status code.
#[derive(Serialize)]
#[serde(untagged)]
pub enum GraphQLBatchResponse<'a, S = DefaultScalarValue>
where
    S: ScalarValue,
{
    /// Result of a single operation in a GraphQL request.
    Single(GraphQLResponse<'a, S>),
    /// Result of a batch operation in a GraphQL request.
    Batch(Vec<GraphQLResponse<'a, S>>),
}

impl<'a, S> GraphQLBatchResponse<'a, S>
where
    S: ScalarValue,
{
    /// Returns if all the GraphQLResponse in this operation are ok,
    /// you can use it to determine wheter to send a 200 or 400 HTTP status code.
    pub fn is_ok(&self) -> bool {
        match self {
            Self::Single(resp) => resp.is_ok(),
            Self::Batch(resps) => resps.iter().all(GraphQLResponse::is_ok),
        }
    }
}

#[cfg(feature = "expose-test-schema")]
#[allow(missing_docs)]
pub mod tests {
    use crate::LocalBoxFuture;
    use serde_json::{self, Value as Json};

    /// Normalized response content we expect to get back from
    /// the http framework integration we are testing.
    #[derive(Debug)]
    pub struct TestResponse {
        pub status_code: i32,
        pub body: Option<String>,
        pub content_type: String,
    }

    /// Normalized way to make requests to the HTTP framework integration we are testing.
    pub trait HttpIntegration {
        /// Sends GET HTTP request to this integration with the provided `url` parameters string,
        /// and returns response returned by this integration.
        fn get(&self, url: &str) -> TestResponse;

        /// Sends POST HTTP request to this integration with the provided JSON-encoded `body`, and
        /// returns response returned by this integration.
        fn post_json(&self, url: &str, body: &str) -> TestResponse;

        /// Sends POST HTTP request to this integration with the provided raw GraphQL query as
        /// `body`, and returns response returned by this integration.
        fn post_graphql(&self, url: &str, body: &str) -> TestResponse;
    }

    #[allow(missing_docs)]
    pub fn run_http_test_suite<T: HttpIntegration>(integration: &T) {
        println!("Running HTTP Test suite for integration");

        println!("  - test_simple_get");
        test_simple_get(integration);

        println!("  - test_encoded_get");
        test_encoded_get(integration);

        println!("  - test_get_with_variables");
        test_get_with_variables(integration);

        println!("  - test_simple_post");
        test_simple_post(integration);

        println!("  - test_batched_post");
        test_batched_post(integration);

        println!("  - test_empty_batched_post");
        test_empty_batched_post(integration);

        println!("  - test_invalid_json");
        test_invalid_json(integration);

        println!("  - test_invalid_field");
        test_invalid_field(integration);

        println!("  - test_duplicate_keys");
        test_duplicate_keys(integration);

        println!("  - test_graphql_post");
        test_graphql_post(integration);

        println!("  - test_invalid_graphql_post");
        test_invalid_graphql_post(integration);
    }

    fn unwrap_json_response(response: &TestResponse) -> Json {
        serde_json::from_str::<Json>(
            response
                .body
                .as_ref()
                .expect("No data returned from request"),
        )
        .expect("Could not parse JSON object")
    }

    fn test_simple_get<T: HttpIntegration>(integration: &T) {
        // {hero{name}}
        let response = integration.get("/?query=%7Bhero%7Bname%7D%7D");

        assert_eq!(response.status_code, 200);
        assert_eq!(response.content_type.as_str(), "application/json");

        assert_eq!(
            unwrap_json_response(&response),
            serde_json::from_str::<Json>(r#"{"data": {"hero": {"name": "R2-D2"}}}"#)
                .expect("Invalid JSON constant in test")
        );
    }

    fn test_encoded_get<T: HttpIntegration>(integration: &T) {
        // query { human(id: "1000") { id, name, appearsIn, homePlanet } }
        let response = integration.get(
            "/?query=query%20%7B%20human(id%3A%20%221000%22)%20%7B%20id%2C%20name%2C%20appearsIn%2C%20homePlanet%20%7D%20%7D");

        assert_eq!(response.status_code, 200);
        assert_eq!(response.content_type.as_str(), "application/json");

        assert_eq!(
            unwrap_json_response(&response),
            serde_json::from_str::<Json>(
                r#"{
                    "data": {
                        "human": {
                            "appearsIn": [
                                "NEW_HOPE",
                                "EMPIRE",
                                "JEDI"
                                ],
                                "homePlanet": "Tatooine",
                                "name": "Luke Skywalker",
                                "id": "1000"
                            }
                        }
                    }"#
            )
            .expect("Invalid JSON constant in test")
        );
    }

    fn test_get_with_variables<T: HttpIntegration>(integration: &T) {
        // query($id: String!) { human(id: $id) { id, name, appearsIn, homePlanet } }
        // with variables = { "id": "1000" }
        let response = integration.get(
            "/?query=query(%24id%3A%20String!)%20%7B%20human(id%3A%20%24id)%20%7B%20id%2C%20name%2C%20appearsIn%2C%20homePlanet%20%7D%20%7D&variables=%7B%20%22id%22%3A%20%221000%22%20%7D");

        assert_eq!(response.status_code, 200);
        assert_eq!(response.content_type, "application/json");

        assert_eq!(
            unwrap_json_response(&response),
            serde_json::from_str::<Json>(
                r#"{
                    "data": {
                        "human": {
                            "appearsIn": [
                                "NEW_HOPE",
                                "EMPIRE",
                                "JEDI"
                                ],
                                "homePlanet": "Tatooine",
                                "name": "Luke Skywalker",
                                "id": "1000"
                            }
                        }
                    }"#
            )
            .expect("Invalid JSON constant in test")
        );
    }

    fn test_simple_post<T: HttpIntegration>(integration: &T) {
        let response = integration.post_json("/", r#"{"query": "{hero{name}}"}"#);

        assert_eq!(response.status_code, 200);
        assert_eq!(response.content_type, "application/json");

        assert_eq!(
            unwrap_json_response(&response),
            serde_json::from_str::<Json>(r#"{"data": {"hero": {"name": "R2-D2"}}}"#)
                .expect("Invalid JSON constant in test"),
        );
    }

    fn test_batched_post<T: HttpIntegration>(integration: &T) {
        let response = integration.post_json(
            "/",
            r#"[{"query": "{hero{name}}"}, {"query": "{hero{name}}"}]"#,
        );

        assert_eq!(response.status_code, 200);
        assert_eq!(response.content_type, "application/json");

        assert_eq!(
            unwrap_json_response(&response),
            serde_json::from_str::<Json>(
                r#"[{"data": {"hero": {"name": "R2-D2"}}}, {"data": {"hero": {"name": "R2-D2"}}}]"#,
            )
            .expect("Invalid JSON constant in test"),
        );
    }

    fn test_empty_batched_post<T: HttpIntegration>(integration: &T) {
        let response = integration.post_json("/", "[]");
        assert_eq!(response.status_code, 400);
    }

    fn test_invalid_json<T: HttpIntegration>(integration: &T) {
        let response = integration.get("/?query=blah");
        assert_eq!(response.status_code, 400);
        let response = integration.post_json("/", r#"blah"#);
        assert_eq!(response.status_code, 400);
    }

    fn test_invalid_field<T: HttpIntegration>(integration: &T) {
        // {hero{blah}}
        let response = integration.get("/?query=%7Bhero%7Bblah%7D%7D");
        assert_eq!(response.status_code, 400);
        let response = integration.post_json("/", r#"{"query": "{hero{blah}}"}"#);
        assert_eq!(response.status_code, 400);
    }

    fn test_duplicate_keys<T: HttpIntegration>(integration: &T) {
        // {hero{name}}
        let response = integration.get("/?query=%7B%22query%22%3A%20%22%7Bhero%7Bname%7D%7D%22%2C%20%22query%22%3A%20%22%7Bhero%7Bname%7D%7D%22%7D");
        assert_eq!(response.status_code, 400);
        let response =
            integration.post_json("/", r#"{"query": "{hero{name}}", "query": "{hero{name}}"}"#);
        assert_eq!(response.status_code, 400);
    }

    fn test_graphql_post<T: HttpIntegration>(integration: &T) {
        let resp = integration.post_graphql("/", r#"{hero{name}}"#);

        assert_eq!(resp.status_code, 200);
        assert_eq!(resp.content_type, "application/json");

        assert_eq!(
            unwrap_json_response(&resp),
            serde_json::from_str::<Json>(r#"{"data": {"hero": {"name": "R2-D2"}}}"#)
                .expect("Invalid JSON constant in test"),
        );
    }

    fn test_invalid_graphql_post<T: HttpIntegration>(integration: &T) {
        let resp = integration.post_graphql("/", r#"{hero{name}"#);

        assert_eq!(resp.status_code, 400);
    }

    /// Normalized way to make requests to the WebSocket framework integration we are testing.
    pub trait WsIntegration {
        /// Runs a test with the given messages
        fn run(
            &self,
            messages: Vec<WsIntegrationMessage>,
        ) -> LocalBoxFuture<Result<(), anyhow::Error>>;
    }

    /// WebSocket framework integration message
    pub enum WsIntegrationMessage {
        /// Send message through the WebSocket
        /// Takes a message as a String
        Send(String),
        /// Expect message to come through the WebSocket
        /// Takes expected message as a String and a timeout in milliseconds
        Expect(String, u64),
    }

    /// Default value in milliseconds for how long to wait for an incoming message
    pub const WS_INTEGRATION_EXPECT_DEFAULT_TIMEOUT: u64 = 100;

    #[allow(missing_docs)]
    pub async fn run_ws_test_suite<T: WsIntegration>(integration: &T) {
        println!("Running WebSocket Test suite for integration");

        println!("  - test_ws_simple_subscription");
        test_ws_simple_subscription(integration).await;

        println!("  - test_ws_invalid_json");
        test_ws_invalid_json(integration).await;

        println!("  - test_ws_invalid_query");
        test_ws_invalid_query(integration).await;
    }

    async fn test_ws_simple_subscription<T: WsIntegration>(integration: &T) {
        let messages = vec![
            WsIntegrationMessage::Send(
                r#"{
                    "type":"connection_init",
                    "payload":{}
                }"#
                .to_owned(),
            ),
            WsIntegrationMessage::Expect(
                r#"{
                    "type":"connection_ack"
                }"#
                .to_owned(),
                WS_INTEGRATION_EXPECT_DEFAULT_TIMEOUT,
            ),
            WsIntegrationMessage::Expect(
                r#"{
                    "type":"ka"
                }"#
                .to_owned(),
                WS_INTEGRATION_EXPECT_DEFAULT_TIMEOUT,
            ),
            WsIntegrationMessage::Send(
                r#"{
                    "id":"1",
                    "type":"start",
                    "payload":{
                        "variables":{},
                        "extensions":{},
                        "operationName":null,
                        "query":"subscription { asyncHuman { id, name, homePlanet } }"
                    }
                }"#
                .to_owned(),
            ),
            WsIntegrationMessage::Expect(
                r#"{
                    "type":"data",
                    "id":"1",
                    "payload":{
                        "data":{
                            "asyncHuman":{
                                "id":"1000",
                                "name":"Luke Skywalker",
                                "homePlanet":"Tatooine"
                            }
                        }
                    }
                }"#
                .to_owned(),
                WS_INTEGRATION_EXPECT_DEFAULT_TIMEOUT,
            ),
        ];

        integration.run(messages).await.unwrap();
    }

    async fn test_ws_invalid_json<T: WsIntegration>(integration: &T) {
        let messages = vec![
            WsIntegrationMessage::Send("invalid json".to_owned()),
            WsIntegrationMessage::Expect(
                r#"{
                    "type":"connection_error",
                    "payload":{
                        "message":"serde error: expected value at line 1 column 1"
                    }
                }"#
                .to_owned(),
                WS_INTEGRATION_EXPECT_DEFAULT_TIMEOUT,
            ),
        ];

        integration.run(messages).await.unwrap();
    }

    async fn test_ws_invalid_query<T: WsIntegration>(integration: &T) {
        let messages = vec![
            WsIntegrationMessage::Send(
                r#"{
                    "type":"connection_init",
                    "payload":{}
                }"#
                .to_owned(),
            ),
            WsIntegrationMessage::Expect(
                r#"{
                    "type":"connection_ack"
                }"#
                .to_owned(),
                WS_INTEGRATION_EXPECT_DEFAULT_TIMEOUT
            ),
            WsIntegrationMessage::Expect(
                r#"{
                    "type":"ka"
                }"#
                .to_owned(),
                WS_INTEGRATION_EXPECT_DEFAULT_TIMEOUT
            ),
            WsIntegrationMessage::Send(
                r#"{
                    "id":"1",
                    "type":"start",
                    "payload":{
                        "variables":{},
                        "extensions":{},
                        "operationName":null,
                        "query":"subscription { asyncHuman }"
                    }
                }"#
                .to_owned(),
            ),
            WsIntegrationMessage::Expect(
                r#"{
                    "type":"error",
                    "id":"1",
                    "payload":[{
                        "message":"Field \"asyncHuman\" of type \"Human!\" must have a selection of subfields. Did you mean \"asyncHuman { ... }\"?",
                        "locations":[{
                            "line":1,
                            "column":16
                        }]
                    }]
                }"#
                .to_owned(),
                WS_INTEGRATION_EXPECT_DEFAULT_TIMEOUT
            )
        ];

        integration.run(messages).await.unwrap();
    }
}
