use juniper::{GraphQLSubscriptionType, GraphQLTypeAsync, RootNode, ScalarValue};
use std::sync::Arc;

/// Schema defines the requirements for schemas that can be used for operations. Typically this is
/// just an `Arc<RootNode<...>>` and you should not have to implement it yourself.
pub trait Schema: Unpin + Clone + Send + Sync + 'static {
    /// The context type.
    type Context: Unpin;

    /// The scalar value type.
    type ScalarValue: ScalarValue + Send + Sync;

    /// The query type info.
    type QueryTypeInfo: Send + Sync;

    /// The query type.
    type Query: GraphQLTypeAsync<
        Self::ScalarValue,
        Context = Self::Context,
        TypeInfo = Self::QueryTypeInfo,
    > + Send;

    /// The mutation type info.
    type MutationTypeInfo: Send + Sync;

    /// The mutation type.
    type Mutation: GraphQLTypeAsync<
        Self::ScalarValue,
        Context = Self::Context,
        TypeInfo = Self::MutationTypeInfo,
    > + Send;

    /// The subscription type info.
    type SubscriptionTypeInfo: Send + Sync;

    /// The subscription type.
    type Subscription: GraphQLSubscriptionType<
        Self::ScalarValue,
        Context = Self::Context,
        TypeInfo = Self::SubscriptionTypeInfo,
    > + Send;

    /// Returns the root node for the schema.
    fn root_node(
        &self,
    ) -> &RootNode<'static, Self::Query, Self::Mutation, Self::Subscription, Self::ScalarValue>;
}

/// This exists as a work-around for this issue: https://github.com/rust-lang/rust/issues/64552
///
/// It can be used in generators where using Arc directly would result in an error.
// TODO: Remove this once that issue is resolved.
#[doc(hidden)]
pub struct ArcSchema<QueryT, MutationT, SubscriptionT, CtxT, S>(
    pub Arc<RootNode<'static, QueryT, MutationT, SubscriptionT, S>>,
)
where
    QueryT: GraphQLTypeAsync<S, Context = CtxT> + Send + 'static,
    QueryT::TypeInfo: Send + Sync,
    MutationT: GraphQLTypeAsync<S, Context = CtxT> + Send + 'static,
    MutationT::TypeInfo: Send + Sync,
    SubscriptionT: GraphQLSubscriptionType<S, Context = CtxT> + Send + 'static,
    SubscriptionT::TypeInfo: Send + Sync,
    CtxT: Unpin,
    S: ScalarValue + Send + Sync + 'static;

impl<QueryT, MutationT, SubscriptionT, CtxT, S> Clone
    for ArcSchema<QueryT, MutationT, SubscriptionT, CtxT, S>
where
    QueryT: GraphQLTypeAsync<S, Context = CtxT> + Send + 'static,
    QueryT::TypeInfo: Send + Sync,
    MutationT: GraphQLTypeAsync<S, Context = CtxT> + Send + 'static,
    MutationT::TypeInfo: Send + Sync,
    SubscriptionT: GraphQLSubscriptionType<S, Context = CtxT> + Send + 'static,
    SubscriptionT::TypeInfo: Send + Sync,
    CtxT: Unpin,
    S: ScalarValue + Send + Sync + 'static,
{
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<QueryT, MutationT, SubscriptionT, CtxT, S> Schema
    for ArcSchema<QueryT, MutationT, SubscriptionT, CtxT, S>
where
    QueryT: GraphQLTypeAsync<S, Context = CtxT> + Send + 'static,
    QueryT::TypeInfo: Send + Sync,
    MutationT: GraphQLTypeAsync<S, Context = CtxT> + Send + 'static,
    MutationT::TypeInfo: Send + Sync,
    SubscriptionT: GraphQLSubscriptionType<S, Context = CtxT> + Send + 'static,
    SubscriptionT::TypeInfo: Send + Sync,
    CtxT: Unpin + 'static,
    S: ScalarValue + Send + Sync + 'static,
{
    type Context = CtxT;
    type ScalarValue = S;
    type QueryTypeInfo = QueryT::TypeInfo;
    type Query = QueryT;
    type MutationTypeInfo = MutationT::TypeInfo;
    type Mutation = MutationT;
    type SubscriptionTypeInfo = SubscriptionT::TypeInfo;
    type Subscription = SubscriptionT;

    fn root_node(&self) -> &RootNode<'static, QueryT, MutationT, SubscriptionT, S> {
        &self.0
    }
}

impl<QueryT, MutationT, SubscriptionT, CtxT, S> Schema
    for Arc<RootNode<'static, QueryT, MutationT, SubscriptionT, S>>
where
    QueryT: GraphQLTypeAsync<S, Context = CtxT> + Send + 'static,
    QueryT::TypeInfo: Send + Sync,
    MutationT: GraphQLTypeAsync<S, Context = CtxT> + Send + 'static,
    MutationT::TypeInfo: Send + Sync,
    SubscriptionT: GraphQLSubscriptionType<S, Context = CtxT> + Send + 'static,
    SubscriptionT::TypeInfo: Send + Sync,
    CtxT: Unpin,
    S: ScalarValue + Send + Sync + 'static,
{
    type Context = CtxT;
    type ScalarValue = S;
    type QueryTypeInfo = QueryT::TypeInfo;
    type Query = QueryT;
    type MutationTypeInfo = MutationT::TypeInfo;
    type Mutation = MutationT;
    type SubscriptionTypeInfo = SubscriptionT::TypeInfo;
    type Subscription = SubscriptionT;

    fn root_node(&self) -> &RootNode<'static, QueryT, MutationT, SubscriptionT, S> {
        self
    }
}
