use juniper_codegen::GraphQLInputObjectInternal as GraphQLInputObject;

use crate::{
    executor::Variables,
    schema::model::RootNode,
    types::scalars::EmptyMutation,
    value::{DefaultScalarValue, Value},
};

struct Root;

/*

Syntax to validate:

* No args at all
* Executor arg vs. no executor arg
* Single arg vs. multi arg
* Trailing comma vs. no trailing comma
* Default value vs. no default value
* Complex default value
* Description vs. no description

*/

#[derive(GraphQLInputObject)]
struct Point {
    x: i32,
}

#[crate::object_internal]
impl Root {
    fn simple() -> i32 {
        0
    }
    fn exec_arg(executor: &Executor) -> i32 {
        0
    }
    fn exec_arg_and_more(executor: &Executor, arg: i32) -> i32 {
        0
    }

    fn single_arg(arg: i32) -> i32 {
        0
    }

    fn multi_args(arg1: i32, arg2: i32) -> i32 {
        0
    }

    fn multi_args_trailing_comma(arg1: i32, arg2: i32) -> i32 {
        0
    }

    fn single_arg_descr(#[graphql(description = "The arg")] arg: i32) -> i32 {
        0
    }

    fn multi_args_descr(
        #[graphql(description = "The first arg")] arg1: i32,
        #[graphql(description = "The second arg")] arg2: i32,
    ) -> i32 {
        0
    }

    fn multi_args_descr_trailing_comma(
        #[graphql(description = "The first arg")] arg1: i32,
        #[graphql(description = "The second arg")] arg2: i32,
    ) -> i32 {
        0
    }

    fn arg_with_default(#[graphql(default = 123)] arg: i32) -> i32 {
        0
    }

    fn multi_args_with_default(
        #[graphql(default = 123)] arg1: i32,
        #[graphql(default = 456)] arg2: i32,
    ) -> i32 {
        0
    }

    fn multi_args_with_default_trailing_comma(
        #[graphql(default = 123)] arg1: i32,
        #[graphql(default = 456)] arg2: i32,
    ) -> i32 {
        0
    }

    fn arg_with_default_descr(#[graphql(default = 123, description = "The arg")] arg: i32) -> i32 {
        0
    }

    fn multi_args_with_default_descr(
        #[graphql(default = 123, description = "The first arg")] arg1: i32,
        #[graphql(default = 456, description = "The second arg")] arg2: i32,
    ) -> i32 {
        0
    }

    fn multi_args_with_default_trailing_comma_descr(
        #[graphql(default = 123, description = "The first arg")] arg1: i32,
        #[graphql(default = 456, description = "The second arg")] arg2: i32,
    ) -> i32 {
        0
    }

    fn args_with_complex_default(
        #[graphql(
            default = "test".to_string(),
            description = "A string default argument",
        )]
        arg1: String,

        #[graphql(
            default = Point{ x: 1 },
            description = "An input object default argument",
        )]
        arg2: Point,
    ) -> i32 {
        0
    }
}

fn run_args_info_query<F>(field_name: &str, f: F)
where
    F: Fn(&Vec<Value<DefaultScalarValue>>) -> (),
{
    let doc = r#"
    {
        __type(name: "Root") {
            fields {
                name
                args {
                    name
                    description
                    defaultValue
                    type {
                        name
                        ofType {
                            name
                        }
                    }
                }
            }
        }
    }
    "#;
    let schema = RootNode::new(Root {}, EmptyMutation::<()>::new());

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

    let fields = type_info
        .get_field_value("fields")
        .expect("fields field missing")
        .as_list_value()
        .expect("fields not a list");

    let field = fields
        .into_iter()
        .filter(|f| {
            f.as_object_value()
                .expect("Field not an object")
                .get_field_value("name")
                .expect("name field missing from field")
                .as_scalar_value::<String>()
                .expect("name is not a string")
                == field_name
        })
        .next()
        .expect("Field not found")
        .as_object_value()
        .expect("Field is not an object");

    println!("Field: {:#?}", field);

    let args = field
        .get_field_value("args")
        .expect("args missing from field")
        .as_list_value()
        .expect("args is not a list");

    println!("Args: {:#?}", args);

    f(args);
}

#[test]
fn introspect_field_simple() {
    run_args_info_query("simple", |args| {
        assert_eq!(args.len(), 0);
    });
}

#[test]
fn introspect_field_exec_arg() {
    run_args_info_query("execArg", |args| {
        assert_eq!(args.len(), 0);
    });
}

#[test]
fn introspect_field_exec_arg_and_more() {
    run_args_info_query("execArgAndMore", |args| {
        assert_eq!(args.len(), 1);

        assert!(args.contains(&Value::object(
            vec![
                ("name", Value::scalar("arg")),
                ("description", Value::null()),
                ("defaultValue", Value::null()),
                (
                    "type",
                    Value::object(
                        vec![
                            ("name", Value::null()),
                            (
                                "ofType",
                                Value::object(
                                    vec![("name", Value::scalar("Int"))].into_iter().collect(),
                                ),
                            ),
                        ]
                        .into_iter()
                        .collect(),
                    ),
                ),
            ]
            .into_iter()
            .collect(),
        )));
    });
}

#[test]
fn introspect_field_single_arg() {
    run_args_info_query("singleArg", |args| {
        assert_eq!(args.len(), 1);

        assert!(args.contains(&Value::object(
            vec![
                ("name", Value::scalar("arg")),
                ("description", Value::null()),
                ("defaultValue", Value::null()),
                (
                    "type",
                    Value::object(
                        vec![
                            ("name", Value::null()),
                            (
                                "ofType",
                                Value::object(
                                    vec![("name", Value::scalar("Int"))].into_iter().collect(),
                                ),
                            ),
                        ]
                        .into_iter()
                        .collect(),
                    ),
                ),
            ]
            .into_iter()
            .collect(),
        )));
    });
}

#[test]
fn introspect_field_multi_args() {
    run_args_info_query("multiArgs", |args| {
        assert_eq!(args.len(), 2);

        assert!(args.contains(&Value::object(
            vec![
                ("name", Value::scalar("arg1")),
                ("description", Value::null()),
                ("defaultValue", Value::null()),
                (
                    "type",
                    Value::object(
                        vec![
                            ("name", Value::null()),
                            (
                                "ofType",
                                Value::object(
                                    vec![("name", Value::scalar("Int"))].into_iter().collect(),
                                ),
                            ),
                        ]
                        .into_iter()
                        .collect(),
                    ),
                ),
            ]
            .into_iter()
            .collect(),
        )));

        assert!(args.contains(&Value::object(
            vec![
                ("name", Value::scalar("arg2")),
                ("description", Value::null()),
                ("defaultValue", Value::null()),
                (
                    "type",
                    Value::object(
                        vec![
                            ("name", Value::null()),
                            (
                                "ofType",
                                Value::object(
                                    vec![("name", Value::scalar("Int"))].into_iter().collect(),
                                ),
                            ),
                        ]
                        .into_iter()
                        .collect(),
                    ),
                ),
            ]
            .into_iter()
            .collect(),
        )));
    });
}

#[test]
fn introspect_field_multi_args_trailing_comma() {
    run_args_info_query("multiArgsTrailingComma", |args| {
        assert_eq!(args.len(), 2);

        assert!(args.contains(&Value::object(
            vec![
                ("name", Value::scalar("arg1")),
                ("description", Value::null()),
                ("defaultValue", Value::null()),
                (
                    "type",
                    Value::object(
                        vec![
                            ("name", Value::null()),
                            (
                                "ofType",
                                Value::object(
                                    vec![("name", Value::scalar("Int"))].into_iter().collect(),
                                ),
                            ),
                        ]
                        .into_iter()
                        .collect(),
                    ),
                ),
            ]
            .into_iter()
            .collect(),
        )));

        assert!(args.contains(&Value::object(
            vec![
                ("name", Value::scalar("arg2")),
                ("description", Value::null()),
                ("defaultValue", Value::null()),
                (
                    "type",
                    Value::object(
                        vec![
                            ("name", Value::null()),
                            (
                                "ofType",
                                Value::object(
                                    vec![("name", Value::scalar("Int"))].into_iter().collect(),
                                ),
                            ),
                        ]
                        .into_iter()
                        .collect(),
                    ),
                ),
            ]
            .into_iter()
            .collect(),
        )));
    });
}

#[test]
fn introspect_field_single_arg_descr() {
    run_args_info_query("singleArgDescr", |args| {
        assert_eq!(args.len(), 1);

        assert!(args.contains(&Value::object(
            vec![
                ("name", Value::scalar("arg")),
                ("description", Value::scalar("The arg")),
                ("defaultValue", Value::null()),
                (
                    "type",
                    Value::object(
                        vec![
                            ("name", Value::null()),
                            (
                                "ofType",
                                Value::object(
                                    vec![("name", Value::scalar("Int"))].into_iter().collect(),
                                ),
                            ),
                        ]
                        .into_iter()
                        .collect(),
                    ),
                ),
            ]
            .into_iter()
            .collect(),
        )));
    });
}

#[test]
fn introspect_field_multi_args_descr() {
    run_args_info_query("multiArgsDescr", |args| {
        assert_eq!(args.len(), 2);

        assert!(args.contains(&Value::object(
            vec![
                ("name", Value::scalar("arg1")),
                ("description", Value::scalar("The first arg")),
                ("defaultValue", Value::null()),
                (
                    "type",
                    Value::object(
                        vec![
                            ("name", Value::null()),
                            (
                                "ofType",
                                Value::object(
                                    vec![("name", Value::scalar("Int"))].into_iter().collect(),
                                ),
                            ),
                        ]
                        .into_iter()
                        .collect(),
                    ),
                ),
            ]
            .into_iter()
            .collect(),
        )));

        assert!(args.contains(&Value::object(
            vec![
                ("name", Value::scalar("arg2")),
                ("description", Value::scalar("The second arg")),
                ("defaultValue", Value::null()),
                (
                    "type",
                    Value::object(
                        vec![
                            ("name", Value::null()),
                            (
                                "ofType",
                                Value::object(
                                    vec![("name", Value::scalar("Int"))].into_iter().collect(),
                                ),
                            ),
                        ]
                        .into_iter()
                        .collect(),
                    ),
                ),
            ]
            .into_iter()
            .collect(),
        )));
    });
}

#[test]
fn introspect_field_multi_args_descr_trailing_comma() {
    run_args_info_query("multiArgsDescrTrailingComma", |args| {
        assert_eq!(args.len(), 2);

        assert!(args.contains(&Value::object(
            vec![
                ("name", Value::scalar("arg1")),
                ("description", Value::scalar("The first arg")),
                ("defaultValue", Value::null()),
                (
                    "type",
                    Value::object(
                        vec![
                            ("name", Value::null()),
                            (
                                "ofType",
                                Value::object(
                                    vec![("name", Value::scalar("Int"))].into_iter().collect(),
                                ),
                            ),
                        ]
                        .into_iter()
                        .collect(),
                    ),
                ),
            ]
            .into_iter()
            .collect(),
        )));

        assert!(args.contains(&Value::object(
            vec![
                ("name", Value::scalar("arg2")),
                ("description", Value::scalar("The second arg")),
                ("defaultValue", Value::null()),
                (
                    "type",
                    Value::object(
                        vec![
                            ("name", Value::null()),
                            (
                                "ofType",
                                Value::object(
                                    vec![("name", Value::scalar("Int"))].into_iter().collect(),
                                ),
                            ),
                        ]
                        .into_iter()
                        .collect(),
                    ),
                ),
            ]
            .into_iter()
            .collect(),
        )));
    });
}

// TODO: enable once [RFC 2565](https://github.com/rust-lang/rust/issues/60406) is implemented
// #[test]
// fn introspect_field_attr_arg_descr() {
//     run_args_info_query("attrArgDescr", |args| {
//         assert_eq!(args.len(), 1);

//         assert!(args.contains(&Value::object(
//             vec![
//                 ("name", Value::scalar("arg")),
//                 ("description", Value::scalar("The arg")),
//                 ("defaultValue", Value::null()),
//                 (
//                     "type",
//                     Value::object(
//                         vec![
//                             ("name", Value::null()),
//                             (
//                                 "ofType",
//                                 Value::object(
//                                     vec![("name", Value::scalar("Int"))].into_iter().collect(),
//                                 ),
//                             ),
//                         ]
//                         .into_iter()
//                         .collect(),
//                     ),
//                 ),
//             ]
//             .into_iter()
//             .collect(),
//         )));
//     });
// }

// TODO: enable once [RFC 2565](https://github.com/rust-lang/rust/issues/60406) is implemented
// #[test]
// fn introspect_field_attr_arg_descr_collapse() {
//     run_args_info_query("attrArgDescrCollapse", |args| {
//         assert_eq!(args.len(), 1);

//         assert!(args.contains(&Value::object(
//             vec![
//                 ("name", Value::scalar("arg")),
//                 ("description", Value::scalar("The arg\nand more details")),
//                 ("defaultValue", Value::null()),
//                 (
//                     "type",
//                     Value::object(
//                         vec![
//                             ("name", Value::null()),
//                             (
//                                 "ofType",
//                                 Value::object(
//                                     vec![("name", Value::scalar("Int"))].into_iter().collect(),
//                                 ),
//                             ),
//                         ]
//                         .into_iter()
//                         .collect(),
//                     ),
//                 ),
//             ]
//             .into_iter()
//             .collect(),
//         )));
//     });
// }

#[test]
fn introspect_field_arg_with_default() {
    run_args_info_query("argWithDefault", |args| {
        assert_eq!(args.len(), 1);

        assert!(args.contains(&Value::object(
            vec![
                ("name", Value::scalar("arg")),
                ("description", Value::null()),
                ("defaultValue", Value::scalar("123")),
                (
                    "type",
                    Value::object(
                        vec![("name", Value::scalar("Int")), ("ofType", Value::null())]
                            .into_iter()
                            .collect(),
                    ),
                ),
            ]
            .into_iter()
            .collect(),
        )));
    });
}

#[test]
fn introspect_field_multi_args_with_default() {
    run_args_info_query("multiArgsWithDefault", |args| {
        assert_eq!(args.len(), 2);

        assert!(args.contains(&Value::object(
            vec![
                ("name", Value::scalar("arg1")),
                ("description", Value::null()),
                ("defaultValue", Value::scalar("123")),
                (
                    "type",
                    Value::object(
                        vec![("name", Value::scalar("Int")), ("ofType", Value::null())]
                            .into_iter()
                            .collect(),
                    ),
                ),
            ]
            .into_iter()
            .collect(),
        )));

        assert!(args.contains(&Value::object(
            vec![
                ("name", Value::scalar("arg2")),
                ("description", Value::null()),
                ("defaultValue", Value::scalar("456")),
                (
                    "type",
                    Value::object(
                        vec![("name", Value::scalar("Int")), ("ofType", Value::null())]
                            .into_iter()
                            .collect(),
                    ),
                ),
            ]
            .into_iter()
            .collect(),
        )));
    });
}

#[test]
fn introspect_field_multi_args_with_default_trailing_comma() {
    run_args_info_query("multiArgsWithDefaultTrailingComma", |args| {
        assert_eq!(args.len(), 2);

        assert!(args.contains(&Value::object(
            vec![
                ("name", Value::scalar("arg1")),
                ("description", Value::null()),
                ("defaultValue", Value::scalar("123")),
                (
                    "type",
                    Value::object(
                        vec![("name", Value::scalar("Int")), ("ofType", Value::null())]
                            .into_iter()
                            .collect(),
                    ),
                ),
            ]
            .into_iter()
            .collect(),
        )));

        assert!(args.contains(&Value::object(
            vec![
                ("name", Value::scalar("arg2")),
                ("description", Value::null()),
                ("defaultValue", Value::scalar("456")),
                (
                    "type",
                    Value::object(
                        vec![("name", Value::scalar("Int")), ("ofType", Value::null())]
                            .into_iter()
                            .collect(),
                    ),
                ),
            ]
            .into_iter()
            .collect(),
        )));
    });
}

#[test]
fn introspect_field_arg_with_default_descr() {
    run_args_info_query("argWithDefaultDescr", |args| {
        assert_eq!(args.len(), 1);

        assert!(args.contains(&Value::object(
            vec![
                ("name", Value::scalar("arg")),
                ("description", Value::scalar("The arg")),
                ("defaultValue", Value::scalar("123")),
                (
                    "type",
                    Value::object(
                        vec![("name", Value::scalar("Int")), ("ofType", Value::null())]
                            .into_iter()
                            .collect(),
                    ),
                ),
            ]
            .into_iter()
            .collect(),
        )));
    });
}

#[test]
fn introspect_field_multi_args_with_default_descr() {
    run_args_info_query("multiArgsWithDefaultDescr", |args| {
        assert_eq!(args.len(), 2);

        assert!(args.contains(&Value::object(
            vec![
                ("name", Value::scalar("arg1")),
                ("description", Value::scalar("The first arg")),
                ("defaultValue", Value::scalar("123")),
                (
                    "type",
                    Value::object(
                        vec![("name", Value::scalar("Int")), ("ofType", Value::null())]
                            .into_iter()
                            .collect(),
                    ),
                ),
            ]
            .into_iter()
            .collect(),
        )));

        assert!(args.contains(&Value::object(
            vec![
                ("name", Value::scalar("arg2")),
                ("description", Value::scalar("The second arg")),
                ("defaultValue", Value::scalar("456")),
                (
                    "type",
                    Value::object(
                        vec![("name", Value::scalar("Int")), ("ofType", Value::null())]
                            .into_iter()
                            .collect(),
                    ),
                ),
            ]
            .into_iter()
            .collect(),
        )));
    });
}

#[test]
fn introspect_field_multi_args_with_default_trailing_comma_descr() {
    run_args_info_query("multiArgsWithDefaultTrailingCommaDescr", |args| {
        assert_eq!(args.len(), 2);

        assert!(args.contains(&Value::object(
            vec![
                ("name", Value::scalar("arg1")),
                ("description", Value::scalar("The first arg")),
                ("defaultValue", Value::scalar("123")),
                (
                    "type",
                    Value::object(
                        vec![("name", Value::scalar("Int")), ("ofType", Value::null())]
                            .into_iter()
                            .collect(),
                    ),
                ),
            ]
            .into_iter()
            .collect(),
        )));

        assert!(args.contains(&Value::object(
            vec![
                ("name", Value::scalar("arg2")),
                ("description", Value::scalar("The second arg")),
                ("defaultValue", Value::scalar("456")),
                (
                    "type",
                    Value::object(
                        vec![("name", Value::scalar("Int")), ("ofType", Value::null())]
                            .into_iter()
                            .collect(),
                    ),
                ),
            ]
            .into_iter()
            .collect(),
        )));
    });
}

#[test]
fn introspect_field_args_with_complex_default() {
    run_args_info_query("argsWithComplexDefault", |args| {
        assert_eq!(args.len(), 2);

        assert!(args.contains(&Value::object(
            vec![
                ("name", Value::scalar("arg1")),
                ("description", Value::scalar("A string default argument")),
                ("defaultValue", Value::scalar(r#""test""#)),
                (
                    "type",
                    Value::object(
                        vec![("name", Value::scalar("String")), ("ofType", Value::null())]
                            .into_iter()
                            .collect(),
                    ),
                ),
            ]
            .into_iter()
            .collect(),
        )));

        assert!(args.contains(&Value::object(
            vec![
                ("name", Value::scalar("arg2")),
                (
                    "description",
                    Value::scalar("An input object default argument"),
                ),
                ("defaultValue", Value::scalar(r#"{x: 1}"#)),
                (
                    "type",
                    Value::object(
                        vec![("name", Value::scalar("Point")), ("ofType", Value::null())]
                            .into_iter()
                            .collect(),
                    ),
                ),
            ]
            .into_iter()
            .collect(),
        )));
    });
}
