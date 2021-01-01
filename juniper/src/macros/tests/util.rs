use crate::{DefaultScalarValue, GraphQLType, GraphQLTypeAsync, RootNode, Value, Variables};

pub async fn run_query<Query, Mutation, Subscription>(query: &str) -> Value
where
    Query: GraphQLTypeAsync<DefaultScalarValue, TypeInfo = ()> + Default,
    Query::Context: Default,
    Mutation:
        GraphQLTypeAsync<DefaultScalarValue, TypeInfo = (), Context = Query::Context> + Default,
    Subscription:
        GraphQLType<DefaultScalarValue, TypeInfo = (), Context = Query::Context> + Default + Sync,
{
    let schema = RootNode::new(
        Query::default(),
        Mutation::default(),
        Subscription::default(),
    );
    let (result, errs) = crate::execute(
        query,
        None,
        &schema,
        &Variables::new(),
        &Query::Context::default(),
    )
    .await
    .expect("Execution failed");

    assert_eq!(errs, []);
    result
}

pub async fn run_info_query<Query, Mutation, Subscription>(type_name: &str) -> Value
where
    Query: GraphQLTypeAsync<DefaultScalarValue, TypeInfo = ()> + Default,
    Query::Context: Default,
    Mutation:
        GraphQLTypeAsync<DefaultScalarValue, TypeInfo = (), Context = Query::Context> + Default,
    Subscription:
        GraphQLType<DefaultScalarValue, TypeInfo = (), Context = Query::Context> + Default + Sync,
{
    let query = format!(
        r#"
    {{
        __type(name: "{}") {{
            name,
            description,
            fields {{
                name
                description
                args {{
                    name
                    description
                    type {{
                        name
                    }}
                }}
            }}
        }}
    }}
    "#,
        type_name
    );
    let result = run_query::<Query, Mutation, Subscription>(&query).await;
    result
        .as_object_value()
        .expect("Result is not an object")
        .get_field_value("__type")
        .expect("__type field missing")
        .clone()
}
