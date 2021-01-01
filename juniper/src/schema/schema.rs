use crate::{
    ast::Selection,
    executor::{ExecutionResult, Executor, Registry},
    types::{
        async_await::{GraphQLTypeAsync, GraphQLValueAsync},
        base::{Arguments, GraphQLType, GraphQLValue, TypeKind},
    },
    value::{ScalarValue, Value},
};

use crate::schema::{
    meta::{
        Argument, EnumMeta, EnumValue, Field, InputObjectMeta, InterfaceMeta, MetaType, ObjectMeta,
        UnionMeta,
    },
    model::{DirectiveLocation, DirectiveType, RootNode, SchemaType, TypeType},
};

impl<'a, S, QueryT, MutationT, SubscriptionT> GraphQLType<S>
    for RootNode<'a, QueryT, MutationT, SubscriptionT, S>
where
    S: ScalarValue,
    QueryT: GraphQLType<S>,
    MutationT: GraphQLType<S, Context = QueryT::Context>,
    SubscriptionT: GraphQLType<S, Context = QueryT::Context>,
{
    fn name(info: &Self::TypeInfo) -> Option<&str> {
        QueryT::name(info)
    }

    fn meta<'r>(info: &Self::TypeInfo, registry: &mut Registry<'r, S>) -> MetaType<'r, S>
    where
        S: 'r,
    {
        QueryT::meta(info, registry)
    }
}

impl<'a, S, QueryT, MutationT, SubscriptionT> GraphQLValue<S>
    for RootNode<'a, QueryT, MutationT, SubscriptionT, S>
where
    S: ScalarValue,
    QueryT: GraphQLType<S>,
    MutationT: GraphQLType<S, Context = QueryT::Context>,
    SubscriptionT: GraphQLType<S, Context = QueryT::Context>,
{
    type Context = QueryT::Context;
    type TypeInfo = QueryT::TypeInfo;

    fn type_name<'i>(&self, info: &'i Self::TypeInfo) -> Option<&'i str> {
        QueryT::name(info)
    }

    fn concrete_type_name(&self, context: &Self::Context, info: &Self::TypeInfo) -> String {
        self.query_type.concrete_type_name(context, info)
    }

    fn resolve_field(
        &self,
        info: &Self::TypeInfo,
        field: &str,
        args: &Arguments<S>,
        executor: &Executor<Self::Context, S>,
    ) -> ExecutionResult<S> {
        match field {
            "__schema" => executor
                .replaced_context(&self.schema)
                .resolve(&(), &self.schema),
            "__type" => {
                let type_name: String = args.get("name").unwrap();
                executor
                    .replaced_context(&self.schema)
                    .resolve(&(), &self.schema.type_by_name(&type_name))
            }
            _ => self.query_type.resolve_field(info, field, args, executor),
        }
    }

    fn resolve(
        &self,
        info: &Self::TypeInfo,
        selection_set: Option<&[Selection<S>]>,
        executor: &Executor<Self::Context, S>,
    ) -> ExecutionResult<S> {
        use crate::{types::base::resolve_selection_set_into, value::Object};
        if let Some(selection_set) = selection_set {
            let mut result = Object::with_capacity(selection_set.len());
            if resolve_selection_set_into(self, info, selection_set, executor, &mut result) {
                Ok(Value::Object(result))
            } else {
                Ok(Value::null())
            }
        } else {
            // TODO: this panic seems useless, investigate why it is here.
            panic!("resolve() must be implemented by non-object output types");
        }
    }
}

impl<'a, S, QueryT, MutationT, SubscriptionT> GraphQLValueAsync<S>
    for RootNode<'a, QueryT, MutationT, SubscriptionT, S>
where
    QueryT: GraphQLTypeAsync<S>,
    QueryT::TypeInfo: Sync,
    QueryT::Context: 'a,
    MutationT: GraphQLTypeAsync<S, Context = QueryT::Context>,
    MutationT::TypeInfo: Sync,
    SubscriptionT: GraphQLType<S, Context = QueryT::Context> + Sync,
    SubscriptionT::TypeInfo: Sync,
    S: ScalarValue + Send + Sync,
{
    fn resolve_field_async<'b>(
        &'b self,
        info: &'b Self::TypeInfo,
        field_name: &'b str,
        arguments: &'b Arguments<S>,
        executor: &'b Executor<Self::Context, S>,
    ) -> crate::LocalBoxFuture<'b, ExecutionResult<S>> {
        use futures::future::ready;
        match field_name {
            "__schema" | "__type" => {
                let v = self.resolve_field(info, field_name, arguments, executor);
                Box::pin(ready(v))
            }
            _ => self
                .query_type
                .resolve_field_async(info, field_name, arguments, executor),
        }
    }
}

#[crate::graphql_object(
    name = "__Schema"
    Context = SchemaType<'a, S>,
    Scalar = S,
    internal,
    // FIXME: make this redundant.
    noasync,
)]
impl<'a, S> SchemaType<'a, S>
where
    S: crate::ScalarValue + 'a,
{
    fn types(&self) -> Vec<TypeType<S>> {
        self.type_list()
            .into_iter()
            .filter(|t| {
                t.to_concrete()
                    .map(|t| {
                        !(t.name() == Some("_EmptyMutation")
                            || t.name() == Some("_EmptySubscription"))
                    })
                    .unwrap_or(false)
            })
            .collect::<Vec<_>>()
    }

    fn query_type(&self) -> TypeType<S> {
        self.query_type()
    }

    fn mutation_type(&self) -> Option<TypeType<S>> {
        self.mutation_type()
    }

    fn subscription_type(&self) -> Option<TypeType<S>> {
        self.subscription_type()
    }

    fn directives(&self) -> Vec<&DirectiveType<S>> {
        self.directive_list()
    }
}

#[crate::graphql_object(
    name = "__Type"
    Context = SchemaType<'a, S>,
    Scalar = S,
    internal,
    // FIXME: make this redundant.
    noasync,
)]
impl<'a, S> TypeType<'a, S>
where
    S: crate::ScalarValue + 'a,
{
    fn name(&self) -> Option<&str> {
        match *self {
            TypeType::Concrete(t) => t.name(),
            _ => None,
        }
    }

    fn description(&self) -> Option<&String> {
        match *self {
            TypeType::Concrete(t) => t.description(),
            _ => None,
        }
    }

    fn kind(&self) -> TypeKind {
        match *self {
            TypeType::Concrete(t) => t.type_kind(),
            TypeType::List(_) => TypeKind::List,
            TypeType::NonNull(_) => TypeKind::NonNull,
        }
    }

    #[graphql(arguments(include_deprecated(default = false)))]
    fn fields(&self, include_deprecated: bool) -> Option<Vec<&Field<S>>> {
        match *self {
            TypeType::Concrete(&MetaType::Interface(InterfaceMeta { ref fields, .. }))
            | TypeType::Concrete(&MetaType::Object(ObjectMeta { ref fields, .. })) => Some(
                fields
                    .iter()
                    .filter(|f| include_deprecated || !f.deprecation_status.is_deprecated())
                    .filter(|f| !f.name.starts_with("__"))
                    .collect(),
            ),
            _ => None,
        }
    }

    fn of_type(&self) -> Option<&Box<TypeType<S>>> {
        match *self {
            TypeType::Concrete(_) => None,
            TypeType::List(ref l) | TypeType::NonNull(ref l) => Some(l),
        }
    }

    fn input_fields(&self) -> Option<&Vec<Argument<S>>> {
        match *self {
            TypeType::Concrete(&MetaType::InputObject(InputObjectMeta {
                ref input_fields,
                ..
            })) => Some(input_fields),
            _ => None,
        }
    }

    fn interfaces(&self, schema: &SchemaType<'a, S>) -> Option<Vec<TypeType<S>>> {
        match *self {
            TypeType::Concrete(&MetaType::Object(ObjectMeta {
                ref interface_names,
                ..
            })) => Some(
                interface_names
                    .iter()
                    .filter_map(|n| schema.type_by_name(n))
                    .collect(),
            ),
            _ => None,
        }
    }

    fn possible_types(&self, schema: &SchemaType<'a, S>) -> Option<Vec<TypeType<S>>> {
        match *self {
            TypeType::Concrete(&MetaType::Union(UnionMeta {
                ref of_type_names, ..
            })) => Some(
                of_type_names
                    .iter()
                    .filter_map(|tn| schema.type_by_name(tn))
                    .collect(),
            ),
            TypeType::Concrete(&MetaType::Interface(InterfaceMeta {
                name: ref iface_name,
                ..
            })) => Some(
                schema
                    .concrete_type_list()
                    .iter()
                    .filter_map(|&ct| {
                        if let MetaType::Object(ObjectMeta {
                            ref name,
                            ref interface_names,
                            ..
                        }) = *ct
                        {
                            if interface_names.contains(&iface_name.to_string()) {
                                schema.type_by_name(name)
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    })
                    .collect(),
            ),
            _ => None,
        }
    }

    #[graphql(arguments(include_deprecated(default = false)))]
    fn enum_values(&self, include_deprecated: bool) -> Option<Vec<&EnumValue>> {
        match *self {
            TypeType::Concrete(&MetaType::Enum(EnumMeta { ref values, .. })) => Some(
                values
                    .iter()
                    .filter(|f| include_deprecated || !f.deprecation_status.is_deprecated())
                    .collect(),
            ),
            _ => None,
        }
    }
}

#[crate::graphql_object(
    name = "__Field",
    Context = SchemaType<'a, S>,
    Scalar = S,
    internal,
    // FIXME: make this redundant.
    noasync,
)]
impl<'a, S> Field<'a, S>
where
    S: crate::ScalarValue + 'a,
{
    fn name(&self) -> &String {
        &self.name
    }

    fn description(&self) -> &Option<String> {
        &self.description
    }

    fn args(&self) -> Vec<&Argument<S>> {
        self.arguments
            .as_ref()
            .map_or_else(Vec::new, |v| v.iter().collect())
    }

    #[graphql(name = "type")]
    fn _type(&self, context: &SchemaType<'a, S>) -> TypeType<S> {
        context.make_type(&self.field_type)
    }

    fn is_deprecated(&self) -> bool {
        self.deprecation_status.is_deprecated()
    }

    fn deprecation_reason(&self) -> Option<&String> {
        self.deprecation_status.reason()
    }
}

#[crate::graphql_object(
    name = "__InputValue",
    Context = SchemaType<'a, S>,
    Scalar = S,
    internal,
    // FIXME: make this redundant.
    noasync,
)]
impl<'a, S> Argument<'a, S>
where
    S: crate::ScalarValue + 'a,
{
    fn name(&self) -> &String {
        &self.name
    }

    fn description(&self) -> &Option<String> {
        &self.description
    }

    #[graphql(name = "type")]
    fn _type(&self, context: &SchemaType<'a, S>) -> TypeType<S> {
        context.make_type(&self.arg_type)
    }

    fn default_value(&self) -> Option<String> {
        self.default_value.as_ref().map(|v| format!("{}", v))
    }
}

#[crate::graphql_object(
    name = "__EnumValue",
    Scalar = S,
    internal,
    // FIXME: make this redundant.
    noasync,
)]
impl<'a, S> EnumValue
where
    S: crate::ScalarValue + 'a,
{
    fn name(&self) -> &String {
        &self.name
    }

    fn description(&self) -> &Option<String> {
        &self.description
    }

    fn is_deprecated(&self) -> bool {
        self.deprecation_status.is_deprecated()
    }

    fn deprecation_reason(&self) -> Option<&String> {
        self.deprecation_status.reason()
    }
}

#[crate::graphql_object(
    name = "__Directive",
    Context = SchemaType<'a, S>,
    Scalar = S,
    internal,
    // FIXME: make this redundant.
    noasync,
)]
impl<'a, S> DirectiveType<'a, S>
where
    S: crate::ScalarValue + 'a,
{
    fn name(&self) -> &String {
        &self.name
    }

    fn description(&self) -> &Option<String> {
        &self.description
    }

    fn locations(&self) -> &Vec<DirectiveLocation> {
        &self.locations
    }

    fn args(&self) -> &Vec<Argument<S>> {
        &self.arguments
    }

    // Included for compatibility with the introspection query in GraphQL.js
    #[graphql(deprecated = "Use the locations array instead")]
    fn on_operation(&self) -> bool {
        self.locations.contains(&DirectiveLocation::Query)
    }

    // Included for compatibility with the introspection query in GraphQL.js
    #[graphql(deprecated = "Use the locations array instead")]
    fn on_fragment(&self) -> bool {
        self.locations
            .contains(&DirectiveLocation::FragmentDefinition)
            || self.locations.contains(&DirectiveLocation::InlineFragment)
            || self.locations.contains(&DirectiveLocation::FragmentSpread)
    }

    // Included for compatibility with the introspection query in GraphQL.js
    #[graphql(deprecated = "Use the locations array instead")]
    fn on_field(&self) -> bool {
        self.locations.contains(&DirectiveLocation::Field)
    }
}
