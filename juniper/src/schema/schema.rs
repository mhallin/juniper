use crate::{
    ast::Selection,
    executor::{ExecutionResult, Executor, Registry},
    types::base::{Arguments, GraphQLType, TypeKind},
    value::{ScalarRefValue, ScalarValue, Value},
};

use crate::schema::{
    meta::{
        Argument, EnumMeta, EnumValue, Field, InputObjectMeta, InterfaceMeta, MetaType, ObjectMeta,
        UnionMeta,
    },
    model::{DirectiveLocation, DirectiveType, RootNode, SchemaType, TypeType},
};

impl<'a, CtxT, S, QueryT, MutationT> GraphQLType<S> for RootNode<'a, QueryT, MutationT, S>
where
    S: ScalarValue,
    QueryT: GraphQLType<S, Context = CtxT>,
    MutationT: GraphQLType<S, Context = CtxT>,
    for<'b> &'b S: ScalarRefValue<'b>,
{
    type Context = CtxT;
    type TypeInfo = QueryT::TypeInfo;

    fn name(info: &QueryT::TypeInfo) -> Option<&str> {
        QueryT::name(info)
    }

    fn meta<'r>(info: &QueryT::TypeInfo, registry: &mut Registry<'r, S>) -> MetaType<'r, S>
    where
        S: 'r,
        for<'b> &'b S: ScalarRefValue<'b>,
    {
        QueryT::meta(info, registry)
    }

    fn resolve_field(
        &self,
        info: &QueryT::TypeInfo,
        field: &str,
        args: &Arguments<S>,
        executor: &Executor<CtxT, S>,
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
    ) -> Value<S> {
        use crate::{types::base::resolve_selection_set_into, value::Object};
        if let Some(selection_set) = selection_set {
            let mut result = Object::with_capacity(selection_set.len());
            if resolve_selection_set_into(self, info, selection_set, executor, &mut result) {
                Value::Object(result)
            } else {
                Value::null()
            }
        } else {
            panic!("resolve() must be implemented by non-object output types");
        }
    }
}

#[crate::object_internal(
    name = "__Schema"
    Context = SchemaType<'a, S>,
    Scalar = S,
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
                    .map(|t| t.name() != Some("_EmptyMutation"))
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

#[crate::object_internal(
    name = "__Type"
    Context = SchemaType<'a, S>,
    Scalar = S,
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

    fn fields(
        &self,
        #[graphql(default = false)] include_deprecated: bool,
    ) -> Option<Vec<&Field<S>>> {
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

    fn enum_values(
        &self,
        #[graphql(default = false)] include_deprecated: bool,
    ) -> Option<Vec<&EnumValue>> {
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

#[crate::object_internal(
    name = "__Field",
    Context = SchemaType<'a, S>,
    Scalar = S,
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

#[crate::object_internal(
    name = "__InputValue",
    Context = SchemaType<'a, S>,
    Scalar = S,
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

#[crate::object_internal(
    name = "__EnumValue",
    Scalar = S,
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

#[crate::object_internal(
    name = "__Directive",
    Context = SchemaType<'a, S>,
    Scalar = S,
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
