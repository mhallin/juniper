mod enums;
mod input_object;

use juniper_codegen::GraphQLEnumInternal as GraphQLEnum;

// This asserts that the input objects defined public actually became public
#[allow(unused_imports)]
use self::input_object::{NamedPublic, NamedPublicWithDescription};

use crate::{
    executor::Variables,
    schema::model::RootNode,
    types::scalars::EmptyMutation,
    value::{ParseScalarResult, ParseScalarValue, Value},
};

#[derive(GraphQLEnum)]
#[graphql(name = "SampleEnum")]
enum Sample {
    One,
    Two,
}

struct Scalar(i32);

struct Interface;

struct Root;

graphql_scalar!(Scalar as "SampleScalar" {
    resolve(&self) -> Value {
        Value::scalar(self.0)
    }

    from_input_value(v: &InputValue) -> Option<Scalar> {
        v.as_scalar_value().map(|i: &i32| Scalar(*i))
    }

    from_str<'a>(value: ScalarToken<'a>) -> ParseScalarResult<'a> {
        <i32 as ParseScalarValue>::from_str(value)
    }
});

graphql_interface!(Interface: () as "SampleInterface" |&self| {
    description: "A sample interface"

    field sample_enum() -> Sample as "A sample field in the interface" {
        Sample::One
    }

    instance_resolvers: |&_| {
        Root => Some(Root),
    }
});

/// The root query object in the schema
#[crate::object_internal(
    interfaces = [&Interface]
    Scalar = crate::DefaultScalarValue,
)]
impl Root {
    fn sample_enum() -> Sample {
        Sample::One
    }

    /// A sample scalar field on the object
    fn sample_scalar(
        #[graphql(description = "The first number")] first: i32,
        #[graphql(description = "The second number", default = 123)] second: i32,
    ) -> Scalar {
        Scalar(first + second)
    }
}

#[test]
fn test_execution() {
    let doc = r#"
    {
        sampleEnum
        first: sampleScalar(first: 0)
        second: sampleScalar(first: 10 second: 20)
    }
    "#;
    let schema = RootNode::new(Root, EmptyMutation::<()>::new());

    let (result, errs) =
        crate::execute(doc, None, &schema, &Variables::new(), &()).expect("Execution failed");

    assert_eq!(errs, []);

    println!("Result: {:#?}", result);

    assert_eq!(
        result,
        Value::object(
            vec![
                ("sampleEnum", Value::scalar("ONE")),
                ("first", Value::scalar(123)),
                ("second", Value::scalar(30)),
            ]
            .into_iter()
            .collect()
        )
    );
}

#[test]
fn enum_introspection() {
    let doc = r#"
    {
        __type(name: "SampleEnum") {
            name
            kind
            description
            enumValues {
                name
                description
                isDeprecated
                deprecationReason
            }
            interfaces { name }
            possibleTypes { name }
            inputFields { name }
            ofType { name }
        }
    }
    "#;
    let schema = RootNode::new(Root, EmptyMutation::<()>::new());

    let (result, errs) =
        crate::execute(doc, None, &schema, &Variables::new(), &()).expect("Execution failed");

    assert_eq!(errs, []);

    println!("Result: {:#?}", result);

    let type_info = result
        .as_object_value()
        .expect("Result is not an object")
        .get_field_value("__type")
        .expect("__type field missing")
        .as_object_value()
        .expect("__type field not an object value");

    assert_eq!(
        type_info.get_field_value("name"),
        Some(&Value::scalar("SampleEnum"))
    );
    assert_eq!(
        type_info.get_field_value("kind"),
        Some(&Value::scalar("ENUM"))
    );
    assert_eq!(
        type_info.get_field_value("description"),
        Some(&Value::null())
    );
    assert_eq!(
        type_info.get_field_value("interfaces"),
        Some(&Value::null())
    );
    assert_eq!(
        type_info.get_field_value("possibleTypes"),
        Some(&Value::null())
    );
    assert_eq!(
        type_info.get_field_value("inputFields"),
        Some(&Value::null())
    );
    assert_eq!(type_info.get_field_value("ofType"), Some(&Value::null()));

    let values = type_info
        .get_field_value("enumValues")
        .expect("enumValues field missing")
        .as_list_value()
        .expect("enumValues not a list");

    assert_eq!(values.len(), 2);

    assert!(values.contains(&Value::object(
        vec![
            ("name", Value::scalar("ONE")),
            ("description", Value::null()),
            ("isDeprecated", Value::scalar(false)),
            ("deprecationReason", Value::null()),
        ]
        .into_iter()
        .collect(),
    )));

    assert!(values.contains(&Value::object(
        vec![
            ("name", Value::scalar("TWO")),
            ("description", Value::null()),
            ("isDeprecated", Value::scalar(false)),
            ("deprecationReason", Value::null()),
        ]
        .into_iter()
        .collect(),
    )));
}

#[test]
fn interface_introspection() {
    let doc = r#"
    {
        __type(name: "SampleInterface") {
            name
            kind
            description
            possibleTypes {
                name
            }
            fields {
                name
                description
                args {
                    name
                }
                type {
                    name
                    kind
                    ofType {
                        name
                        kind
                    }
                }
                isDeprecated
                deprecationReason
            }
            interfaces { name }
            enumValues { name }
            inputFields { name }
            ofType { name }
        }
    }
    "#;
    let schema = RootNode::new(Root, EmptyMutation::<()>::new());

    let (result, errs) =
        crate::execute(doc, None, &schema, &Variables::new(), &()).expect("Execution failed");

    assert_eq!(errs, []);

    println!("Result: {:#?}", result);

    let type_info = result
        .as_object_value()
        .expect("Result is not an object")
        .get_field_value("__type")
        .expect("__type field missing")
        .as_object_value()
        .expect("__type field not an object value");

    assert_eq!(
        type_info.get_field_value("name"),
        Some(&Value::scalar("SampleInterface"))
    );
    assert_eq!(
        type_info.get_field_value("kind"),
        Some(&Value::scalar("INTERFACE"))
    );
    assert_eq!(
        type_info.get_field_value("description"),
        Some(&Value::scalar("A sample interface"))
    );
    assert_eq!(
        type_info.get_field_value("interfaces"),
        Some(&Value::null())
    );
    assert_eq!(
        type_info.get_field_value("enumValues"),
        Some(&Value::null())
    );
    assert_eq!(
        type_info.get_field_value("inputFields"),
        Some(&Value::null())
    );
    assert_eq!(type_info.get_field_value("ofType"), Some(&Value::null()));

    let possible_types = type_info
        .get_field_value("possibleTypes")
        .expect("possibleTypes field missing")
        .as_list_value()
        .expect("possibleTypes not a list");

    assert_eq!(possible_types.len(), 1);

    assert!(possible_types.contains(&Value::object(
        vec![("name", Value::scalar("Root"))].into_iter().collect()
    )));

    let fields = type_info
        .get_field_value("fields")
        .expect("fields field missing")
        .as_list_value()
        .expect("fields field not an object value");

    assert_eq!(fields.len(), 1);

    assert!(fields.contains(&Value::object(
        vec![
            ("name", Value::scalar("sampleEnum")),
            (
                "description",
                Value::scalar("A sample field in the interface"),
            ),
            ("args", Value::list(vec![])),
            (
                "type",
                Value::object(
                    vec![
                        ("name", Value::null()),
                        ("kind", Value::scalar("NON_NULL")),
                        (
                            "ofType",
                            Value::object(
                                vec![
                                    ("name", Value::scalar("SampleEnum")),
                                    ("kind", Value::scalar("ENUM")),
                                ]
                                .into_iter()
                                .collect(),
                            ),
                        ),
                    ]
                    .into_iter()
                    .collect(),
                ),
            ),
            ("isDeprecated", Value::scalar(false)),
            ("deprecationReason", Value::null()),
        ]
        .into_iter()
        .collect(),
    )));
}

#[test]
fn object_introspection() {
    let doc = r#"
    {
        __type(name: "Root") {
            name
            kind
            description
            fields {
                name
                description
                args {
                    name
                    description
                    type {
                        name
                        kind
                        ofType {
                            name
                            kind
                            ofType {
                                name
                            }
                        }
                    }
                    defaultValue
                }
                type {
                    name
                    kind
                    ofType {
                        name
                        kind
                    }
                }
                isDeprecated
                deprecationReason
            }
            possibleTypes { name }
            interfaces { name }
            enumValues { name }
            inputFields { name }
            ofType { name }
        }
    }
    "#;
    let schema = RootNode::new(Root, EmptyMutation::<()>::new());

    let (result, errs) =
        crate::execute(doc, None, &schema, &Variables::new(), &()).expect("Execution failed");

    assert_eq!(errs, []);

    println!("Result: {:#?}", result);

    let type_info = result
        .as_object_value()
        .expect("Result is not an object")
        .get_field_value("__type")
        .expect("__type field missing")
        .as_object_value()
        .expect("__type field not an object value");

    assert_eq!(
        type_info.get_field_value("name"),
        Some(&Value::scalar("Root"))
    );
    assert_eq!(
        type_info.get_field_value("kind"),
        Some(&Value::scalar("OBJECT"))
    );
    assert_eq!(
        type_info.get_field_value("description"),
        Some(&Value::scalar("The root query object in the schema"))
    );
    assert_eq!(
        type_info.get_field_value("interfaces"),
        Some(&Value::list(vec![Value::object(
            vec![("name", Value::scalar("SampleInterface"))]
                .into_iter()
                .collect(),
        )]))
    );
    assert_eq!(
        type_info.get_field_value("enumValues"),
        Some(&Value::null())
    );
    assert_eq!(
        type_info.get_field_value("inputFields"),
        Some(&Value::null())
    );
    assert_eq!(type_info.get_field_value("ofType"), Some(&Value::null()));
    assert_eq!(
        type_info.get_field_value("possibleTypes"),
        Some(&Value::null())
    );

    let fields = type_info
        .get_field_value("fields")
        .expect("fields field missing")
        .as_list_value()
        .expect("fields field not an object value");

    assert_eq!(fields.len(), 2);

    println!("Fields: {:#?}", fields);

    assert!(fields.contains(&Value::object(
        vec![
            ("name", Value::scalar("sampleEnum")),
            ("description", Value::null()),
            ("args", Value::list(vec![])),
            (
                "type",
                Value::object(
                    vec![
                        ("name", Value::null()),
                        ("kind", Value::scalar("NON_NULL")),
                        (
                            "ofType",
                            Value::object(
                                vec![
                                    ("name", Value::scalar("SampleEnum")),
                                    ("kind", Value::scalar("ENUM")),
                                ]
                                .into_iter()
                                .collect(),
                            ),
                        ),
                    ]
                    .into_iter()
                    .collect(),
                ),
            ),
            ("isDeprecated", Value::scalar(false)),
            ("deprecationReason", Value::null()),
        ]
        .into_iter()
        .collect(),
    )));

    assert!(fields.contains(&Value::object(
        vec![
            ("name", Value::scalar("sampleScalar")),
            (
                "description",
                Value::scalar("A sample scalar field on the object"),
            ),
            (
                "args",
                Value::list(vec![
                    Value::object(
                        vec![
                            ("name", Value::scalar("first")),
                            ("description", Value::scalar("The first number")),
                            (
                                "type",
                                Value::object(
                                    vec![
                                        ("name", Value::null()),
                                        ("kind", Value::scalar("NON_NULL")),
                                        (
                                            "ofType",
                                            Value::object(
                                                vec![
                                                    ("name", Value::scalar("Int")),
                                                    ("kind", Value::scalar("SCALAR")),
                                                    ("ofType", Value::null()),
                                                ]
                                                .into_iter()
                                                .collect(),
                                            ),
                                        ),
                                    ]
                                    .into_iter()
                                    .collect(),
                                ),
                            ),
                            ("defaultValue", Value::null()),
                        ]
                        .into_iter()
                        .collect(),
                    ),
                    Value::object(
                        vec![
                            ("name", Value::scalar("second")),
                            ("description", Value::scalar("The second number")),
                            (
                                "type",
                                Value::object(
                                    vec![
                                        ("name", Value::scalar("Int")),
                                        ("kind", Value::scalar("SCALAR")),
                                        ("ofType", Value::null()),
                                    ]
                                    .into_iter()
                                    .collect(),
                                ),
                            ),
                            ("defaultValue", Value::scalar("123")),
                        ]
                        .into_iter()
                        .collect(),
                    ),
                ]),
            ),
            (
                "type",
                Value::object(
                    vec![
                        ("name", Value::null()),
                        ("kind", Value::scalar("NON_NULL")),
                        (
                            "ofType",
                            Value::object(
                                vec![
                                    ("name", Value::scalar("SampleScalar")),
                                    ("kind", Value::scalar("SCALAR")),
                                ]
                                .into_iter()
                                .collect(),
                            ),
                        ),
                    ]
                    .into_iter()
                    .collect(),
                ),
            ),
            ("isDeprecated", Value::scalar(false)),
            ("deprecationReason", Value::null()),
        ]
        .into_iter()
        .collect(),
    )));
}

#[test]
fn scalar_introspection() {
    let doc = r#"
    {
        __type(name: "SampleScalar") {
            name
            kind
            description
            fields { name }
            interfaces { name }
            possibleTypes { name }
            enumValues { name }
            inputFields { name }
            ofType { name }
        }
    }
    "#;
    let schema = RootNode::new(Root, EmptyMutation::<()>::new());

    let (result, errs) =
        crate::execute(doc, None, &schema, &Variables::new(), &()).expect("Execution failed");

    assert_eq!(errs, []);

    println!("Result: {:#?}", result);

    let type_info = result
        .as_object_value()
        .expect("Result is not an object")
        .get_field_value("__type")
        .expect("__type field missing");

    assert_eq!(
        type_info,
        &Value::object(
            vec![
                ("name", Value::scalar("SampleScalar")),
                ("kind", Value::scalar("SCALAR")),
                ("description", Value::null()),
                ("fields", Value::null()),
                ("interfaces", Value::null()),
                ("possibleTypes", Value::null()),
                ("enumValues", Value::null()),
                ("inputFields", Value::null()),
                ("ofType", Value::null()),
            ]
            .into_iter()
            .collect()
        )
    );
}
