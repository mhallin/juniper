use crate::{
    ast::VariableDefinition,
    parser::Spanning,
    types::utilities::validate_literal_value,
    validation::{ValidatorContext, Visitor},
    value::ScalarValue,
};

pub struct DefaultValuesOfCorrectType;

pub fn factory() -> DefaultValuesOfCorrectType {
    DefaultValuesOfCorrectType
}

impl<'a, S> Visitor<'a, S> for DefaultValuesOfCorrectType
where
    S: ScalarValue,
{
    fn enter_variable_definition(
        &mut self,
        ctx: &mut ValidatorContext<'a, S>,
        &(ref var_name, ref var_def): &'a (Spanning<&'a str>, VariableDefinition<S>),
    ) {
        if let Some(Spanning {
            item: ref var_value,
            ref start,
            ..
        }) = var_def.default_value
        {
            if var_def.var_type.item.is_non_null() {
                ctx.report_error(
                    &non_null_error_message(var_name.item, &format!("{}", var_def.var_type.item)),
                    &[*start],
                )
            } else {
                let meta_type = ctx.schema.make_type(&var_def.var_type.item);

                if let Some(error_message) =
                    validate_literal_value(ctx.schema, &meta_type, var_value)
                {
                    ctx.report_error(
                        &type_error_message(
                            var_name.item,
                            &format!("{}", var_def.var_type.item),
                            &error_message,
                        ),
                        &[*start],
                    );
                }
            }
        }
    }
}

fn type_error_message(arg_name: &str, type_name: &str, reason: &str) -> String {
    format!(
        "Invalid default value for argument \"{}\", expected type \"{}\".  Reason: {}",
        arg_name, type_name, reason
    )
}

fn non_null_error_message(arg_name: &str, type_name: &str) -> String {
    format!(
        "Argument \"{}\" has type \"{}\" and is not nullable, so it can't have a default value",
        arg_name, type_name
    )
}

#[cfg(test)]
mod tests {
    use super::{factory, non_null_error_message, type_error_message};

    use crate::{
        parser::SourcePosition,
        types::utilities,
        validation::{expect_fails_rule, expect_passes_rule, RuleError},
        value::DefaultScalarValue,
    };

    #[test]
    fn variables_with_no_default_values() {
        expect_passes_rule::<_, _, DefaultScalarValue>(
            factory,
            r#"
          query NullableValues($a: Int, $b: String, $c: ComplexInput) {
            dog { name }
          }
        "#,
        );
    }

    #[test]
    fn required_variables_without_default_values() {
        expect_passes_rule::<_, _, DefaultScalarValue>(
            factory,
            r#"
          query RequiredValues($a: Int!, $b: String!) {
            dog { name }
          }
        "#,
        );
    }

    #[test]
    fn variables_with_valid_default_values() {
        expect_passes_rule::<_, _, DefaultScalarValue>(
            factory,
            r#"
          query WithDefaultValues(
            $a: Int = 1,
            $b: String = "ok",
            $c: ComplexInput = { requiredField: true, intField: 3 }
          ) {
            dog { name }
          }
        "#,
        );
    }

    #[test]
    fn no_required_variables_with_default_values() {
        expect_fails_rule::<_, _, DefaultScalarValue>(
            factory,
            r#"
          query UnreachableDefaultValues($a: Int! = 3, $b: String! = "default") {
            dog { name }
          }
        "#,
            &[
                RuleError::new(
                    &non_null_error_message("a", "Int!"),
                    &[SourcePosition::new(53, 1, 52)],
                ),
                RuleError::new(
                    &non_null_error_message("b", "String!"),
                    &[SourcePosition::new(70, 1, 69)],
                ),
            ],
        );
    }

    #[test]
    fn variables_with_invalid_default_values() {
        expect_fails_rule::<_, _, DefaultScalarValue>(
            factory,
            r#"
          query InvalidDefaultValues(
            $a: Int = "one",
            $b: String = 4,
            $c: ComplexInput = "notverycomplex"
          ) {
            dog { name }
          }
        "#,
            &[
                RuleError::new(
                    &type_error_message(
                        "a",
                        "Int",
                        &utilities::type_error_message("\"one\"", "Int"),
                    ),
                    &[SourcePosition::new(61, 2, 22)],
                ),
                RuleError::new(
                    &type_error_message(
                        "b",
                        "String",
                        &utilities::type_error_message("4", "String"),
                    ),
                    &[SourcePosition::new(93, 3, 25)],
                ),
                RuleError::new(
                    &type_error_message(
                        "c",
                        "ComplexInput",
                        &utilities::type_error_message("\"notverycomplex\"", "ComplexInput"),
                    ),
                    &[SourcePosition::new(127, 4, 31)],
                ),
            ],
        );
    }

    #[test]
    fn complex_variables_missing_required_field() {
        expect_fails_rule::<_, _, DefaultScalarValue>(
            factory,
            r#"
          query MissingRequiredField($a: ComplexInput = {intField: 3}) {
            dog { name }
          }
        "#,
            &[RuleError::new(
                &type_error_message(
                    "a",
                    "ComplexInput",
                    &utilities::missing_field_error_message("ComplexInput", "\"requiredField\""),
                ),
                &[SourcePosition::new(57, 1, 56)],
            )],
        );
    }

    #[test]
    fn list_variables_with_invalid_item() {
        expect_fails_rule::<_, _, DefaultScalarValue>(
            factory,
            r#"
          query InvalidItem($a: [String] = ["one", 2]) {
            dog { name }
          }
        "#,
            &[RuleError::new(
                &type_error_message(
                    "a",
                    "[String]",
                    &utilities::type_error_message("2", "String"),
                ),
                &[SourcePosition::new(44, 1, 43)],
            )],
        );
    }
}
