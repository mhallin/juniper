use crate::{
    ast::InputValue,
    executor::Variables,
    graphql_object, graphql_scalar,
    parser::SourcePosition,
    schema::model::RootNode,
    types::scalars::{EmptyMutation, EmptySubscription},
    validation::RuleError,
    value::{DefaultScalarValue, Object, ParseScalarResult, ParseScalarValue, ScalarValue, Value},
    GraphQLError::ValidationError,
    GraphQLInputObject,
};

#[derive(Debug)]
struct TestComplexScalar;

#[graphql_scalar]
impl<S: ScalarValue> GraphQLScalar for TestComplexScalar {
    fn resolve(&self) -> Value {
        Value::scalar(String::from("SerializedValue"))
    }

    fn from_input_value(v: &InputValue) -> Option<TestComplexScalar> {
        if let Some(s) = v.as_scalar().and_then(ScalarValue::as_str) {
            if s == "SerializedValue" {
                return Some(TestComplexScalar);
            }
        }

        None
    }

    fn from_str<'a>(value: ScalarToken<'a>) -> ParseScalarResult<'a, S> {
        <String as ParseScalarValue<S>>::from_str(value)
    }
}

#[derive(GraphQLInputObject, Debug)]
struct TestInputObject {
    a: Option<String>,
    b: Option<Vec<Option<String>>>,
    c: String,
    d: Option<TestComplexScalar>,
}

#[derive(GraphQLInputObject, Debug)]
struct TestNestedInputObject {
    na: TestInputObject,
    nb: String,
}

#[derive(GraphQLInputObject, Debug)]
struct ExampleInputObject {
    a: Option<String>,
    b: i32,
}

#[derive(GraphQLInputObject, Debug)]
struct InputWithDefaults {
    #[graphql(default = "123")]
    a: i32,
}

struct TestType;

#[graphql_object]
impl TestType {
    fn field_with_object_input(input: Option<TestInputObject>) -> String {
        format!("{:?}", input)
    }

    fn field_with_nullable_string_input(input: Option<String>) -> String {
        format!("{:?}", input)
    }

    fn field_with_non_nullable_string_input(input: String) -> String {
        format!("{:?}", input)
    }

    #[graphql(
        arguments(
            input(
                default = "Hello World".to_string(),
            )
        )
    )]
    fn field_with_default_argument_value(input: String) -> String {
        format!("{:?}", input)
    }

    fn field_with_nested_object_input(input: Option<TestNestedInputObject>) -> String {
        format!("{:?}", input)
    }

    fn list(input: Option<Vec<Option<String>>>) -> String {
        format!("{:?}", input)
    }

    fn nn_list(input: Vec<Option<String>>) -> String {
        format!("{:?}", input)
    }

    fn list_nn(input: Option<Vec<String>>) -> String {
        format!("{:?}", input)
    }

    fn nn_list_nn(input: Vec<String>) -> String {
        format!("{:?}", input)
    }

    fn example_input(arg: ExampleInputObject) -> String {
        format!("a: {:?}, b: {:?}", arg.a, arg.b)
    }

    fn input_with_defaults(arg: InputWithDefaults) -> String {
        format!("a: {:?}", arg.a)
    }

    fn integer_input(value: i32) -> String {
        format!("value: {}", value)
    }

    fn float_input(value: f64) -> String {
        format!("value: {}", value)
    }
}

async fn run_variable_query<F>(query: &str, vars: Variables<DefaultScalarValue>, f: F)
where
    F: Fn(&Object<DefaultScalarValue>) -> (),
{
    let schema = RootNode::new(
        TestType,
        EmptyMutation::<()>::new(),
        EmptySubscription::<()>::new(),
    );

    let (result, errs) = crate::execute(query, None, &schema, &vars, &())
        .await
        .expect("Execution failed");

    assert_eq!(errs, []);

    println!("Result: {:?}", result);

    let obj = result.as_object_value().expect("Result is not an object");

    f(obj);
}

async fn run_query<F>(query: &str, f: F)
where
    F: Fn(&Object<DefaultScalarValue>) -> (),
{
    run_variable_query(query, Variables::new(), f).await;
}

#[tokio::test]
async fn inline_complex_input() {
    run_query(
        r#"{ fieldWithObjectInput(input: {a: "foo", b: ["bar"], c: "baz"}) }"#,
        |result: &Object<DefaultScalarValue>| {
            assert_eq!(
                result.get_field_value("fieldWithObjectInput"),
                Some(&Value::scalar(
                    r#"Some(TestInputObject { a: Some("foo"), b: Some([Some("bar")]), c: "baz", d: None })"#
                ))
            );
        },
    ).await;
}

#[tokio::test]
async fn inline_parse_single_value_to_list() {
    run_query(
        r#"{ fieldWithObjectInput(input: {a: "foo", b: "bar", c: "baz"}) }"#,
        |result: &Object<DefaultScalarValue>| {
            assert_eq!(
                result.get_field_value("fieldWithObjectInput"),
                Some(&Value::scalar(
                    r#"Some(TestInputObject { a: Some("foo"), b: Some([Some("bar")]), c: "baz", d: None })"#
                ))
            );
        },
    ).await;
}

#[tokio::test]
async fn inline_runs_from_input_value_on_scalar() {
    run_query(
        r#"{ fieldWithObjectInput(input: {c: "baz", d: "SerializedValue"}) }"#,
        |result: &Object<DefaultScalarValue>| {
            assert_eq!(
                result.get_field_value("fieldWithObjectInput"),
                Some(&Value::scalar(
                    r#"Some(TestInputObject { a: None, b: None, c: "baz", d: Some(TestComplexScalar) })"#
                ))
            );
        },
    ).await;
}

#[tokio::test]
async fn variable_complex_input() {
    run_variable_query(
        r#"query q($input: TestInputObject) { fieldWithObjectInput(input: $input) }"#,
        vec![(
            "input".to_owned(),
            InputValue::object(
                vec![
                    ("a", InputValue::scalar("foo")),
                    ("b", InputValue::list(vec![InputValue::scalar("bar")])),
                    ("c", InputValue::scalar("baz")),
                ]
                .into_iter()
                .collect(),
            ),
        )]
        .into_iter()
        .collect(),
        |result: &Object<DefaultScalarValue>| {
            assert_eq!(
                result.get_field_value("fieldWithObjectInput"),
                Some(&Value::scalar(
                    r#"Some(TestInputObject { a: Some("foo"), b: Some([Some("bar")]), c: "baz", d: None })"#
                ))
            );
        },
    ).await;
}

#[tokio::test]
async fn variable_parse_single_value_to_list() {
    run_variable_query(
        r#"query q($input: TestInputObject) { fieldWithObjectInput(input: $input) }"#,
        vec![(
            "input".to_owned(),
            InputValue::object(
                vec![
                    ("a", InputValue::scalar("foo")),
                    ("b", InputValue::scalar("bar")),
                    ("c", InputValue::scalar("baz")),
                ]
                .into_iter()
                .collect(),
            ),
        )]
        .into_iter()
        .collect(),
        |result: &Object<DefaultScalarValue>| {
            assert_eq!(
                result.get_field_value("fieldWithObjectInput"),
                Some(&Value::scalar(
                    r#"Some(TestInputObject { a: Some("foo"), b: Some([Some("bar")]), c: "baz", d: None })"#
                ))
            );
        },
    ).await;
}

#[tokio::test]
async fn variable_runs_from_input_value_on_scalar() {
    run_variable_query(
        r#"query q($input: TestInputObject) { fieldWithObjectInput(input: $input) }"#,
        vec![(
            "input".to_owned(),
            InputValue::object(
                vec![
                    ("c", InputValue::scalar("baz")),
                    ("d", InputValue::scalar("SerializedValue")),
                ]
                .into_iter()
                .collect(),
            ),
        )]
        .into_iter()
        .collect(),
        |result: &Object<DefaultScalarValue>| {
            assert_eq!(
                result.get_field_value("fieldWithObjectInput"),
                Some(&Value::scalar(
                    r#"Some(TestInputObject { a: None, b: None, c: "baz", d: Some(TestComplexScalar) })"#
                ))
            );
        },
    ).await;
}

#[tokio::test]
async fn variable_error_on_nested_non_null() {
    let schema = RootNode::new(
        TestType,
        EmptyMutation::<()>::new(),
        EmptySubscription::<()>::new(),
    );

    let query = r#"query q($input: TestInputObject) { fieldWithObjectInput(input: $input) }"#;
    let vars = vec![(
        "input".to_owned(),
        InputValue::object(
            vec![
                ("a", InputValue::scalar("foo")),
                ("b", InputValue::scalar("bar")),
                ("c", InputValue::null()),
            ]
            .into_iter()
            .collect(),
        ),
    )]
    .into_iter()
    .collect();

    let error = crate::execute(query, None, &schema, &vars, &())
        .await
        .unwrap_err();

    assert_eq!(
        error,
        ValidationError(vec![RuleError::new(
            r#"Variable "$input" got invalid value. In field "c": Expected "String!", found null."#,
            &[SourcePosition::new(8, 0, 8)],
        )])
    );
}

#[tokio::test]
async fn variable_error_on_incorrect_type() {
    let schema = RootNode::new(
        TestType,
        EmptyMutation::<()>::new(),
        EmptySubscription::<()>::new(),
    );

    let query = r#"query q($input: TestInputObject) { fieldWithObjectInput(input: $input) }"#;
    let vars = vec![("input".to_owned(), InputValue::scalar("foo bar"))]
        .into_iter()
        .collect();

    let error = crate::execute(query, None, &schema, &vars, &())
        .await
        .unwrap_err();

    assert_eq!(
        error,
        ValidationError(vec![RuleError::new(
            r#"Variable "$input" got invalid value. Expected "TestInputObject", found not an object."#,
            &[SourcePosition::new(8, 0, 8)],
        ),])
    );
}

#[tokio::test]
async fn variable_error_on_omit_non_null() {
    let schema = RootNode::new(
        TestType,
        EmptyMutation::<()>::new(),
        EmptySubscription::<()>::new(),
    );

    let query = r#"query q($input: TestInputObject) { fieldWithObjectInput(input: $input) }"#;
    let vars = vec![(
        "input".to_owned(),
        InputValue::object(
            vec![
                ("a", InputValue::scalar("foo")),
                ("b", InputValue::scalar("bar")),
            ]
            .into_iter()
            .collect(),
        ),
    )]
    .into_iter()
    .collect();

    let error = crate::execute(query, None, &schema, &vars, &())
        .await
        .unwrap_err();

    assert_eq!(
        error,
        ValidationError(vec![RuleError::new(
            r#"Variable "$input" got invalid value. In field "c": Expected "String!", found null."#,
            &[SourcePosition::new(8, 0, 8)],
        )])
    );
}

#[tokio::test]
async fn variable_multiple_errors_with_nesting() {
    let schema = RootNode::new(
        TestType,
        EmptyMutation::<()>::new(),
        EmptySubscription::<()>::new(),
    );

    let query =
        r#"query q($input: TestNestedInputObject) { fieldWithNestedObjectInput(input: $input) }"#;
    let vars = vec![(
        "input".to_owned(),
        InputValue::object(
            vec![(
                "na",
                InputValue::object(vec![("a", InputValue::scalar("foo"))].into_iter().collect()),
            )]
            .into_iter()
            .collect(),
        ),
    )]
    .into_iter()
    .collect();

    let error = crate::execute(query, None, &schema, &vars, &())
        .await
        .unwrap_err();

    assert_eq!(
        error,
        ValidationError(vec![
            RuleError::new(
                r#"Variable "$input" got invalid value. In field "na": In field "c": Expected "String!", found null."#,
                &[SourcePosition::new(8, 0, 8)],
            ),
            RuleError::new(
                r#"Variable "$input" got invalid value. In field "nb": Expected "String!", found null."#,
                &[SourcePosition::new(8, 0, 8)],
            ),
        ])
    );
}

#[tokio::test]
async fn variable_error_on_additional_field() {
    let schema = RootNode::new(
        TestType,
        EmptyMutation::<()>::new(),
        EmptySubscription::<()>::new(),
    );

    let query = r#"query q($input: TestInputObject) { fieldWithObjectInput(input: $input) }"#;
    let vars = vec![(
        "input".to_owned(),
        InputValue::object(
            vec![
                ("a", InputValue::scalar("foo")),
                ("b", InputValue::scalar("bar")),
                ("c", InputValue::scalar("baz")),
                ("extra", InputValue::scalar("dog")),
            ]
            .into_iter()
            .collect(),
        ),
    )]
    .into_iter()
    .collect();

    let error = crate::execute(query, None, &schema, &vars, &())
        .await
        .unwrap_err();

    assert_eq!(
        error,
        ValidationError(vec![RuleError::new(
            r#"Variable "$input" got invalid value. In field "extra": Unknown field."#,
            &[SourcePosition::new(8, 0, 8)],
        )])
    );
}

#[tokio::test]
async fn allow_nullable_inputs_to_be_omitted() {
    run_query(
        r#"{ fieldWithNullableStringInput }"#,
        |result: &Object<DefaultScalarValue>| {
            assert_eq!(
                result.get_field_value("fieldWithNullableStringInput"),
                Some(&Value::scalar(r#"None"#))
            );
        },
    )
    .await;
}

#[tokio::test]
async fn allow_nullable_inputs_to_be_omitted_in_variable() {
    run_query(
        r#"query q($value: String) { fieldWithNullableStringInput(input: $value) }"#,
        |result: &Object<DefaultScalarValue>| {
            assert_eq!(
                result.get_field_value("fieldWithNullableStringInput"),
                Some(&Value::scalar(r#"None"#))
            );
        },
    )
    .await;
}

#[tokio::test]
async fn allow_nullable_inputs_to_be_explicitly_null() {
    run_query(
        r#"{ fieldWithNullableStringInput(input: null) }"#,
        |result: &Object<DefaultScalarValue>| {
            assert_eq!(
                result.get_field_value("fieldWithNullableStringInput"),
                Some(&Value::scalar(r#"None"#))
            );
        },
    )
    .await;
}

#[tokio::test]
async fn allow_nullable_inputs_to_be_set_to_null_in_variable() {
    run_variable_query(
        r#"query q($value: String) { fieldWithNullableStringInput(input: $value) }"#,
        vec![("value".to_owned(), InputValue::null())]
            .into_iter()
            .collect(),
        |result: &Object<DefaultScalarValue>| {
            assert_eq!(
                result.get_field_value("fieldWithNullableStringInput"),
                Some(&Value::scalar(r#"None"#))
            );
        },
    )
    .await;
}

#[tokio::test]
async fn allow_nullable_inputs_to_be_set_to_value_in_variable() {
    run_variable_query(
        r#"query q($value: String) { fieldWithNullableStringInput(input: $value) }"#,
        vec![("value".to_owned(), InputValue::scalar("a"))]
            .into_iter()
            .collect(),
        |result: &Object<DefaultScalarValue>| {
            assert_eq!(
                result.get_field_value("fieldWithNullableStringInput"),
                Some(&Value::scalar(r#"Some("a")"#))
            );
        },
    )
    .await;
}

#[tokio::test]
async fn allow_nullable_inputs_to_be_set_to_value_directly() {
    run_query(
        r#"{ fieldWithNullableStringInput(input: "a") }"#,
        |result: &Object<DefaultScalarValue>| {
            assert_eq!(
                result.get_field_value("fieldWithNullableStringInput"),
                Some(&Value::scalar(r#"Some("a")"#))
            );
        },
    )
    .await;
}

#[tokio::test]
async fn does_not_allow_non_nullable_input_to_be_omitted_in_variable() {
    let schema = RootNode::new(
        TestType,
        EmptyMutation::<()>::new(),
        EmptySubscription::<()>::new(),
    );

    let query = r#"query q($value: String!) { fieldWithNonNullableStringInput(input: $value) }"#;
    let vars = vec![].into_iter().collect();

    let error = crate::execute(query, None, &schema, &vars, &())
        .await
        .unwrap_err();

    assert_eq!(
        error,
        ValidationError(vec![RuleError::new(
            r#"Variable "$value" of required type "String!" was not provided."#,
            &[SourcePosition::new(8, 0, 8)],
        )])
    );
}

#[tokio::test]
async fn does_not_allow_non_nullable_input_to_be_set_to_null_in_variable() {
    let schema = RootNode::new(
        TestType,
        EmptyMutation::<()>::new(),
        EmptySubscription::<()>::new(),
    );

    let query = r#"query q($value: String!) { fieldWithNonNullableStringInput(input: $value) }"#;
    let vars = vec![("value".to_owned(), InputValue::null())]
        .into_iter()
        .collect();

    let error = crate::execute(query, None, &schema, &vars, &())
        .await
        .unwrap_err();

    assert_eq!(
        error,
        ValidationError(vec![RuleError::new(
            r#"Variable "$value" of required type "String!" was not provided."#,
            &[SourcePosition::new(8, 0, 8)],
        )])
    );
}

#[tokio::test]
async fn allow_non_nullable_inputs_to_be_set_to_value_in_variable() {
    run_variable_query(
        r#"query q($value: String!) { fieldWithNonNullableStringInput(input: $value) }"#,
        vec![("value".to_owned(), InputValue::scalar("a"))]
            .into_iter()
            .collect(),
        |result| {
            assert_eq!(
                result.get_field_value("fieldWithNonNullableStringInput"),
                Some(&Value::scalar(r#""a""#))
            );
        },
    )
    .await;
}

#[tokio::test]
async fn allow_non_nullable_inputs_to_be_set_to_value_directly() {
    run_query(
        r#"{ fieldWithNonNullableStringInput(input: "a") }"#,
        |result: &Object<DefaultScalarValue>| {
            assert_eq!(
                result.get_field_value("fieldWithNonNullableStringInput"),
                Some(&Value::scalar(r#""a""#))
            );
        },
    )
    .await;
}

#[tokio::test]
async fn allow_lists_to_be_null() {
    run_variable_query(
        r#"query q($input: [String]) { list(input: $input) }"#,
        vec![("input".to_owned(), InputValue::null())]
            .into_iter()
            .collect(),
        |result: &Object<DefaultScalarValue>| {
            assert_eq!(
                result.get_field_value("list"),
                Some(&Value::scalar(r#"None"#))
            );
        },
    )
    .await;
}

#[tokio::test]
async fn allow_lists_to_contain_values() {
    run_variable_query(
        r#"query q($input: [String]) { list(input: $input) }"#,
        vec![(
            "input".to_owned(),
            InputValue::list(vec![InputValue::scalar("A")]),
        )]
        .into_iter()
        .collect(),
        |result| {
            assert_eq!(
                result.get_field_value("list"),
                Some(&Value::scalar(r#"Some([Some("A")])"#))
            );
        },
    )
    .await;
}

#[tokio::test]
async fn allow_lists_to_contain_null() {
    run_variable_query(
        r#"query q($input: [String]) { list(input: $input) }"#,
        vec![(
            "input".to_owned(),
            InputValue::list(vec![
                InputValue::scalar("A"),
                InputValue::null(),
                InputValue::scalar("B"),
            ]),
        )]
        .into_iter()
        .collect(),
        |result| {
            assert_eq!(
                result.get_field_value("list"),
                Some(&Value::scalar(r#"Some([Some("A"), None, Some("B")])"#))
            );
        },
    )
    .await;
}

#[tokio::test]
async fn does_not_allow_non_null_lists_to_be_null() {
    let schema = RootNode::new(
        TestType,
        EmptyMutation::<()>::new(),
        EmptySubscription::<()>::new(),
    );

    let query = r#"query q($input: [String]!) { nnList(input: $input) }"#;
    let vars = vec![("input".to_owned(), InputValue::null())]
        .into_iter()
        .collect();

    let error = crate::execute(query, None, &schema, &vars, &())
        .await
        .unwrap_err();

    assert_eq!(
        error,
        ValidationError(vec![RuleError::new(
            r#"Variable "$input" of required type "[String]!" was not provided."#,
            &[SourcePosition::new(8, 0, 8)],
        )])
    );
}

#[tokio::test]
async fn allow_non_null_lists_to_contain_values() {
    run_variable_query(
        r#"query q($input: [String]!) { nnList(input: $input) }"#,
        vec![(
            "input".to_owned(),
            InputValue::list(vec![InputValue::scalar("A")]),
        )]
        .into_iter()
        .collect(),
        |result| {
            assert_eq!(
                result.get_field_value("nnList"),
                Some(&Value::scalar(r#"[Some("A")]"#))
            );
        },
    )
    .await;
}
#[tokio::test]
async fn allow_non_null_lists_to_contain_null() {
    run_variable_query(
        r#"query q($input: [String]!) { nnList(input: $input) }"#,
        vec![(
            "input".to_owned(),
            InputValue::list(vec![
                InputValue::scalar("A"),
                InputValue::null(),
                InputValue::scalar("B"),
            ]),
        )]
        .into_iter()
        .collect(),
        |result| {
            assert_eq!(
                result.get_field_value("nnList"),
                Some(&Value::scalar(r#"[Some("A"), None, Some("B")]"#))
            );
        },
    )
    .await;
}

#[tokio::test]
async fn allow_lists_of_non_null_to_be_null() {
    run_variable_query(
        r#"query q($input: [String!]) { listNn(input: $input) }"#,
        vec![("input".to_owned(), InputValue::null())]
            .into_iter()
            .collect(),
        |result| {
            assert_eq!(
                result.get_field_value("listNn"),
                Some(&Value::scalar(r#"None"#))
            );
        },
    )
    .await;
}

#[tokio::test]
async fn allow_lists_of_non_null_to_contain_values() {
    run_variable_query(
        r#"query q($input: [String!]) { listNn(input: $input) }"#,
        vec![(
            "input".to_owned(),
            InputValue::list(vec![InputValue::scalar("A")]),
        )]
        .into_iter()
        .collect(),
        |result| {
            assert_eq!(
                result.get_field_value("listNn"),
                Some(&Value::scalar(r#"Some(["A"])"#))
            );
        },
    )
    .await;
}

#[tokio::test]
async fn does_not_allow_lists_of_non_null_to_contain_null() {
    let schema = RootNode::new(
        TestType,
        EmptyMutation::<()>::new(),
        EmptySubscription::<()>::new(),
    );

    let query = r#"query q($input: [String!]) { listNn(input: $input) }"#;
    let vars = vec![(
        "input".to_owned(),
        InputValue::list(vec![
            InputValue::scalar("A"),
            InputValue::null(),
            InputValue::scalar("B"),
        ]),
    )]
    .into_iter()
    .collect();

    let error = crate::execute(query, None, &schema, &vars, &())
        .await
        .unwrap_err();

    assert_eq!(
        error,
        ValidationError(vec![RuleError::new(
            r#"Variable "$input" got invalid value. In element #1: Expected "String!", found null."#,
            &[SourcePosition::new(8, 0, 8)],
        ),])
    );
}

#[tokio::test]
async fn does_not_allow_non_null_lists_of_non_null_to_contain_null() {
    let schema = RootNode::new(
        TestType,
        EmptyMutation::<()>::new(),
        EmptySubscription::<()>::new(),
    );

    let query = r#"query q($input: [String!]!) { nnListNn(input: $input) }"#;
    let vars = vec![(
        "input".to_owned(),
        InputValue::list(vec![
            InputValue::scalar("A"),
            InputValue::null(),
            InputValue::scalar("B"),
        ]),
    )]
    .into_iter()
    .collect();

    let error = crate::execute(query, None, &schema, &vars, &())
        .await
        .unwrap_err();

    assert_eq!(
        error,
        ValidationError(vec![RuleError::new(
            r#"Variable "$input" got invalid value. In element #1: Expected "String!", found null."#,
            &[SourcePosition::new(8, 0, 8)],
        ),])
    );
}

#[tokio::test]
async fn does_not_allow_non_null_lists_of_non_null_to_be_null() {
    let schema = RootNode::new(
        TestType,
        EmptyMutation::<()>::new(),
        EmptySubscription::<()>::new(),
    );

    let query = r#"query q($input: [String!]!) { nnListNn(input: $input) }"#;
    let vars = vec![("value".to_owned(), InputValue::null())]
        .into_iter()
        .collect();

    let error = crate::execute(query, None, &schema, &vars, &())
        .await
        .unwrap_err();

    assert_eq!(
        error,
        ValidationError(vec![RuleError::new(
            r#"Variable "$input" of required type "[String!]!" was not provided."#,
            &[SourcePosition::new(8, 0, 8)],
        )])
    );
}

#[tokio::test]
async fn allow_non_null_lists_of_non_null_to_contain_values() {
    run_variable_query(
        r#"query q($input: [String!]!) { nnListNn(input: $input) }"#,
        vec![(
            "input".to_owned(),
            InputValue::list(vec![InputValue::scalar("A")]),
        )]
        .into_iter()
        .collect(),
        |result| {
            assert_eq!(
                result.get_field_value("nnListNn"),
                Some(&Value::scalar(r#"["A"]"#))
            );
        },
    )
    .await;
}

#[tokio::test]
async fn default_argument_when_not_provided() {
    run_query(r#"{ fieldWithDefaultArgumentValue }"#, |result| {
        assert_eq!(
            result.get_field_value("fieldWithDefaultArgumentValue"),
            Some(&Value::scalar(r#""Hello World""#))
        );
    })
    .await;
}

#[tokio::test]
async fn default_argument_when_nullable_variable_not_provided() {
    run_query(
        r#"query q($input: String) { fieldWithDefaultArgumentValue(input: $input) }"#,
        |result| {
            assert_eq!(
                result.get_field_value("fieldWithDefaultArgumentValue"),
                Some(&Value::scalar(r#""Hello World""#))
            );
        },
    )
    .await;
}

#[tokio::test]
async fn default_argument_when_nullable_variable_set_to_null() {
    run_variable_query(
        r#"query q($input: String) { fieldWithDefaultArgumentValue(input: $input) }"#,
        vec![("input".to_owned(), InputValue::null())]
            .into_iter()
            .collect(),
        |result| {
            assert_eq!(
                result.get_field_value("fieldWithDefaultArgumentValue"),
                Some(&Value::scalar(r#""Hello World""#))
            );
        },
    )
    .await;
}

#[tokio::test]
async fn nullable_input_object_arguments_successful_without_variables() {
    run_query(r#"{ exampleInput(arg: {a: "abc", b: 123}) }"#, |result| {
        assert_eq!(
            result.get_field_value("exampleInput"),
            Some(&Value::scalar(r#"a: Some("abc"), b: 123"#))
        );
    })
    .await;

    run_query(r#"{ exampleInput(arg: {a: null, b: 1}) }"#, |result| {
        assert_eq!(
            result.get_field_value("exampleInput"),
            Some(&Value::scalar(r#"a: None, b: 1"#))
        );
    })
    .await;
}

#[tokio::test]
async fn nullable_input_object_arguments_successful_with_variables() {
    run_variable_query(
        r#"query q($var: Int!) { exampleInput(arg: {b: $var}) }"#,
        vec![("var".to_owned(), InputValue::scalar(123))]
            .into_iter()
            .collect(),
        |result| {
            assert_eq!(
                result.get_field_value("exampleInput"),
                Some(&Value::scalar(r#"a: None, b: 123"#))
            );
        },
    )
    .await;

    run_variable_query(
        r#"query q($var: String) { exampleInput(arg: {a: $var, b: 1}) }"#,
        vec![("var".to_owned(), InputValue::null())]
            .into_iter()
            .collect(),
        |result| {
            assert_eq!(
                result.get_field_value("exampleInput"),
                Some(&Value::scalar(r#"a: None, b: 1"#))
            );
        },
    )
    .await;

    run_variable_query(
        r#"query q($var: String) { exampleInput(arg: {a: $var, b: 1}) }"#,
        vec![].into_iter().collect(),
        |result| {
            assert_eq!(
                result.get_field_value("exampleInput"),
                Some(&Value::scalar(r#"a: None, b: 1"#))
            );
        },
    )
    .await;
}

#[tokio::test]
async fn does_not_allow_missing_required_field() {
    let schema = RootNode::new(
        TestType,
        EmptyMutation::<()>::new(),
        EmptySubscription::<()>::new(),
    );

    let query = r#"{ exampleInput(arg: {a: "abc"}) }"#;
    let vars = vec![].into_iter().collect();

    let error = crate::execute(query, None, &schema, &vars, &())
        .await
        .unwrap_err();

    assert_eq!(
        error,
        ValidationError(vec![RuleError::new(
            r#"Error for argument "arg": "ExampleInputObject" is missing fields: "b""#,
            &[SourcePosition::new(20, 0, 20)],
        )])
    );
}

#[tokio::test]
async fn does_not_allow_null_in_required_field() {
    let schema = RootNode::new(
        TestType,
        EmptyMutation::<()>::new(),
        EmptySubscription::<()>::new(),
    );

    let query = r#"{ exampleInput(arg: {a: "abc", b: null}) }"#;
    let vars = vec![].into_iter().collect();

    let error = crate::execute(query, None, &schema, &vars, &())
        .await
        .unwrap_err();

    assert_eq!(
        error,
        ValidationError(vec![RuleError::new(
            r#"Error for argument "arg": Error on "ExampleInputObject" field "b": Type "Int!" is not nullable"#,
            &[SourcePosition::new(20, 0, 20)],
        )])
    );
}

#[tokio::test]
async fn does_not_allow_missing_variable_for_required_field() {
    let schema = RootNode::new(
        TestType,
        EmptyMutation::<()>::new(),
        EmptySubscription::<()>::new(),
    );

    let query = r#"query q($var: Int!) { exampleInput(arg: {b: $var}) }"#;
    let vars = vec![].into_iter().collect();

    let error = crate::execute(query, None, &schema, &vars, &())
        .await
        .unwrap_err();

    assert_eq!(
        error,
        ValidationError(vec![RuleError::new(
            r#"Variable "$var" of required type "Int!" was not provided."#,
            &[SourcePosition::new(8, 0, 8)],
        )])
    );
}

#[tokio::test]
async fn does_not_allow_null_variable_for_required_field() {
    let schema = RootNode::new(
        TestType,
        EmptyMutation::<()>::new(),
        EmptySubscription::<()>::new(),
    );

    let query = r#"query q($var: Int!) { exampleInput(arg: {b: $var}) }"#;
    let vars = vec![("var".to_owned(), InputValue::null())]
        .into_iter()
        .collect();

    let error = crate::execute(query, None, &schema, &vars, &())
        .await
        .unwrap_err();

    assert_eq!(
        error,
        ValidationError(vec![RuleError::new(
            r#"Variable "$var" of required type "Int!" was not provided."#,
            &[SourcePosition::new(8, 0, 8)],
        )])
    );
}

#[tokio::test]
async fn input_object_with_default_values() {
    run_query(r#"{ inputWithDefaults(arg: {a: 1}) }"#, |result| {
        assert_eq!(
            result.get_field_value("inputWithDefaults"),
            Some(&Value::scalar(r#"a: 1"#))
        );
    })
    .await;

    run_variable_query(
        r#"query q($var: Int!) { inputWithDefaults(arg: {a: $var}) }"#,
        vec![("var".to_owned(), InputValue::scalar(1))]
            .into_iter()
            .collect(),
        |result| {
            assert_eq!(
                result.get_field_value("inputWithDefaults"),
                Some(&Value::scalar(r#"a: 1"#))
            );
        },
    )
    .await;

    run_variable_query(
        r#"query q($var: Int = 1) { inputWithDefaults(arg: {a: $var}) }"#,
        vec![].into_iter().collect(),
        |result| {
            assert_eq!(
                result.get_field_value("inputWithDefaults"),
                Some(&Value::scalar(r#"a: 1"#))
            );
        },
    )
    .await;

    run_variable_query(
        r#"query q($var: Int = 1) { inputWithDefaults(arg: {a: $var}) }"#,
        vec![("var".to_owned(), InputValue::scalar(2))]
            .into_iter()
            .collect(),
        |result| {
            assert_eq!(
                result.get_field_value("inputWithDefaults"),
                Some(&Value::scalar(r#"a: 2"#))
            );
        },
    )
    .await;
}

mod integers {
    use super::*;

    #[tokio::test]
    async fn positive_and_negative_should_work() {
        run_variable_query(
            r#"query q($var: Int!) { integerInput(value: $var) }"#,
            vec![("var".to_owned(), InputValue::scalar(1))]
                .into_iter()
                .collect(),
            |result| {
                assert_eq!(
                    result.get_field_value("integerInput"),
                    Some(&Value::scalar(r#"value: 1"#))
                );
            },
        )
        .await;

        run_variable_query(
            r#"query q($var: Int!) { integerInput(value: $var) }"#,
            vec![("var".to_owned(), InputValue::scalar(-1))]
                .into_iter()
                .collect(),
            |result| {
                assert_eq!(
                    result.get_field_value("integerInput"),
                    Some(&Value::scalar(r#"value: -1"#))
                );
            },
        )
        .await;
    }

    #[tokio::test]
    async fn does_not_coerce_from_float() {
        let schema = RootNode::new(
            TestType,
            EmptyMutation::<()>::new(),
            EmptySubscription::<()>::new(),
        );

        let query = r#"query q($var: Int!) { integerInput(value: $var) }"#;
        let vars = vec![("var".to_owned(), InputValue::scalar(10.0))]
            .into_iter()
            .collect();

        let error = crate::execute(query, None, &schema, &vars, &())
            .await
            .unwrap_err();

        assert_eq!(
            error,
            ValidationError(vec![RuleError::new(
                r#"Variable "$var" got invalid value. Expected "Int"."#,
                &[SourcePosition::new(8, 0, 8)],
            )])
        );
    }

    #[tokio::test]
    async fn does_not_coerce_from_string() {
        let schema = RootNode::new(
            TestType,
            EmptyMutation::<()>::new(),
            EmptySubscription::<()>::new(),
        );

        let query = r#"query q($var: Int!) { integerInput(value: $var) }"#;
        let vars = vec![("var".to_owned(), InputValue::scalar("10"))]
            .into_iter()
            .collect();

        let error = crate::execute(query, None, &schema, &vars, &())
            .await
            .unwrap_err();

        assert_eq!(
            error,
            ValidationError(vec![RuleError::new(
                r#"Variable "$var" got invalid value. Expected "Int"."#,
                &[SourcePosition::new(8, 0, 8)],
            )])
        );
    }
}

mod floats {
    use super::*;

    #[tokio::test]
    async fn float_values_should_work() {
        run_variable_query(
            r#"query q($var: Float!) { floatInput(value: $var) }"#,
            vec![("var".to_owned(), InputValue::scalar(10.0))]
                .into_iter()
                .collect(),
            |result| {
                assert_eq!(
                    result.get_field_value("floatInput"),
                    Some(&Value::scalar(r#"value: 10"#))
                );
            },
        )
        .await;
    }

    #[tokio::test]
    async fn coercion_from_integers_should_work() {
        run_variable_query(
            r#"query q($var: Float!) { floatInput(value: $var) }"#,
            vec![("var".to_owned(), InputValue::scalar(-1))]
                .into_iter()
                .collect(),
            |result| {
                assert_eq!(
                    result.get_field_value("floatInput"),
                    Some(&Value::scalar(r#"value: -1"#))
                );
            },
        )
        .await;
    }

    #[tokio::test]
    async fn does_not_coerce_from_string() {
        let schema = RootNode::new(
            TestType,
            EmptyMutation::<()>::new(),
            EmptySubscription::<()>::new(),
        );

        let query = r#"query q($var: Float!) { floatInput(value: $var) }"#;
        let vars = vec![("var".to_owned(), InputValue::scalar("10"))]
            .into_iter()
            .collect();

        let error = crate::execute(query, None, &schema, &vars, &())
            .await
            .unwrap_err();

        assert_eq!(
            error,
            ValidationError(vec![RuleError::new(
                r#"Variable "$var" got invalid value. Expected "Float"."#,
                &[SourcePosition::new(8, 0, 8)],
            )])
        );
    }
}
