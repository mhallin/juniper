use crate::{
    ast::Selection,
    executor::{ExecutionResult, Executor},
    parser::Spanning,
    value::{DefaultScalarValue, Object, ScalarValue, Value},
};

use crate::LocalBoxFuture;

use super::base::{is_excluded, merge_key_into, Arguments, GraphQLType, GraphQLValue};

/// Extension of [`GraphQLValue`] trait with asynchronous queries/mutations resolvers.
///
/// Convenience macros related to asynchronous queries/mutations expand into an implementation of
/// this trait and [`GraphQLValue`] for the given type.
pub trait GraphQLValueAsync<S = DefaultScalarValue>: GraphQLValue<S> + Sync
where
    Self::TypeInfo: Sync,
    S: ScalarValue + Send + Sync,
{
    /// Resolves the value of a single field on this [`GraphQLValueAsync`].
    ///
    /// The `arguments` object contains all the specified arguments, with default values being
    /// substituted for the ones not provided by the query.
    ///
    /// The `executor` can be used to drive selections into sub-[objects][3].
    ///
    /// # Panics
    ///
    /// The default implementation panics.
    ///
    /// [3]: https://spec.graphql.org/June2018/#sec-Objects
    fn resolve_field_async<'a>(
        &'a self,
        _info: &'a Self::TypeInfo,
        _field_name: &'a str,
        _arguments: &'a Arguments<S>,
        _executor: &'a Executor<Self::Context, S>,
    ) -> LocalBoxFuture<'a, ExecutionResult<S>> {
        panic!(
            "GraphQLValueAsync::resolve_field_async() must be implemented by objects and \
             interfaces",
        );
    }

    /// Resolves this [`GraphQLValueAsync`] (being an [interface][1] or an [union][2]) into a
    /// concrete downstream [object][3] type.
    ///
    /// Tries to resolve this [`GraphQLValueAsync`] into the provided `type_name`. If the type
    /// matches, then passes the instance along to [`Executor::resolve`].
    ///
    /// # Panics
    ///
    /// The default implementation panics.
    ///
    /// [1]: https://spec.graphql.org/June2018/#sec-Interfaces
    /// [2]: https://spec.graphql.org/June2018/#sec-Unions
    /// [3]: https://spec.graphql.org/June2018/#sec-Objects
    fn resolve_into_type_async<'a>(
        &'a self,
        info: &'a Self::TypeInfo,
        type_name: &str,
        selection_set: Option<&'a [Selection<'a, S>]>,
        executor: &'a Executor<'a, 'a, Self::Context, S>,
    ) -> LocalBoxFuture<'a, ExecutionResult<S>> {
        if self.type_name(info).unwrap() == type_name {
            self.resolve_async(info, selection_set, executor)
        } else {
            panic!(
                "GraphQLValueAsync::resolve_into_type_async() must be implemented by unions and \
                 interfaces",
            );
        }
    }

    /// Resolves the provided `selection_set` against this [`GraphQLValueAsync`].
    ///
    /// For non-[object][3] types, the `selection_set` will be [`None`] and the value should simply
    /// be returned.
    ///
    /// For [objects][3], all fields in the `selection_set` should be resolved. The default
    /// implementation uses [`GraphQLValueAsync::resolve_field_async`] to resolve all fields,
    /// including those through a fragment expansion.
    ///
    /// Since the [GraphQL spec specifies][0] that errors during field processing should result in
    /// a null-value, this might return `Ok(Null)` in case of a failure. Errors are recorded
    /// internally.
    ///
    /// # Panics
    ///
    /// The default implementation panics, if `selection_set` is [`None`].
    ///
    /// [0]: https://spec.graphql.org/June2018/#sec-Errors-and-Non-Nullability
    /// [3]: https://spec.graphql.org/June2018/#sec-Objects
    fn resolve_async<'a>(
        &'a self,
        info: &'a Self::TypeInfo,
        selection_set: Option<&'a [Selection<S>]>,
        executor: &'a Executor<Self::Context, S>,
    ) -> LocalBoxFuture<'a, ExecutionResult<S>> {
        if let Some(sel) = selection_set {
            Box::pin(async move {
                Ok(resolve_selection_set_into_async(self, info, sel, executor).await)
            })
        } else {
            panic!(
                "GraphQLValueAsync::resolve_async() must be implemented by non-object output types",
            );
        }
    }
}

crate::sa::assert_obj_safe!(GraphQLValueAsync<Context = (), TypeInfo = ()>);

/// Helper alias for naming [trait objects][1] of [`GraphQLValueAsync`].
///
/// [1]: https://doc.rust-lang.org/reference/types/trait-object.html
pub type DynGraphQLValueAsync<S, C, TI> =
    dyn GraphQLValueAsync<S, Context = C, TypeInfo = TI> + Send + 'static;

/// Extension of [`GraphQLType`] trait with asynchronous queries/mutations resolvers.
///
/// It's automatically implemented for [`GraphQLValueAsync`] and [`GraphQLType`] implementers, so
/// doesn't require manual or code-generated implementation.
pub trait GraphQLTypeAsync<S = DefaultScalarValue>: GraphQLValueAsync<S> + GraphQLType<S>
where
    Self::TypeInfo: Sync,
    S: ScalarValue + Send + Sync,
{
}

impl<S, T> GraphQLTypeAsync<S> for T
where
    T: GraphQLValueAsync<S> + GraphQLType<S> + ?Sized,
    T::TypeInfo: Sync,
    S: ScalarValue + Send + Sync,
{
}

// Wrapper function around resolve_selection_set_into_async_recursive.
// This wrapper is necessary because async fns can not be recursive.
fn resolve_selection_set_into_async<'a, 'e, T, S>(
    instance: &'a T,
    info: &'a T::TypeInfo,
    selection_set: &'e [Selection<'e, S>],
    executor: &'e Executor<'e, 'e, T::Context, S>,
) -> LocalBoxFuture<'a, Value<S>>
where
    T: GraphQLValueAsync<S> + ?Sized,
    T::TypeInfo: Sync,
    S: ScalarValue + Send + Sync,
    'e: 'a,
{
    Box::pin(resolve_selection_set_into_async_recursive(
        instance,
        info,
        selection_set,
        executor,
    ))
}

struct AsyncField<S> {
    name: String,
    value: Option<Value<S>>,
}

enum AsyncValue<S> {
    Field(AsyncField<S>),
    Nested(Value<S>),
}

pub(crate) async fn resolve_selection_set_into_async_recursive<'a, T, S>(
    instance: &'a T,
    info: &'a T::TypeInfo,
    selection_set: &'a [Selection<'a, S>],
    executor: &'a Executor<'a, 'a, T::Context, S>,
) -> Value<S>
where
    T: GraphQLValueAsync<S> + ?Sized,
    T::TypeInfo: Sync,
    S: ScalarValue + Send + Sync,
{
    use futures::stream::{FuturesOrdered, StreamExt as _};

    #[derive(futures_enum::Future)]
    enum AsyncValueFuture<A, B, C, D> {
        Field(A),
        FragmentSpread(B),
        InlineFragment1(C),
        InlineFragment2(D),
    }

    let mut object = Object::with_capacity(selection_set.len());

    let mut async_values = FuturesOrdered::<AsyncValueFuture<_, _, _, _>>::new();

    let meta_type = executor
        .schema()
        .concrete_type_by_name(
            instance
                .type_name(info)
                .expect("Resolving named type's selection set")
                .as_ref(),
        )
        .expect("Type not found in schema");

    for selection in selection_set {
        match *selection {
            Selection::Field(Spanning {
                item: ref f,
                start: ref start_pos,
                ..
            }) => {
                if is_excluded(&f.directives, executor.variables()) {
                    continue;
                }

                let response_name = f.alias.as_ref().unwrap_or(&f.name).item;

                if f.name.item == "__typename" {
                    object.add_field(
                        response_name,
                        Value::scalar(instance.concrete_type_name(executor.context(), info)),
                    );
                    continue;
                }

                let meta_field = meta_type.field_by_name(f.name.item).unwrap_or_else(|| {
                    panic!(format!(
                        "Field {} not found on type {:?}",
                        f.name.item,
                        meta_type.name()
                    ))
                });

                let exec_vars = executor.variables();

                let sub_exec = executor.field_sub_executor(
                    &response_name,
                    f.name.item,
                    *start_pos,
                    f.selection_set.as_ref().map(|v| &v[..]),
                );
                let args = Arguments::new(
                    f.arguments.as_ref().map(|m| {
                        m.item
                            .iter()
                            .map(|&(ref k, ref v)| (k.item, v.item.clone().into_const(exec_vars)))
                            .collect()
                    }),
                    &meta_field.arguments,
                );

                let pos = *start_pos;
                let is_non_null = meta_field.field_type.is_non_null();

                let response_name = response_name.to_string();
                async_values.push(AsyncValueFuture::Field(async move {
                    // TODO: implement custom future type instead of
                    //       two-level boxing.
                    let res = instance
                        .resolve_field_async(info, f.name.item, &args, &sub_exec)
                        .await;

                    let value = match res {
                        Ok(Value::Null) if is_non_null => None,
                        Ok(v) => Some(v),
                        Err(e) => {
                            sub_exec.push_error_at(e, pos);

                            if is_non_null {
                                None
                            } else {
                                Some(Value::null())
                            }
                        }
                    };
                    AsyncValue::Field(AsyncField {
                        name: response_name,
                        value,
                    })
                }));
            }

            Selection::FragmentSpread(Spanning {
                item: ref spread, ..
            }) => {
                if is_excluded(&spread.directives, executor.variables()) {
                    continue;
                }
                async_values.push(AsyncValueFuture::FragmentSpread(async move {
                    let fragment = &executor
                        .fragment_by_name(spread.name.item)
                        .expect("Fragment could not be found");
                    let value = resolve_selection_set_into_async(
                        instance,
                        info,
                        &fragment.selection_set[..],
                        executor,
                    )
                    .await;
                    AsyncValue::Nested(value)
                }));
            }

            Selection::InlineFragment(Spanning {
                item: ref fragment,
                start: ref start_pos,
                ..
            }) => {
                if is_excluded(&fragment.directives, executor.variables()) {
                    continue;
                }

                let sub_exec = executor.type_sub_executor(
                    fragment.type_condition.as_ref().map(|c| c.item),
                    Some(&fragment.selection_set[..]),
                );

                if let Some(ref type_condition) = fragment.type_condition {
                    // Check whether the type matches the type condition.
                    let concrete_type_name = instance.concrete_type_name(sub_exec.context(), info);
                    if type_condition.item == concrete_type_name {
                        let sub_result = instance
                            .resolve_into_type_async(
                                info,
                                type_condition.item,
                                Some(&fragment.selection_set[..]),
                                &sub_exec,
                            )
                            .await;

                        if let Ok(Value::Object(obj)) = sub_result {
                            for (k, v) in obj {
                                async_values.push(AsyncValueFuture::InlineFragment1(async move {
                                    AsyncValue::Field(AsyncField {
                                        name: k,
                                        value: Some(v),
                                    })
                                }));
                            }
                        } else if let Err(e) = sub_result {
                            sub_exec.push_error_at(e, *start_pos);
                        }
                    }
                } else {
                    async_values.push(AsyncValueFuture::InlineFragment2(async move {
                        let value = resolve_selection_set_into_async(
                            instance,
                            info,
                            &fragment.selection_set[..],
                            &sub_exec,
                        )
                        .await;
                        AsyncValue::Nested(value)
                    }));
                }
            }
        }
    }

    while let Some(item) = async_values.next().await {
        match item {
            AsyncValue::Field(AsyncField { name, value }) => {
                if let Some(value) = value {
                    merge_key_into(&mut object, &name, value);
                } else {
                    return Value::null();
                }
            }
            AsyncValue::Nested(obj) => match obj {
                v @ Value::Null => {
                    return v;
                }
                Value::Object(obj) => {
                    for (k, v) in obj {
                        merge_key_into(&mut object, &k, v);
                    }
                }
                _ => unreachable!(),
            },
        }
    }

    Value::Object(object)
}
