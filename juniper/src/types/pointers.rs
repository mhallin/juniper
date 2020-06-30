use std::{fmt, sync::Arc};

use crate::{
    ast::{FromInputValue, InputValue, Selection, ToInputValue},
    executor::{ExecutionResult, Executor, Registry},
    schema::meta::MetaType,
    types::{
        async_await::GraphQLValueAsync,
        base::{Arguments, GraphQLType, GraphQLValue},
    },
    value::ScalarValue,
    BoxFuture,
};

impl<S, T, CtxT> GraphQLType<S> for Box<T>
where
    T: GraphQLType<S, Context = CtxT> + ?Sized,
    S: ScalarValue,
{
    fn name(info: &T::TypeInfo) -> Option<&str> {
        T::name(info)
    }

    fn meta<'r>(info: &T::TypeInfo, registry: &mut Registry<'r, S>) -> MetaType<'r, S>
    where
        S: 'r,
    {
        T::meta(info, registry)
    }
}

impl<S, T, CtxT> GraphQLValue<S> for Box<T>
where
    S: ScalarValue,
    T: GraphQLValue<S, Context = CtxT> + ?Sized,
{
    type Context = CtxT;
    type TypeInfo = T::TypeInfo;

    fn type_name<'i>(&self, info: &'i T::TypeInfo) -> Option<&'i str> {
        (**self).type_name(info)
    }

    fn resolve_into_type(
        &self,
        info: &T::TypeInfo,
        name: &str,
        selection_set: Option<&[Selection<S>]>,
        executor: &Executor<CtxT, S>,
    ) -> ExecutionResult<S> {
        (**self).resolve_into_type(info, name, selection_set, executor)
    }

    fn resolve_field(
        &self,
        info: &T::TypeInfo,
        field: &str,
        args: &Arguments<S>,
        executor: &Executor<CtxT, S>,
    ) -> ExecutionResult<S> {
        (**self).resolve_field(info, field, args, executor)
    }

    fn resolve(
        &self,
        info: &T::TypeInfo,
        selection_set: Option<&[Selection<S>]>,
        executor: &Executor<CtxT, S>,
    ) -> ExecutionResult<S> {
        (**self).resolve(info, selection_set, executor)
    }
}

impl<S, T> GraphQLValueAsync<S> for Box<T>
where
    T: GraphQLValueAsync<S> + ?Sized,
    T::TypeInfo: Sync,
    T::Context: Sync,
    S: ScalarValue + Send + Sync,
{
    fn resolve_async<'a>(
        &'a self,
        info: &'a Self::TypeInfo,
        selection_set: Option<&'a [Selection<S>]>,
        executor: &'a Executor<Self::Context, S>,
    ) -> BoxFuture<'a, ExecutionResult<S>> {
        (**self).resolve_async(info, selection_set, executor)
    }
}

impl<T, S> FromInputValue<S> for Box<T>
where
    S: ScalarValue,
    T: FromInputValue<S>,
{
    fn from_input_value<'a>(v: &'a InputValue<S>) -> Option<Box<T>> {
        match <T as FromInputValue<S>>::from_input_value(v) {
            Some(v) => Some(Box::new(v)),
            None => None,
        }
    }
}

impl<T, S> ToInputValue<S> for Box<T>
where
    S: fmt::Debug,
    T: ToInputValue<S>,
{
    fn to_input_value(&self) -> InputValue<S> {
        (**self).to_input_value()
    }
}

impl<'e, S, T, CtxT> GraphQLType<S> for &'e T
where
    T: GraphQLType<S, Context = CtxT> + ?Sized,
    S: ScalarValue,
{
    fn name(info: &T::TypeInfo) -> Option<&str> {
        T::name(info)
    }

    fn meta<'r>(info: &T::TypeInfo, registry: &mut Registry<'r, S>) -> MetaType<'r, S>
    where
        S: 'r,
    {
        T::meta(info, registry)
    }
}

impl<'e, S, T, CtxT> GraphQLValue<S> for &'e T
where
    S: ScalarValue,
    T: GraphQLValue<S, Context = CtxT> + ?Sized,
{
    type Context = CtxT;
    type TypeInfo = T::TypeInfo;

    fn type_name<'i>(&self, info: &'i T::TypeInfo) -> Option<&'i str> {
        (**self).type_name(info)
    }

    fn resolve_into_type(
        &self,
        info: &T::TypeInfo,
        name: &str,
        selection_set: Option<&[Selection<S>]>,
        executor: &Executor<CtxT, S>,
    ) -> ExecutionResult<S> {
        (**self).resolve_into_type(info, name, selection_set, executor)
    }

    fn resolve_field(
        &self,
        info: &T::TypeInfo,
        field: &str,
        args: &Arguments<S>,
        executor: &Executor<CtxT, S>,
    ) -> ExecutionResult<S> {
        (**self).resolve_field(info, field, args, executor)
    }

    fn resolve(
        &self,
        info: &T::TypeInfo,
        selection_set: Option<&[Selection<S>]>,
        executor: &Executor<CtxT, S>,
    ) -> ExecutionResult<S> {
        (**self).resolve(info, selection_set, executor)
    }
}

impl<'e, S, T> GraphQLValueAsync<S> for &'e T
where
    T: GraphQLValueAsync<S> + ?Sized,
    T::TypeInfo: Sync,
    T::Context: Sync,
    S: ScalarValue + Send + Sync,
{
    fn resolve_field_async<'b>(
        &'b self,
        info: &'b Self::TypeInfo,
        field_name: &'b str,
        arguments: &'b Arguments<S>,
        executor: &'b Executor<Self::Context, S>,
    ) -> BoxFuture<'b, ExecutionResult<S>> {
        (**self).resolve_field_async(info, field_name, arguments, executor)
    }

    fn resolve_async<'a>(
        &'a self,
        info: &'a Self::TypeInfo,
        selection_set: Option<&'a [Selection<S>]>,
        executor: &'a Executor<Self::Context, S>,
    ) -> BoxFuture<'a, ExecutionResult<S>> {
        (**self).resolve_async(info, selection_set, executor)
    }
}

impl<'a, T, S> ToInputValue<S> for &'a T
where
    S: fmt::Debug,
    T: ToInputValue<S>,
{
    fn to_input_value(&self) -> InputValue<S> {
        (**self).to_input_value()
    }
}

impl<S, T> GraphQLType<S> for Arc<T>
where
    S: ScalarValue,
    T: GraphQLType<S> + ?Sized,
{
    fn name(info: &T::TypeInfo) -> Option<&str> {
        T::name(info)
    }

    fn meta<'r>(info: &T::TypeInfo, registry: &mut Registry<'r, S>) -> MetaType<'r, S>
    where
        S: 'r,
    {
        T::meta(info, registry)
    }
}

impl<S, T> GraphQLValue<S> for Arc<T>
where
    S: ScalarValue,
    T: GraphQLValue<S> + ?Sized,
{
    type Context = T::Context;
    type TypeInfo = T::TypeInfo;

    fn type_name<'i>(&self, info: &'i T::TypeInfo) -> Option<&'i str> {
        (**self).type_name(info)
    }

    fn resolve_into_type(
        &self,
        info: &T::TypeInfo,
        name: &str,
        selection_set: Option<&[Selection<S>]>,
        executor: &Executor<T::Context, S>,
    ) -> ExecutionResult<S> {
        (**self).resolve_into_type(info, name, selection_set, executor)
    }

    fn resolve_field(
        &self,
        info: &T::TypeInfo,
        field: &str,
        args: &Arguments<S>,
        executor: &Executor<T::Context, S>,
    ) -> ExecutionResult<S> {
        (**self).resolve_field(info, field, args, executor)
    }

    fn resolve(
        &self,
        info: &T::TypeInfo,
        selection_set: Option<&[Selection<S>]>,
        executor: &Executor<T::Context, S>,
    ) -> ExecutionResult<S> {
        (**self).resolve(info, selection_set, executor)
    }
}

impl<'e, S, T> GraphQLValueAsync<S> for Arc<T>
where
    T: GraphQLValueAsync<S> + Send + ?Sized,
    T::TypeInfo: Sync,
    T::Context: Sync,
    S: ScalarValue + Send + Sync,
{
    fn resolve_async<'a>(
        &'a self,
        info: &'a Self::TypeInfo,
        selection_set: Option<&'a [Selection<S>]>,
        executor: &'a Executor<Self::Context, S>,
    ) -> BoxFuture<'a, ExecutionResult<S>> {
        (**self).resolve_async(info, selection_set, executor)
    }
}

impl<T, S> ToInputValue<S> for Arc<T>
where
    S: fmt::Debug,
    T: ToInputValue<S>,
{
    fn to_input_value(&self) -> InputValue<S> {
        (**self).to_input_value()
    }
}
