use indexmap::IndexMap;
use serde::{
    de,
    ser::{self, SerializeMap},
};
use serde_derive::Serialize;

use std::fmt;

use crate::{
    ast::InputValue,
    executor::ExecutionError,
    parser::{ParseError, SourcePosition, Spanning},
    validation::RuleError,
    GraphQLError, Object, ScalarValue, Value,
};

#[derive(Serialize)]
struct SerializeHelper {
    message: &'static str,
}

impl<T> ser::Serialize for ExecutionError<T>
where
    T: ScalarValue,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        let mut map = serializer.serialize_map(Some(4))?;

        map.serialize_key("message")?;
        map.serialize_value(self.error().message())?;

        let locations = vec![self.location()];
        map.serialize_key("locations")?;
        map.serialize_value(&locations)?;

        map.serialize_key("path")?;
        map.serialize_value(self.path())?;

        if !self.error().extensions().is_null() {
            map.serialize_key("extensions")?;
            map.serialize_value(self.error().extensions())?;
        }

        map.end()
    }
}

impl<'a> ser::Serialize for GraphQLError<'a> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        match *self {
            GraphQLError::ParseError(ref err) => vec![err].serialize(serializer),
            GraphQLError::ValidationError(ref errs) => errs.serialize(serializer),
            GraphQLError::NoOperationProvided => [SerializeHelper {
                message: "Must provide an operation",
            }]
            .serialize(serializer),
            GraphQLError::MultipleOperationsProvided => [SerializeHelper {
                message: "Must provide operation name \
                          if query contains multiple operations",
            }]
            .serialize(serializer),
            GraphQLError::UnknownOperationName => [SerializeHelper {
                message: "Unknown operation",
            }]
            .serialize(serializer),
            GraphQLError::IsSubscription => [SerializeHelper {
                message: "Expected query, got subscription",
            }]
            .serialize(serializer),
        }
    }
}

impl<'de, S> de::Deserialize<'de> for InputValue<S>
where
    S: ScalarValue,
{
    fn deserialize<D>(deserializer: D) -> Result<InputValue<S>, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        struct InputValueVisitor<S: ScalarValue>(S::Visitor);

        impl<S: ScalarValue> Default for InputValueVisitor<S> {
            fn default() -> Self {
                InputValueVisitor(S::Visitor::default())
            }
        }

        impl<'de, S> de::Visitor<'de> for InputValueVisitor<S>
        where
            S: ScalarValue,
        {
            type Value = InputValue<S>;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a valid input value")
            }

            fn visit_bool<E>(self, value: bool) -> Result<InputValue<S>, E>
            where
                E: de::Error,
            {
                self.0.visit_bool(value).map(InputValue::Scalar)
            }

            fn visit_i8<E>(self, value: i8) -> Result<InputValue<S>, E>
            where
                E: de::Error,
            {
                self.0.visit_i8(value).map(InputValue::Scalar)
            }

            fn visit_i16<E>(self, value: i16) -> Result<InputValue<S>, E>
            where
                E: de::Error,
            {
                self.0.visit_i16(value).map(InputValue::Scalar)
            }

            fn visit_i32<E>(self, value: i32) -> Result<InputValue<S>, E>
            where
                E: de::Error,
            {
                self.0.visit_i32(value).map(InputValue::Scalar)
            }

            fn visit_i64<E>(self, value: i64) -> Result<InputValue<S>, E>
            where
                E: de::Error,
            {
                self.0.visit_i64(value).map(InputValue::Scalar)
            }

            serde::serde_if_integer128! {
                fn visit_i128<E>(self, value: i128) -> Result<InputValue<S>, E>
                where
                    E: de::Error,
                {
                    self.0.visit_i128(value).map(InputValue::Scalar)
                }
            }

            fn visit_u8<E>(self, value: u8) -> Result<InputValue<S>, E>
            where
                E: de::Error,
            {
                self.0.visit_u8(value).map(InputValue::Scalar)
            }

            fn visit_u16<E>(self, value: u16) -> Result<InputValue<S>, E>
            where
                E: de::Error,
            {
                self.0.visit_u16(value).map(InputValue::Scalar)
            }

            fn visit_u32<E>(self, value: u32) -> Result<InputValue<S>, E>
            where
                E: de::Error,
            {
                self.0.visit_u32(value).map(InputValue::Scalar)
            }

            fn visit_u64<E>(self, value: u64) -> Result<InputValue<S>, E>
            where
                E: de::Error,
            {
                self.0.visit_u64(value).map(InputValue::Scalar)
            }

            serde::serde_if_integer128! {
                fn visit_u128<E>(self, value: u128) -> Result<InputValue<S>, E>
                where
                    E: de::Error,
                {
                    self.0.visit_u128(value).map(InputValue::Scalar)
                }
            }

            fn visit_f32<E>(self, value: f32) -> Result<InputValue<S>, E>
            where
                E: de::Error,
            {
                self.0.visit_f32(value).map(InputValue::Scalar)
            }

            fn visit_f64<E>(self, value: f64) -> Result<InputValue<S>, E>
            where
                E: de::Error,
            {
                self.0.visit_f64(value).map(InputValue::Scalar)
            }

            fn visit_char<E>(self, value: char) -> Result<InputValue<S>, E>
            where
                E: de::Error,
            {
                self.0.visit_char(value).map(InputValue::Scalar)
            }

            fn visit_str<E>(self, value: &str) -> Result<InputValue<S>, E>
            where
                E: de::Error,
            {
                self.0.visit_str(value).map(InputValue::Scalar)
            }

            fn visit_string<E>(self, value: String) -> Result<InputValue<S>, E>
            where
                E: de::Error,
            {
                self.0.visit_string(value).map(InputValue::Scalar)
            }

            fn visit_bytes<E>(self, bytes: &[u8]) -> Result<InputValue<S>, E>
            where
                E: de::Error,
            {
                self.0.visit_bytes(bytes).map(InputValue::Scalar)
            }

            fn visit_byte_buf<E>(self, bytes: Vec<u8>) -> Result<InputValue<S>, E>
            where
                E: de::Error,
            {
                self.0.visit_byte_buf(bytes).map(InputValue::Scalar)
            }

            fn visit_none<E>(self) -> Result<InputValue<S>, E>
            where
                E: de::Error,
            {
                Ok(InputValue::null())
            }

            fn visit_unit<E>(self) -> Result<InputValue<S>, E>
            where
                E: de::Error,
            {
                Ok(InputValue::null())
            }

            fn visit_seq<V>(self, mut visitor: V) -> Result<InputValue<S>, V::Error>
            where
                V: de::SeqAccess<'de>,
            {
                let mut values = Vec::new();

                while let Some(el) = visitor.next_element()? {
                    values.push(el);
                }

                Ok(InputValue::list(values))
            }

            fn visit_map<V>(self, mut visitor: V) -> Result<InputValue<S>, V::Error>
            where
                V: de::MapAccess<'de>,
            {
                let mut object = IndexMap::<String, InputValue<S>>::with_capacity(
                    visitor.size_hint().unwrap_or(0),
                );

                while let Some((key, value)) = visitor.next_entry()? {
                    object.insert(key, value);
                }

                Ok(InputValue::object(object))
            }
        }

        deserializer.deserialize_any(InputValueVisitor::default())
    }
}

impl<T> ser::Serialize for InputValue<T>
where
    T: ScalarValue,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        match *self {
            InputValue::Null | InputValue::Variable(_) => serializer.serialize_unit(),
            InputValue::Scalar(ref s) => s.serialize(serializer),
            InputValue::Enum(ref v) => serializer.serialize_str(v),
            InputValue::List(ref v) => v
                .iter()
                .map(|x| x.item.clone())
                .collect::<Vec<_>>()
                .serialize(serializer),
            InputValue::Object(ref v) => v
                .iter()
                .map(|&(ref k, ref v)| (k.item.clone(), v.item.clone()))
                .collect::<IndexMap<_, _>>()
                .serialize(serializer),
        }
    }
}

impl ser::Serialize for RuleError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        let mut map = serializer.serialize_map(Some(2))?;

        map.serialize_key("message")?;
        map.serialize_value(self.message())?;

        map.serialize_key("locations")?;
        map.serialize_value(self.locations())?;

        map.end()
    }
}

impl ser::Serialize for SourcePosition {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        let mut map = serializer.serialize_map(Some(2))?;

        let line = self.line() + 1;
        map.serialize_key("line")?;
        map.serialize_value(&line)?;

        let column = self.column() + 1;
        map.serialize_key("column")?;
        map.serialize_value(&column)?;

        map.end()
    }
}

impl<'a> ser::Serialize for Spanning<ParseError<'a>> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        let mut map = serializer.serialize_map(Some(2))?;

        let message = format!("{}", self.item);
        map.serialize_key("message")?;
        map.serialize_value(&message)?;

        let mut location = IndexMap::new();
        location.insert("line".to_owned(), self.start.line() + 1);
        location.insert("column".to_owned(), self.start.column() + 1);

        let locations = vec![location];

        map.serialize_key("locations")?;
        map.serialize_value(&locations)?;

        map.end()
    }
}

impl<T> ser::Serialize for Object<T>
where
    T: ser::Serialize,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        let mut map = serializer.serialize_map(Some(self.field_count()))?;

        for &(ref f, ref v) in self.iter() {
            map.serialize_key(f)?;
            map.serialize_value(v)?;
        }

        map.end()
    }
}

impl<T> ser::Serialize for Value<T>
where
    T: ser::Serialize,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        match *self {
            Value::Null => serializer.serialize_unit(),
            Value::Scalar(ref s) => s.serialize(serializer),
            Value::List(ref v) => v.serialize(serializer),
            Value::Object(ref v) => v.serialize(serializer),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ExecutionError, GraphQLError};
    use crate::{
        ast::InputValue,
        value::{DefaultScalarValue, Object},
        FieldError, Value,
    };
    use serde_json::{from_str, to_string};

    #[test]
    fn int() {
        assert_eq!(
            from_str::<InputValue<DefaultScalarValue>>("1235").unwrap(),
            InputValue::scalar(1235)
        );
    }

    #[test]
    fn float() {
        assert_eq!(
            from_str::<InputValue<DefaultScalarValue>>("2.0").unwrap(),
            InputValue::scalar(2.0)
        );
        // large value without a decimal part is also float
        assert_eq!(
            from_str::<InputValue<DefaultScalarValue>>("123567890123").unwrap(),
            InputValue::scalar(123567890123.0)
        );
    }

    #[test]
    fn errors() {
        assert_eq!(
            to_string(&GraphQLError::UnknownOperationName).unwrap(),
            r#"[{"message":"Unknown operation"}]"#
        );
    }

    #[test]
    fn error_extensions() {
        let mut obj: Object<DefaultScalarValue> = Object::with_capacity(1);
        obj.add_field("foo".to_string(), Value::scalar("bar"));
        assert_eq!(
            to_string(&ExecutionError::at_origin(FieldError::new(
                "foo error",
                Value::Object(obj),
            )))
            .unwrap(),
            r#"{"message":"foo error","locations":[{"line":1,"column":1}],"path":[],"extensions":{"foo":"bar"}}"#
        );
    }
}
