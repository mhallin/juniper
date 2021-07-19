use crate::{
    ast::InputValue,
    schema::{
        meta::{Argument, EnumMeta, InputObjectMeta, MetaType},
        model::{SchemaType, TypeType},
    },
    value::ScalarValue,
};
use std::{collections::HashSet, fmt::Display, iter::Iterator};

pub fn non_null_error_message<T>(arg_type: T) -> String
where
    T: Display,
{
    format!("Type \"{}\" is not nullable", arg_type)
}

pub fn enum_error_message<T, U>(arg_value: T, arg_type: U) -> String
where
    T: Display,
    U: Display,
{
    format!("Invalid value \"{}\" for enum \"{}\"", arg_value, arg_type)
}

pub fn type_error_message<T, U>(arg_value: T, arg_type: U) -> String
where
    T: Display,
    U: Display,
{
    format!("Invalid value \"{}\" for type \"{}\"", arg_value, arg_type)
}

pub fn parser_error_message<T>(arg_type: T) -> String
where
    T: Display,
{
    format!("Parser error for \"{}\"", arg_type)
}

pub fn input_object_error_message<T>(arg_type: T) -> String
where
    T: Display,
{
    format!("\"{}\" is not an input object", arg_type)
}

pub fn field_error_message<T, U>(arg_type: T, field_name: U, error_message: &str) -> String
where
    T: Display,
    U: Display,
{
    format!(
        "Error on \"{}\" field \"{}\": {}",
        arg_type, field_name, error_message
    )
}

pub fn missing_field_error_message<T, U>(arg_type: T, missing_fields: U) -> String
where
    T: Display,
    U: Display,
{
    format!("\"{}\" is missing fields: {}", arg_type, missing_fields)
}

pub fn unknown_field_error_message<T, U>(arg_type: T, field_name: U) -> String
where
    T: Display,
    U: Display,
{
    format!(
        "Field \"{}\" does not exist on type \"{}\"",
        field_name, arg_type
    )
}

/// Returns an error string if the field is invalid
fn validate_object_field<S>(
    schema: &SchemaType<S>,
    object_type: &TypeType<S>,
    object_fields: &Vec<Argument<S>>,
    field_value: &InputValue<S>,
    field_key: &str,
) -> Option<String>
where
    S: ScalarValue,
{
    let field_type = object_fields
        .iter()
        .filter(|f| f.name == field_key)
        .map(|f| schema.make_type(&f.arg_type))
        .next();

    if let Some(field_arg_type) = field_type {
        let error_message = validate_literal_value(schema, &field_arg_type, field_value);

        if let Some(error_message) = error_message {
            Some(field_error_message(object_type, field_key, &error_message))
        } else {
            None
        }
    } else {
        Some(unknown_field_error_message(object_type, field_key))
    }
}

/// Returns an error string if the value is invalid
pub fn validate_literal_value<S>(
    schema: &SchemaType<S>,
    arg_type: &TypeType<S>,
    arg_value: &InputValue<S>,
) -> Option<String>
where
    S: ScalarValue,
{
    match *arg_type {
        TypeType::NonNull(ref inner) => {
            if arg_value.is_null() {
                Some(non_null_error_message(arg_type))
            } else {
                validate_literal_value(schema, inner, arg_value)
            }
        }
        TypeType::List(ref inner) => match *arg_value {
            InputValue::List(ref items) => items
                .iter()
                .find_map(|i| validate_literal_value(schema, inner, &i.item)),
            ref v => validate_literal_value(schema, inner, v),
        },
        TypeType::Concrete(t) => {
            // Even though InputValue::String can be parsed into an enum, they
            // are not valid as enum *literals* in a GraphQL query.
            if let (&InputValue::Scalar(_), Some(&MetaType::Enum(EnumMeta { .. }))) =
                (arg_value, arg_type.to_concrete())
            {
                return Some(enum_error_message(arg_value, arg_type));
            }

            match *arg_value {
                InputValue::Null | InputValue::Variable(_) => None,
                ref v @ InputValue::Scalar(_) | ref v @ InputValue::Enum(_) => {
                    if let Some(parse_fn) = t.input_value_parse_fn() {
                        if parse_fn(v) {
                            None
                        } else {
                            Some(type_error_message(arg_value, arg_type))
                        }
                    } else {
                        Some(parser_error_message(arg_type))
                    }
                }
                InputValue::List(_) => Some("Input lists are not literals".to_owned()),
                InputValue::Object(ref obj) => {
                    if let MetaType::InputObject(InputObjectMeta {
                        ref input_fields, ..
                    }) = *t
                    {
                        let mut remaining_required_fields = input_fields
                            .iter()
                            .filter_map(|f| {
                                if f.arg_type.is_non_null() {
                                    Some(&f.name)
                                } else {
                                    None
                                }
                            })
                            .collect::<HashSet<_>>();

                        let error_message = obj.iter().find_map(|&(ref key, ref value)| {
                            remaining_required_fields.remove(&key.item);
                            validate_object_field(
                                schema,
                                arg_type,
                                input_fields,
                                &value.item,
                                &key.item,
                            )
                        });

                        if error_message.is_some() {
                            return error_message;
                        }

                        if remaining_required_fields.is_empty() {
                            None
                        } else {
                            let missing_fields = remaining_required_fields
                                .into_iter()
                                .map(|s| format!("\"{}\"", &**s))
                                .collect::<Vec<_>>()
                                .join(", ");
                            Some(missing_field_error_message(arg_type, missing_fields))
                        }
                    } else {
                        Some(input_object_error_message(arg_type))
                    }
                }
            }
        }
    }
}
