use bson::{oid::ObjectId, DateTime as UtcDateTime};
use chrono::prelude::*;

use crate::{
    parser::{ParseError, ScalarToken, Token},
    value::ParseScalarResult,
    Value,
};

#[crate::graphql_scalar(description = "ObjectId")]
impl<S> GraphQLScalar for ObjectId
where
    S: ScalarValue,
{
    fn resolve(&self) -> Value {
        Value::scalar(self.to_hex())
    }

    fn from_input_value(v: &InputValue) -> Option<ObjectId> {
        v.as_string_value()
            .and_then(|s| ObjectId::parse_str(s).ok())
    }

    fn from_str<'a>(value: ScalarToken<'a>) -> ParseScalarResult<'a, S> {
        if let ScalarToken::String(value) = value {
            Ok(S::from(value.to_owned()))
        } else {
            Err(ParseError::UnexpectedToken(Token::Scalar(value)))
        }
    }
}

#[crate::graphql_scalar(description = "UtcDateTime")]
impl<S> GraphQLScalar for UtcDateTime
where
    S: ScalarValue,
{
    fn resolve(&self) -> Value {
        Value::scalar(DateTime::from(*self).to_rfc3339())
    }

    fn from_input_value(v: &InputValue) -> Option<UtcDateTime> {
        v.as_string_value()
            .and_then(|s| (s.parse::<DateTime<Utc>>().ok()))
            .map(UtcDateTime::from)
    }

    fn from_str<'a>(value: ScalarToken<'a>) -> ParseScalarResult<'a, S> {
        if let ScalarToken::String(value) = value {
            Ok(S::from(value.to_owned()))
        } else {
            Err(ParseError::UnexpectedToken(Token::Scalar(value)))
        }
    }
}

#[cfg(test)]
mod test {
    use crate::{value::DefaultScalarValue, InputValue};
    use bson::{oid::ObjectId, DateTime as UtcDateTime};
    use chrono::prelude::*;

    #[test]
    fn objectid_from_input_value() {
        let raw = "53e37d08776f724e42000000";
        let input: InputValue<DefaultScalarValue> = InputValue::scalar(raw.to_string());

        let parsed: ObjectId = crate::FromInputValue::from_input_value(&input).unwrap();
        let id = ObjectId::parse_str(raw).unwrap();

        assert_eq!(parsed, id);
    }

    #[test]
    fn utcdatetime_from_input_value() {
        let raw = "2020-03-23T17:38:32.446+00:00";
        let input: InputValue<DefaultScalarValue> = InputValue::scalar(raw.to_string());

        let parsed: UtcDateTime = crate::FromInputValue::from_input_value(&input).unwrap();
        let date_time = UtcDateTime::from(
            DateTime::parse_from_rfc3339(raw)
                .unwrap()
                .with_timezone(&Utc),
        );

        assert_eq!(parsed, date_time);
    }
}
