/*!

# juniper_graphql_ws

This crate contains an implementation of the [graphql-ws protocol](https://github.com/apollographql/subscriptions-transport-ws/blob/263844b5c1a850c1e29814564eb62cb587e5eaaf/PROTOCOL.md), as used by Apollo.

*/

#![deny(missing_docs)]
#![deny(warnings)]

mod client_message;
pub use client_message::*;

mod server_message;
pub use server_message::*;

mod schema;
pub use schema::*;

mod utils;

use std::{
    collections::HashMap,
    convert::{Infallible, TryInto},
    error::Error,
    marker::PhantomPinned,
    pin::Pin,
    sync::Arc,
    time::Duration,
};

use juniper::{
    futures::{
        channel::oneshot,
        future::{self, Either, Future, FutureExt, LocalBoxFuture, TryFutureExt},
        stream::{self, LocalBoxStream, SelectAll, StreamExt},
        task::{Context, Poll, Waker},
        Sink, Stream,
    },
    GraphQLError, RuleError, ScalarValue, Variables,
};

struct ExecutionParams<S: Schema> {
    start_payload: StartPayload<S::ScalarValue>,
    schema: S,
}

/// ConnectionConfig is used to configure the connection once the client sends the ConnectionInit
/// message.
pub struct ConnectionConfig<CtxT> {
    context: CtxT,
    max_in_flight_operations: usize,
    keep_alive_interval: Duration,
}

impl<CtxT> ConnectionConfig<CtxT> {
    /// Constructs the configuration required for a connection to be accepted.
    pub fn new(context: CtxT) -> Self {
        Self {
            context,
            max_in_flight_operations: 0,
            keep_alive_interval: Duration::from_secs(15),
        }
    }

    /// Specifies the maximum number of in-flight operations that a connection can have. If this
    /// number is exceeded, attempting to start more will result in an error. By default, there is
    /// no limit to in-flight operations.
    pub fn with_max_in_flight_operations(mut self, max: usize) -> Self {
        self.max_in_flight_operations = max;
        self
    }

    /// Specifies the interval at which to send keep-alives. Specifying a zero duration will
    /// disable keep-alives. By default, keep-alives are sent every 15 seconds.
    pub fn with_keep_alive_interval(mut self, interval: Duration) -> Self {
        self.keep_alive_interval = interval;
        self
    }
}

impl<S: ScalarValue, CtxT: Unpin + Send + 'static> Init<S, CtxT> for ConnectionConfig<CtxT> {
    type Error = Infallible;
    type Future = future::Ready<Result<Self, Self::Error>>;

    fn init(self, _params: Variables<S>) -> Self::Future {
        future::ready(Ok(self))
    }
}

enum Reaction<S: Schema> {
    ServerMessage(ServerMessage<S::ScalarValue>),
    EndStream,
}

impl<S: Schema> Reaction<S> {
    /// Converts the reaction into a one-item stream.
    fn into_stream(self) -> LocalBoxStream<'static, Self> {
        stream::once(future::ready(self)).boxed_local()
    }
}

/// Init defines the requirements for types that can provide connection configurations when
/// ConnectionInit messages are received. Implementations are provided for `ConnectionConfig` and
/// closures that meet the requirements.
pub trait Init<S: ScalarValue, CtxT>: Unpin + 'static {
    /// The error that is returned on failure. The formatted error will be used as the contents of
    /// the "message" field sent back to the client.
    type Error: Error;

    /// The future configuration type.
    type Future: Future<Output = Result<ConnectionConfig<CtxT>, Self::Error>> + Send + 'static;

    /// Returns a future for the configuration to use.
    fn init(self, params: Variables<S>) -> Self::Future;
}

impl<F, S, CtxT, Fut, E> Init<S, CtxT> for F
where
    S: ScalarValue,
    F: FnOnce(Variables<S>) -> Fut + Unpin + 'static,
    Fut: Future<Output = Result<ConnectionConfig<CtxT>, E>> + Send + 'static,
    E: Error,
{
    type Error = E;
    type Future = Fut;

    fn init(self, params: Variables<S>) -> Fut {
        self(params)
    }
}

enum ConnectionState<S: Schema, I: Init<S::ScalarValue, S::Context>> {
    /// PreInit is the state before a ConnectionInit message has been accepted.
    PreInit { init: I, schema: S },
    /// Active is the state after a ConnectionInit message has been accepted.
    Active {
        config: Arc<ConnectionConfig<S::Context>>,
        stoppers: HashMap<String, oneshot::Sender<()>>,
        schema: S,
    },
    /// Terminated is the state after a ConnectionInit message has been rejected.
    Terminated,
}

impl<S: Schema, I: Init<S::ScalarValue, S::Context>> ConnectionState<S, I> {
    // Each message we receive results in a stream of zero or more reactions. For example, a
    // ConnectionTerminate message results in a one-item stream with the EndStream reaction.
    async fn handle_message(
        self,
        msg: ClientMessage<S::ScalarValue>,
    ) -> (Self, LocalBoxStream<'static, Reaction<S>>) {
        if let ClientMessage::ConnectionTerminate = msg {
            return (self, Reaction::EndStream.into_stream());
        }

        match self {
            Self::PreInit { init, schema } => match msg {
                ClientMessage::ConnectionInit { payload } => match init.init(payload).await {
                    Ok(config) => {
                        let keep_alive_interval = config.keep_alive_interval;

                        let mut s = stream::iter(vec![Reaction::ServerMessage(
                            ServerMessage::ConnectionAck,
                        )])
                        .boxed_local();

                        if keep_alive_interval > Duration::from_secs(0) {
                            s = s
                                .chain(
                                    Reaction::ServerMessage(ServerMessage::ConnectionKeepAlive)
                                        .into_stream(),
                                )
                                .boxed_local();
                            s = s
                                .chain(stream::unfold((), move |_| async move {
                                    tokio::time::delay_for(keep_alive_interval).await;
                                    Some((
                                        Reaction::ServerMessage(ServerMessage::ConnectionKeepAlive),
                                        (),
                                    ))
                                }))
                                .boxed_local();
                        }

                        (
                            Self::Active {
                                config: Arc::new(config),
                                stoppers: HashMap::new(),
                                schema,
                            },
                            s,
                        )
                    }
                    Err(e) => (
                        Self::Terminated,
                        stream::iter(vec![
                            Reaction::ServerMessage(ServerMessage::ConnectionError {
                                payload: ConnectionErrorPayload {
                                    message: e.to_string(),
                                },
                            }),
                            Reaction::EndStream,
                        ])
                        .boxed_local(),
                    ),
                },
                _ => (
                    Self::PreInit { init, schema },
                    stream::empty().boxed_local(),
                ),
            },
            Self::Active {
                config,
                mut stoppers,
                schema,
            } => {
                let reactions = match msg {
                    ClientMessage::Start { id, payload } => {
                        if stoppers.contains_key(&id) {
                            // We already have an operation with this id, so we can't start a new
                            // one.
                            stream::empty().boxed_local()
                        } else {
                            // Go ahead and prune canceled stoppers before adding a new one.
                            stoppers.retain(|_, tx| !tx.is_canceled());

                            if config.max_in_flight_operations > 0
                                && stoppers.len() >= config.max_in_flight_operations
                            {
                                // Too many in-flight operations. Just send back a validation error.
                                stream::iter(vec![
                                    Reaction::ServerMessage(ServerMessage::Error {
                                        id: id.clone(),
                                        payload: GraphQLError::ValidationError(vec![
                                            RuleError::new("Too many in-flight operations.", &[]),
                                        ])
                                        .into(),
                                    }),
                                    Reaction::ServerMessage(ServerMessage::Complete { id }),
                                ])
                                .boxed_local()
                            } else {
                                // Create a channel that we can use to cancel the operation.
                                let (tx, rx) = oneshot::channel::<()>();
                                stoppers.insert(id.clone(), tx);

                                // Create the operation stream. This stream will emit Data and Error
                                // messages, but will not emit Complete – that part is up to us.
                                let s = Self::start(
                                    id.clone(),
                                    ExecutionParams {
                                        start_payload: payload,
                                        schema: schema.clone(),
                                    },
                                    config.clone(),
                                )
                                .into_stream()
                                .flatten();

                                // Combine this with our oneshot channel so that the stream ends if the
                                // oneshot is ever fired.
                                let s = stream::unfold(
                                    (rx, s.boxed_local()),
                                    |(rx, mut s)| async move {
                                        let next = match future::select(rx, s.next()).await {
                                            Either::Left(_) => None,
                                            Either::Right((r, rx)) => r.map(|r| (r, rx)),
                                        };
                                        next.map(|(r, rx)| (r, (rx, s)))
                                    },
                                );

                                // Once the stream ends, send the Complete message.
                                let s = s.chain(
                                    Reaction::ServerMessage(ServerMessage::Complete { id })
                                        .into_stream(),
                                );

                                s.boxed_local()
                            }
                        }
                    }
                    ClientMessage::Stop { id } => {
                        stoppers.remove(&id);
                        stream::empty().boxed_local()
                    }
                    _ => stream::empty().boxed_local(),
                };
                (
                    Self::Active {
                        config,
                        stoppers,
                        schema,
                    },
                    reactions,
                )
            }
            Self::Terminated => (self, stream::empty().boxed_local()),
        }
    }

    async fn start(
        id: String,
        params: ExecutionParams<S>,
        config: Arc<ConnectionConfig<S::Context>>,
    ) -> LocalBoxStream<'static, Reaction<S>> {
        // TODO: This could be made more efficient if juniper exposed functionality to allow us to
        // parse and validate the query, determine whether it's a subscription, and then execute
        // it. For now, the query gets parsed and validated twice.

        let params = Arc::new(params);

        // Try to execute this as a query or mutation.
        match juniper::execute(
            &params.start_payload.query,
            params.start_payload.operation_name.as_deref(),
            params.schema.root_node(),
            &params.start_payload.variables,
            &config.context,
        )
        .await
        {
            Ok((data, errors)) => {
                return Reaction::ServerMessage(ServerMessage::Data {
                    id: id.clone(),
                    payload: DataPayload { data, errors },
                })
                .into_stream();
            }
            Err(GraphQLError::IsSubscription) => {}
            Err(e) => {
                return Reaction::ServerMessage(ServerMessage::Error {
                    id: id.clone(),
                    // e only references data owned by params. The new ErrorPayload will continue to keep that data alive.
                    payload: unsafe { ErrorPayload::new_unchecked(Box::new(params.clone()), e) },
                })
                .into_stream();
            }
        }

        // Try to execute as a subscription.
        SubscriptionStart::new(id, params, config).boxed_local()
    }
}

struct InterruptableStream<S> {
    stream: S,
    rx: oneshot::Receiver<()>,
}

impl<S: Stream + Unpin> Stream for InterruptableStream<S> {
    type Item = S::Item;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        match Pin::new(&mut self.rx).poll(cx) {
            Poll::Ready(_) => return Poll::Ready(None),
            Poll::Pending => {}
        }
        Pin::new(&mut self.stream).poll_next(cx)
    }
}

/// SubscriptionStartState is the state for a subscription operation.
enum SubscriptionStartState<S: Schema> {
    /// Init is the start before being polled for the first time.
    Init { id: String },
    /// ResolvingIntoStream is the state after being polled for the first time. In this state,
    /// we're parsing, validating, and getting the actual event stream.
    ResolvingIntoStream {
        id: String,
        future: LocalBoxFuture<
            'static,
            Result<
                juniper_subscriptions::Connection<'static, S::ScalarValue>,
                GraphQLError<'static>,
            >,
        >,
    },
    /// Streaming is the state after we've successfully obtained the event stream for the
    /// subscription. In this state, we're just forwarding events back to the client.
    Streaming {
        id: String,
        stream: juniper_subscriptions::Connection<'static, S::ScalarValue>,
    },
    /// Terminated is the state once we're all done.
    Terminated,
}

/// SubscriptionStart is the stream for a subscription operation.
struct SubscriptionStart<S: Schema> {
    params: Arc<ExecutionParams<S>>,
    config: Arc<ConnectionConfig<S::Context>>,
    state: SubscriptionStartState<S>,
    _marker: PhantomPinned,
}

impl<S: Schema> SubscriptionStart<S> {
    fn new(
        id: String,
        params: Arc<ExecutionParams<S>>,
        config: Arc<ConnectionConfig<S::Context>>,
    ) -> Pin<Box<Self>> {
        Box::pin(Self {
            params,
            config,
            state: SubscriptionStartState::Init { id },
            _marker: PhantomPinned,
        })
    }
}

impl<S: Schema> Stream for SubscriptionStart<S> {
    type Item = Reaction<S>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        let (params, config, state) = unsafe {
            // XXX: The execution parameters are referenced by state and must not be modified.
            // Modifying state is fine though.
            let inner = self.get_unchecked_mut();
            (&inner.params, &inner.config, &mut inner.state)
        };

        loop {
            match state {
                SubscriptionStartState::Init { id } => {
                    // XXX: resolve_into_stream returns a Future that references the execution
                    // parameters, and the returned stream also references them. We can guarantee
                    // that everything has the same lifetime in this self-referential struct.
                    let params = Arc::as_ptr(params);
                    let config = Arc::as_ptr(config);
                    *state = SubscriptionStartState::ResolvingIntoStream {
                        id: id.clone(),
                        future: unsafe {
                            juniper::resolve_into_stream(
                                &(*params).start_payload.query,
                                (*params).start_payload.operation_name.as_deref(),
                                (*params).schema.root_node(),
                                &(*params).start_payload.variables,
                                &(*config).context,
                            )
                        }
                        .map_ok(|(stream, errors)| {
                            juniper_subscriptions::Connection::from_stream(stream, errors)
                        })
                        .boxed_local(),
                    };
                }
                SubscriptionStartState::ResolvingIntoStream {
                    ref id,
                    ref mut future,
                } => match future.as_mut().poll(cx) {
                    Poll::Ready(r) => match r {
                        Ok(stream) => {
                            *state = SubscriptionStartState::Streaming {
                                id: id.clone(),
                                stream,
                            }
                        }
                        Err(e) => {
                            return Poll::Ready(Some(Reaction::ServerMessage(
                                ServerMessage::Error {
                                    id: id.clone(),
                                    // e only references data owned by params. The new ErrorPayload will continue to keep that data alive.
                                    payload: unsafe {
                                        ErrorPayload::new_unchecked(Box::new(params.clone()), e)
                                    },
                                },
                            )));
                        }
                    },
                    Poll::Pending => return Poll::Pending,
                },
                SubscriptionStartState::Streaming {
                    ref id,
                    ref mut stream,
                } => match Pin::new(stream).poll_next(cx) {
                    Poll::Ready(Some(output)) => {
                        return Poll::Ready(Some(Reaction::ServerMessage(ServerMessage::Data {
                            id: id.clone(),
                            payload: DataPayload {
                                data: output.data,
                                errors: output.errors,
                            },
                        })));
                    }
                    Poll::Ready(None) => {
                        *state = SubscriptionStartState::Terminated;
                        return Poll::Ready(None);
                    }
                    Poll::Pending => return Poll::Pending,
                },
                SubscriptionStartState::Terminated => return Poll::Ready(None),
            }
        }
    }
}

enum ConnectionSinkState<S: Schema, I: Init<S::ScalarValue, S::Context>> {
    Ready {
        state: ConnectionState<S, I>,
    },
    HandlingMessage {
        result:
            LocalBoxFuture<'static, (ConnectionState<S, I>, LocalBoxStream<'static, Reaction<S>>)>,
    },
    Closed,
}

/// Implements the graphql-ws protocol. This is a sink for `TryInto<ClientMessage>` and a stream of
/// `ServerMessage`.
pub struct Connection<S: Schema, I: Init<S::ScalarValue, S::Context>> {
    reactions: SelectAll<LocalBoxStream<'static, Reaction<S>>>,
    stream_waker: Option<Waker>,
    sink_state: ConnectionSinkState<S, I>,
}

impl<S, I> Connection<S, I>
where
    S: Schema,
    I: Init<S::ScalarValue, S::Context>,
{
    /// Creates a new connection, which is a sink for `TryInto<ClientMessage>` and a stream of `ServerMessage`.
    ///
    /// The `schema` argument should typically be an `Arc<RootNode<...>>`.
    ///
    /// The `init` argument is used to provide the context and additional configuration for
    /// connections. This can be a `ConnectionConfig` if the context and configuration are already
    /// known, or it can be a closure that gets executed asynchronously when the client sends the
    /// ConnectionInit message. Using a closure allows you to perform authentication based on the
    /// parameters provided by the client.
    pub fn new(schema: S, init: I) -> Self {
        Self {
            reactions: SelectAll::new(),
            stream_waker: None,
            sink_state: ConnectionSinkState::Ready {
                state: ConnectionState::PreInit { init, schema },
            },
        }
    }
}

impl<S, I, T> Sink<T> for Connection<S, I>
where
    T: TryInto<ClientMessage<S::ScalarValue>>,
    T::Error: Error,
    S: Schema,
    I: Init<S::ScalarValue, S::Context> + Send,
{
    type Error = Infallible;

    fn poll_ready(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        match &mut self.sink_state {
            ConnectionSinkState::Ready { .. } => Poll::Ready(Ok(())),
            ConnectionSinkState::HandlingMessage { ref mut result } => {
                match Pin::new(result).poll(cx) {
                    Poll::Ready((state, reactions)) => {
                        self.reactions.push(reactions);
                        self.sink_state = ConnectionSinkState::Ready { state };
                        Poll::Ready(Ok(()))
                    }
                    Poll::Pending => Poll::Pending,
                }
            }
            ConnectionSinkState::Closed => panic!("poll_ready called after close"),
        }
    }

    fn start_send(self: Pin<&mut Self>, item: T) -> Result<(), Self::Error> {
        let s = self.get_mut();
        let state = &mut s.sink_state;
        *state = match std::mem::replace(state, ConnectionSinkState::Closed) {
            ConnectionSinkState::Ready { state } => {
                match item.try_into() {
                    Ok(msg) => ConnectionSinkState::HandlingMessage {
                        result: state.handle_message(msg).boxed_local(),
                    },
                    Err(e) => {
                        // If we weren't able to parse the message, send back an error.
                        s.reactions.push(
                            Reaction::ServerMessage(ServerMessage::ConnectionError {
                                payload: ConnectionErrorPayload {
                                    message: e.to_string(),
                                },
                            })
                            .into_stream(),
                        );
                        ConnectionSinkState::Ready { state }
                    }
                }
            }
            _ => panic!("start_send called when not ready"),
        };
        Ok(())
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        <Self as Sink<T>>::poll_ready(self, cx)
    }

    fn poll_close(mut self: Pin<&mut Self>, _cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        self.sink_state = ConnectionSinkState::Closed;
        if let Some(waker) = self.stream_waker.take() {
            // Wake up the stream so it can close too.
            waker.wake();
        }
        Poll::Ready(Ok(()))
    }
}

impl<S, I> Stream for Connection<S, I>
where
    S: Schema,
    I: Init<S::ScalarValue, S::Context>,
{
    type Item = ServerMessage<S::ScalarValue>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        self.stream_waker = Some(cx.waker().clone());

        if let ConnectionSinkState::Closed = self.sink_state {
            return Poll::Ready(None);
        }

        // Poll the reactions for new outgoing messages.
        loop {
            if !self.reactions.is_empty() {
                match Pin::new(&mut self.reactions).poll_next(cx) {
                    Poll::Ready(Some(reaction)) => match reaction {
                        Reaction::ServerMessage(msg) => return Poll::Ready(Some(msg)),
                        Reaction::EndStream => return Poll::Ready(None),
                    },
                    Poll::Ready(None) => {
                        // In rare cases, the reaction stream may terminate. For example, this will
                        // happen if the first message we receive does not require any reaction. Just
                        // recreate it in that case.
                        self.reactions = SelectAll::new();
                        return Poll::Pending;
                    }
                    Poll::Pending => return Poll::Pending,
                }
            } else {
                return Poll::Pending;
            }
        }
    }
}

#[cfg(test)]
mod test {
    use std::{convert::Infallible, io};

    use juniper::{
        futures::{sink::SinkExt, stream::BoxStream},
        graphql_object, graphql_subscription,
        parser::{ParseError, Spanning, Token},
        DefaultScalarValue, EmptyMutation, FieldError, FieldResult, InputValue, RootNode, Value,
    };

    use super::*;

    struct Context(i32);

    struct Query;

    #[graphql_object(context = Context)]
    impl Query {
        /// context just resolves to the current context.
        async fn context(context: &Context) -> i32 {
            context.0
        }
    }

    struct Subscription;

    #[graphql_subscription(context = Context)]
    impl Subscription {
        /// never never emits anything.
        async fn never(context: &Context) -> BoxStream<'static, FieldResult<i32>> {
            tokio::time::delay_for(Duration::from_secs(10000))
                .map(|_| unreachable!())
                .into_stream()
                .boxed()
        }

        /// context emits the current context once, then never emits anything else.
        async fn context(context: &Context) -> BoxStream<'static, FieldResult<i32>> {
            stream::once(future::ready(Ok(context.0)))
                .chain(
                    tokio::time::delay_for(Duration::from_secs(10000))
                        .map(|_| unreachable!())
                        .into_stream(),
                )
                .boxed()
        }

        /// error emits an error once, then never emits anything else.
        async fn error(context: &Context) -> BoxStream<'static, FieldResult<i32>> {
            stream::once(future::ready(Err(FieldError::new(
                "field error",
                Value::null(),
            ))))
            .chain(
                tokio::time::delay_for(Duration::from_secs(10000))
                    .map(|_| unreachable!())
                    .into_stream(),
            )
            .boxed()
        }
    }

    type ClientMessage = super::ClientMessage<DefaultScalarValue>;
    type ServerMessage = super::ServerMessage<DefaultScalarValue>;

    fn new_test_schema() -> Arc<RootNode<'static, Query, EmptyMutation<Context>, Subscription>> {
        Arc::new(RootNode::new(Query, EmptyMutation::new(), Subscription))
    }

    #[tokio::test]
    async fn test_query() {
        let mut conn = Connection::new(
            new_test_schema(),
            ConnectionConfig::new(Context(1)).with_keep_alive_interval(Duration::from_secs(0)),
        );

        conn.send(ClientMessage::ConnectionInit {
            payload: Variables::default(),
        })
        .await
        .unwrap();

        assert_eq!(ServerMessage::ConnectionAck, conn.next().await.unwrap());

        conn.send(ClientMessage::Start {
            id: "foo".to_string(),
            payload: StartPayload {
                query: "{context}".to_string(),
                variables: Variables::default(),
                operation_name: None,
            },
        })
        .await
        .unwrap();

        assert_eq!(
            ServerMessage::Data {
                id: "foo".to_string(),
                payload: DataPayload {
                    data: Value::Object(
                        [("context", Value::Scalar(DefaultScalarValue::Int(1)))]
                            .iter()
                            .cloned()
                            .collect()
                    ),
                    errors: vec![],
                },
            },
            conn.next().await.unwrap()
        );

        assert_eq!(
            ServerMessage::Complete {
                id: "foo".to_string(),
            },
            conn.next().await.unwrap()
        );
    }

    #[tokio::test]
    async fn test_subscriptions() {
        let mut conn = Connection::new(
            new_test_schema(),
            ConnectionConfig::new(Context(1)).with_keep_alive_interval(Duration::from_secs(0)),
        );

        conn.send(ClientMessage::ConnectionInit {
            payload: Variables::default(),
        })
        .await
        .unwrap();

        assert_eq!(ServerMessage::ConnectionAck, conn.next().await.unwrap());

        conn.send(ClientMessage::Start {
            id: "foo".to_string(),
            payload: StartPayload {
                query: "subscription Foo {context}".to_string(),
                variables: Variables::default(),
                operation_name: None,
            },
        })
        .await
        .unwrap();

        assert_eq!(
            ServerMessage::Data {
                id: "foo".to_string(),
                payload: DataPayload {
                    data: Value::Object([("context", Value::scalar(1))].iter().cloned().collect()),
                    errors: vec![],
                },
            },
            conn.next().await.unwrap()
        );

        conn.send(ClientMessage::Start {
            id: "bar".to_string(),
            payload: StartPayload {
                query: "subscription Bar {context}".to_string(),
                variables: Variables::default(),
                operation_name: None,
            },
        })
        .await
        .unwrap();

        assert_eq!(
            ServerMessage::Data {
                id: "bar".to_string(),
                payload: DataPayload {
                    data: Value::Object([("context", Value::scalar(1))].iter().cloned().collect()),
                    errors: vec![],
                },
            },
            conn.next().await.unwrap()
        );

        conn.send(ClientMessage::Stop {
            id: "foo".to_string(),
        })
        .await
        .unwrap();

        assert_eq!(
            ServerMessage::Complete {
                id: "foo".to_string(),
            },
            conn.next().await.unwrap()
        );
    }

    #[tokio::test]
    async fn test_init_params_ok() {
        let mut conn = Connection::new(new_test_schema(), |params: Variables| async move {
            assert_eq!(params.get("foo"), Some(&InputValue::scalar("bar")));
            Ok(ConnectionConfig::new(Context(1))) as Result<_, Infallible>
        });

        conn.send(ClientMessage::ConnectionInit {
            payload: [("foo".to_string(), InputValue::scalar("bar".to_string()))]
                .iter()
                .cloned()
                .collect(),
        })
        .await
        .unwrap();

        assert_eq!(ServerMessage::ConnectionAck, conn.next().await.unwrap());
    }

    #[tokio::test]
    async fn test_init_params_error() {
        let mut conn = Connection::new(new_test_schema(), |params: Variables| async move {
            assert_eq!(params.get("foo"), Some(&InputValue::scalar("bar")));
            Err(io::Error::new(io::ErrorKind::Other, "init error"))
        });

        conn.send(ClientMessage::ConnectionInit {
            payload: [("foo".to_string(), InputValue::scalar("bar".to_string()))]
                .iter()
                .cloned()
                .collect(),
        })
        .await
        .unwrap();

        assert_eq!(
            ServerMessage::ConnectionError {
                payload: ConnectionErrorPayload {
                    message: "init error".to_string(),
                },
            },
            conn.next().await.unwrap()
        );
    }

    #[tokio::test]
    async fn test_max_in_flight_operations() {
        let mut conn = Connection::new(
            new_test_schema(),
            ConnectionConfig::new(Context(1))
                .with_keep_alive_interval(Duration::from_secs(0))
                .with_max_in_flight_operations(1),
        );

        conn.send(ClientMessage::ConnectionInit {
            payload: Variables::default(),
        })
        .await
        .unwrap();

        assert_eq!(ServerMessage::ConnectionAck, conn.next().await.unwrap());

        conn.send(ClientMessage::Start {
            id: "foo".to_string(),
            payload: StartPayload {
                query: "subscription Foo {never}".to_string(),
                variables: Variables::default(),
                operation_name: None,
            },
        })
        .await
        .unwrap();

        conn.send(ClientMessage::Start {
            id: "bar".to_string(),
            payload: StartPayload {
                query: "subscription Bar {never}".to_string(),
                variables: Variables::default(),
                operation_name: None,
            },
        })
        .await
        .unwrap();

        match conn.next().await.unwrap() {
            ServerMessage::Error { id, .. } => {
                assert_eq!(id, "bar");
            }
            msg @ _ => panic!("expected error, got: {:?}", msg),
        }
    }

    #[tokio::test]
    async fn test_parse_error() {
        let mut conn = Connection::new(
            new_test_schema(),
            ConnectionConfig::new(Context(1)).with_keep_alive_interval(Duration::from_secs(0)),
        );

        conn.send(ClientMessage::ConnectionInit {
            payload: Variables::default(),
        })
        .await
        .unwrap();

        assert_eq!(ServerMessage::ConnectionAck, conn.next().await.unwrap());

        conn.send(ClientMessage::Start {
            id: "foo".to_string(),
            payload: StartPayload {
                query: "asd".to_string(),
                variables: Variables::default(),
                operation_name: None,
            },
        })
        .await
        .unwrap();

        match conn.next().await.unwrap() {
            ServerMessage::Error { id, payload } => {
                assert_eq!(id, "foo");
                match payload.graphql_error() {
                    GraphQLError::ParseError(Spanning {
                        item: ParseError::UnexpectedToken(Token::Name("asd")),
                        ..
                    }) => {}
                    p @ _ => panic!("expected graphql parse error, got: {:?}", p),
                }
            }
            msg @ _ => panic!("expected error, got: {:?}", msg),
        }
    }

    #[tokio::test]
    async fn test_keep_alives() {
        let mut conn = Connection::new(
            new_test_schema(),
            ConnectionConfig::new(Context(1)).with_keep_alive_interval(Duration::from_millis(20)),
        );

        conn.send(ClientMessage::ConnectionInit {
            payload: Variables::default(),
        })
        .await
        .unwrap();

        assert_eq!(ServerMessage::ConnectionAck, conn.next().await.unwrap());

        for _ in 0..10 {
            assert_eq!(
                ServerMessage::ConnectionKeepAlive,
                conn.next().await.unwrap()
            );
        }
    }

    #[tokio::test]
    async fn test_slow_init() {
        let mut conn = Connection::new(
            new_test_schema(),
            ConnectionConfig::new(Context(1)).with_keep_alive_interval(Duration::from_secs(0)),
        );

        conn.send(ClientMessage::ConnectionInit {
            payload: Variables::default(),
        })
        .await
        .unwrap();

        // If we send the start message before the init is handled, we should still get results.
        conn.send(ClientMessage::Start {
            id: "foo".to_string(),
            payload: StartPayload {
                query: "{context}".to_string(),
                variables: Variables::default(),
                operation_name: None,
            },
        })
        .await
        .unwrap();

        assert_eq!(ServerMessage::ConnectionAck, conn.next().await.unwrap());

        assert_eq!(
            ServerMessage::Data {
                id: "foo".to_string(),
                payload: DataPayload {
                    data: Value::Object(
                        [("context", Value::Scalar(DefaultScalarValue::Int(1)))]
                            .iter()
                            .cloned()
                            .collect()
                    ),
                    errors: vec![],
                },
            },
            conn.next().await.unwrap()
        );
    }

    #[tokio::test]
    async fn test_subscription_field_error() {
        let mut conn = Connection::new(
            new_test_schema(),
            ConnectionConfig::new(Context(1)).with_keep_alive_interval(Duration::from_secs(0)),
        );

        conn.send(ClientMessage::ConnectionInit {
            payload: Variables::default(),
        })
        .await
        .unwrap();

        assert_eq!(ServerMessage::ConnectionAck, conn.next().await.unwrap());

        conn.send(ClientMessage::Start {
            id: "foo".to_string(),
            payload: StartPayload {
                query: "subscription Foo {error}".to_string(),
                variables: Variables::default(),
                operation_name: None,
            },
        })
        .await
        .unwrap();

        match conn.next().await.unwrap() {
            ServerMessage::Data {
                id,
                payload: DataPayload { data, errors },
            } => {
                assert_eq!(id, "foo");
                assert_eq!(
                    data,
                    Value::Object([("error", Value::null())].iter().cloned().collect())
                );
                assert_eq!(errors.len(), 1);
            }
            msg @ _ => panic!("expected data, got: {:?}", msg),
        }
    }
}
