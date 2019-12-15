use super::util;
use crate::{graphql_value, EmptyMutation, RootNode};

#[derive(Default)]
struct Context {
    flag1: bool,
}

impl crate::Context for Context {}

struct WithLifetime<'a> {
    value: &'a str,
}

#[crate::object_internal(Context=Context)]
impl<'a> WithLifetime<'a> {
    fn value(&'a self) -> &'a str {
        self.value
    }
}

struct WithContext;

#[crate::object_internal(Context=Context)]
impl WithContext {
    fn ctx(ctx: &Context) -> bool {
        ctx.flag1
    }
}

#[derive(Default)]
struct Query {
    b: bool,
}

#[crate::object_internal(
    scalar = crate::DefaultScalarValue,
    name = "Query",
    context = Context,
)]
/// Query Description.
impl<'a> Query {
    #[graphql(description = "With Self Description")]
    fn with_self(&self) -> bool {
        self.b
    }

    fn independent() -> i32 {
        100
    }

    fn with_executor(_exec: &Executor<Context>) -> bool {
        true
    }

    fn with_executor_and_self(&self, _exec: &Executor<Context>) -> bool {
        true
    }

    fn with_context(_context: &Context) -> bool {
        true
    }

    fn with_context_and_self(&self, _context: &Context) -> bool {
        true
    }

    #[graphql(name = "renamed")]
    fn has_custom_name() -> bool {
        true
    }

    #[graphql(description = "attr")]
    fn has_description_attr() -> bool {
        true
    }

    /// Doc description
    fn has_description_doc_comment() -> bool {
        true
    }

    fn has_argument(arg1: bool) -> bool {
        arg1
    }

    fn default_argument(#[graphql(default = true)] default_arg: bool) -> bool {
        default_arg
    }

    fn arg_with_description(#[graphql(description = "my argument description")] arg: bool) -> bool {
        arg
    }

    fn renamed_argument(#[graphql(name = new_name)] old_name: bool) -> bool {
        old_name
    }

    fn with_context_child(&self) -> WithContext {
        WithContext
    }

    fn with_lifetime_child(&self) -> WithLifetime<'a> {
        WithLifetime { value: "blub" }
    }

    fn with_mut_arg(mut arg: bool) -> bool {
        if arg {
            arg = !arg;
        }
        arg
    }
}

#[derive(Default)]
struct Mutation;

#[crate::object_internal(context = Context)]
impl Mutation {
    fn empty() -> bool {
        true
    }
}

fn juniper_value_to_serde_json_value(
    value: &crate::Value<crate::DefaultScalarValue>,
) -> serde_json::Value {
    serde_json::from_str(&serde_json::to_string(value).unwrap()).unwrap()
}

#[test]
fn object_introspect() {
    let res = util::run_info_query::<Query, Mutation, Context>("Query");

    assert_json_diff::assert_json_include!(
        actual: juniper_value_to_serde_json_value(&res),
        expected: serde_json::json!({
            "name": "Query",
            "description": "Query Description.",
            "fields": [
                {
                    "name": "withSelf",
                    "description": "With Self Description",
                    "args": [],
                },
                {
                    "name": "independent",
                    "description": None::<String>,
                    "args": [],
                },
                {
                    "name": "withExecutor",
                    "description": None::<String>,
                    "args": [],
                },
                {
                    "name": "withExecutorAndSelf",
                    "description": None::<String>,
                    "args": [],
                },
                {
                    "name": "withContext",
                    "description": None::<String>,
                    "args": [],
                },
                {
                    "name": "withContextAndSelf",
                    "description": None::<String>,
                    "args": [],
                },
                {
                    "name": "renamed",
                    "description": None::<String>,
                    "args": [],
                },
                {
                    "name": "hasDescriptionAttr",
                    "description": "attr",
                    "args": [],
                },
                {
                    "name": "hasDescriptionDocComment",
                    "description": "Doc description",
                    "args": [],
                },
                {
                    "name": "hasArgument",
                    "description": None::<String>,
                    "args": [
                        {
                            "name": "arg1",
                            "description": None::<String>,
                            "type": {
                                "name": None::<String>,
                            },
                        }
                    ],
                },
                {
                    "name": "defaultArgument",
                    "description": None::<String>,
                    "args": [
                        {
                            "name": "defaultArg",
                            "description": None::<String>,
                            "type": {
                                "name": "Boolean",
                            },
                        }
                    ],
                },
                {
                    "name": "argWithDescription",
                    "description": None::<String>,
                    "args": [
                        {
                            "name": "arg",
                            "description": "my argument description",
                            "type": {
                                "name": None::<String>
                            },
                        }
                    ],
                },
                {
                    "name": "renamedArgument",
                    "description": None::<String>,
                    "args": [
                        {
                            "name": "newName",
                            "description": None::<String>,
                            "type": {
                                "name": None::<String>
                            },
                        }
                    ],
                },
                {
                    "name": "withContextChild",
                    "description": None::<String>,
                    "args": [],
                },
                {
                    "name": "withLifetimeChild",
                    "description": None::<String>,
                    "args": [],
                },
                {
                    "name": "withMutArg",
                    "description": None::<String>,
                    "args": [
                        {
                            "name": "arg",
                            "description": None::<String>,
                            "type": {
                                "name": None::<String>,
                            },
                        }
                    ],
                },
            ]
        })
    );
}

#[test]
fn object_query() {
    let doc = r#"
    query {
        withSelf
        independent
        withExecutor
        withExecutorAndSelf
        withContext
        withContextAndSelf
        renamed
        hasArgument(arg1: true)
        defaultArgument
        argWithDescription(arg: true)
        withContextChild {
            ctx
        }
        withLifetimeChild {
            value
        }
        withMutArg(arg: true)
    }
    "#;
    let schema = RootNode::new(Query { b: true }, EmptyMutation::<Context>::new());
    let vars = std::collections::HashMap::new();

    let (result, errs) = crate::execute(doc, None, &schema, &vars, &Context { flag1: true })
        .expect("Execution failed");
    assert_eq!(errs, []);
    assert_eq!(
        result,
        graphql_value!({
            "withSelf": true,
            "independent": 100,
            "withExecutor": true,
            "withExecutorAndSelf": true,
            "withContext": true,
            "withContextAndSelf": true,
            "renamed": true,
            "hasArgument": true,
            "defaultArgument": true,
            "argWithDescription": true,
            "withContextChild": { "ctx": true },
            "withLifetimeChild": { "value": "blub" },
            "withMutArg": false,
        })
    );
}
