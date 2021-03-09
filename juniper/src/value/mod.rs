mod object;
mod scalar;

use std::{
    any::TypeId,
    fmt::{self, Display, Formatter},
    mem,
};

use crate::{
    ast::{InputValue, ToInputValue},
    parser::Spanning,
};

pub use self::{
    object::Object,
    scalar::{DefaultScalarValue, ParseScalarResult, ParseScalarValue, ScalarValue},
};

/// Serializable value returned from query and field execution.
///
/// Used by the execution engine and resolvers to build up the response
/// structure. Similar to the `Json` type found in the serialize crate.
///
/// It is also similar to the `InputValue` type, but can not contain enum
/// values or variables. Also, lists and objects do not contain any location
/// information since they are generated by resolving fields and values rather
/// than parsing a source query.
#[derive(Debug, PartialEq, Clone)]
#[allow(missing_docs)]
pub enum Value<S = DefaultScalarValue> {
    Null,
    Scalar(S),
    List(Vec<Value<S>>),
    Object(Object<S>),
}

impl<S: ScalarValue> Value<S> {
    // CONSTRUCTORS

    /// Construct a null value.
    pub fn null() -> Self {
        Self::Null
    }

    /// Construct an integer value.
    #[deprecated(since = "0.11.0", note = "Use `Value::scalar` instead")]
    pub fn int(i: i32) -> Self {
        Self::scalar(i)
    }

    /// Construct a floating point value.
    #[deprecated(since = "0.11.0", note = "Use `Value::scalar` instead")]
    pub fn float(f: f64) -> Self {
        Self::scalar(f)
    }

    /// Construct a string value.
    #[deprecated(since = "0.11.0", note = "Use `Value::scalar` instead")]
    pub fn string(s: &str) -> Self {
        Self::scalar(s.to_owned())
    }

    /// Construct a boolean value.
    #[deprecated(since = "0.11.0", note = "Use `Value::scalar` instead")]
    pub fn boolean(b: bool) -> Self {
        Self::scalar(b)
    }

    /// Construct a list value.
    pub fn list(l: Vec<Self>) -> Self {
        Self::List(l)
    }

    /// Construct an object value.
    pub fn object(o: Object<S>) -> Self {
        Self::Object(o)
    }

    /// Construct a scalar value
    pub fn scalar<T>(s: T) -> Self
    where
        T: Into<S>,
    {
        Self::Scalar(s.into())
    }

    // DISCRIMINATORS

    /// Does this value represent null?
    pub fn is_null(&self) -> bool {
        matches!(*self, Self::Null)
    }

    /// View the underlying scalar value if present
    pub fn as_scalar_value<'a, T>(&'a self) -> Option<&'a T>
    where
        &'a S: Into<Option<&'a T>>,
    {
        match *self {
            Self::Scalar(ref s) => s.into(),
            _ => None,
        }
    }

    /// View the underlying float value, if present.
    pub fn as_float_value(&self) -> Option<f64> {
        match self {
            Self::Scalar(ref s) => s.as_float(),
            _ => None,
        }
    }

    /// View the underlying object value, if present.
    pub fn as_object_value(&self) -> Option<&Object<S>> {
        match *self {
            Self::Object(ref o) => Some(o),
            _ => None,
        }
    }

    /// Convert this value into an Object.
    ///
    /// Returns None if value is not an Object.
    pub fn into_object(self) -> Option<Object<S>> {
        match self {
            Self::Object(o) => Some(o),
            _ => None,
        }
    }

    /// Mutable view into the underlying object value, if present.
    pub fn as_mut_object_value(&mut self) -> Option<&mut Object<S>> {
        match *self {
            Self::Object(ref mut o) => Some(o),
            _ => None,
        }
    }

    /// View the underlying list value, if present.
    pub fn as_list_value(&self) -> Option<&Vec<Self>> {
        match *self {
            Self::List(ref l) => Some(l),
            _ => None,
        }
    }

    /// View the underlying scalar value, if present
    pub fn as_scalar(&self) -> Option<&S> {
        match *self {
            Self::Scalar(ref s) => Some(s),
            _ => None,
        }
    }

    /// View the underlying string value, if present.
    pub fn as_string_value<'a>(&'a self) -> Option<&'a str>
    where
        Option<&'a String>: From<&'a S>,
    {
        self.as_scalar_value::<String>().map(|s| s as &str)
    }

    /// Maps the [`ScalarValue`] type of this [`Value`] into the specified one.
    pub fn map_scalar_value<Into: ScalarValue>(self) -> Value<Into> {
        if TypeId::of::<Into>() == TypeId::of::<S>() {
            // This is totally safe, because we're transmuting the value into itself,
            // so no invariants may change and we're just satisfying the type checker.
            let val = mem::ManuallyDrop::new(self);
            unsafe { mem::transmute_copy(&*val) }
        } else {
            match self {
                Self::Null => Value::Null,
                Self::Scalar(s) => Value::Scalar(s.into_another()),
                Self::List(l) => Value::List(l.into_iter().map(Value::map_scalar_value).collect()),
                Self::Object(o) => Value::Object(
                    o.into_iter()
                        .map(|(k, v)| (k, v.map_scalar_value()))
                        .collect(),
                ),
            }
        }
    }
}

impl<S: ScalarValue> ToInputValue<S> for Value<S> {
    fn to_input_value(&self) -> InputValue<S> {
        match *self {
            Value::Null => InputValue::Null,
            Value::Scalar(ref s) => InputValue::Scalar(s.clone()),
            Value::List(ref l) => InputValue::List(
                l.iter()
                    .map(|x| Spanning::unlocated(x.to_input_value()))
                    .collect(),
            ),
            Value::Object(ref o) => InputValue::Object(
                o.iter()
                    .map(|(k, v)| {
                        (
                            Spanning::unlocated(k.clone()),
                            Spanning::unlocated(v.to_input_value()),
                        )
                    })
                    .collect(),
            ),
        }
    }
}

impl<S: ScalarValue> Display for Value<S> {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            Value::Null => write!(f, "null"),
            Value::Scalar(s) => {
                if let Some(string) = s.as_string() {
                    write!(f, "\"{}\"", string)
                } else {
                    write!(f, "{}", s)
                }
            }
            Value::List(list) => {
                write!(f, "[")?;
                for (idx, item) in list.iter().enumerate() {
                    write!(f, "{}", item)?;
                    if idx < list.len() - 1 {
                        write!(f, ", ")?;
                    }
                }
                write!(f, "]")?;

                Ok(())
            }
            Value::Object(obj) => {
                write!(f, "{{")?;
                for (idx, (key, value)) in obj.iter().enumerate() {
                    write!(f, "\"{}\": {}", key, value)?;

                    if idx < obj.field_count() - 1 {
                        write!(f, ", ")?;
                    }
                }
                write!(f, "}}")?;

                Ok(())
            }
        }
    }
}

impl<S, T> From<Option<T>> for Value<S>
where
    S: ScalarValue,
    Value<S>: From<T>,
{
    fn from(v: Option<T>) -> Value<S> {
        match v {
            Some(v) => v.into(),
            None => Value::null(),
        }
    }
}

impl<'a, S> From<&'a str> for Value<S>
where
    S: ScalarValue,
{
    fn from(s: &'a str) -> Self {
        Value::scalar(s.to_owned())
    }
}

impl<S> From<String> for Value<S>
where
    S: ScalarValue,
{
    fn from(s: String) -> Self {
        Value::scalar(s)
    }
}

impl<S> From<i32> for Value<S>
where
    S: ScalarValue,
{
    fn from(i: i32) -> Self {
        Value::scalar(i)
    }
}

impl<S> From<f64> for Value<S>
where
    S: ScalarValue,
{
    fn from(f: f64) -> Self {
        Value::scalar(f)
    }
}

impl<S> From<bool> for Value<S>
where
    S: ScalarValue,
{
    fn from(b: bool) -> Self {
        Value::scalar(b)
    }
}

/// Construct JSON-like values by using JSON syntax
///
/// This macro can be used to create `Value` instances using a JSON syntax.
/// Value objects are used mostly when creating custom errors from fields.
///
/// Here are some examples; the resulting JSON will look just like what you
/// passed in.
/// ```rust
/// # use juniper::{Value, DefaultScalarValue, graphql_value};
/// # type V = Value<DefaultScalarValue>;
/// #
/// # fn main() {
/// # let _: V =
/// graphql_value!(None);
/// # let _: V =
/// graphql_value!(1234);
/// # let _: V =
/// graphql_value!("test");
/// # let _: V =
/// graphql_value!([ 1234, "test", true ]);
/// # let _: V =
/// graphql_value!({ "key": "value", "foo": 1234 });
/// # }
/// ```
#[macro_export]
macro_rules! graphql_value {
    ([ $($arg:tt),* $(,)* ]) => {
        $crate::Value::list(vec![
            $( graphql_value!($arg), )*
        ])
    };
    ({ $($key:tt : $val:tt ),* $(,)* }) => {
        $crate::Value::object(vec![
            $( ($key, graphql_value!($val)), )*
        ].into_iter().collect())
    };
    (None) => ($crate::Value::null());
    ($e:expr) => ($crate::Value::from($e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn value_macro_string() {
        let s: Value<DefaultScalarValue> = graphql_value!("test");
        assert_eq!(s, Value::scalar("test"));
    }

    #[test]
    fn value_macro_int() {
        let s: Value<DefaultScalarValue> = graphql_value!(123);
        assert_eq!(s, Value::scalar(123));
    }

    #[test]
    fn value_macro_float() {
        let s: Value<DefaultScalarValue> = graphql_value!(123.5);
        assert_eq!(s, Value::scalar(123.5));
    }

    #[test]
    fn value_macro_boolean() {
        let s: Value<DefaultScalarValue> = graphql_value!(false);
        assert_eq!(s, Value::scalar(false));
    }

    #[test]
    fn value_macro_option() {
        let s: Value<DefaultScalarValue> = graphql_value!(Some("test"));
        assert_eq!(s, Value::scalar("test"));
        let s: Value<DefaultScalarValue> = graphql_value!(None);
        assert_eq!(s, Value::null());
    }

    #[test]
    fn value_macro_list() {
        let s: Value<DefaultScalarValue> = graphql_value!([123, "Test", false]);
        assert_eq!(
            s,
            Value::list(vec![
                Value::scalar(123),
                Value::scalar("Test"),
                Value::scalar(false),
            ])
        );
        let s: Value<DefaultScalarValue> = graphql_value!([123, [456], 789]);
        assert_eq!(
            s,
            Value::list(vec![
                Value::scalar(123),
                Value::list(vec![Value::scalar(456)]),
                Value::scalar(789),
            ])
        );
    }

    #[test]
    fn value_macro_object() {
        let s: Value<DefaultScalarValue> = graphql_value!({ "key": 123, "next": true });
        assert_eq!(
            s,
            Value::object(
                vec![("key", Value::scalar(123)), ("next", Value::scalar(true))]
                    .into_iter()
                    .collect(),
            )
        );
    }

    #[test]
    fn display_null() {
        let s: Value<DefaultScalarValue> = graphql_value!(None);
        assert_eq!("null", format!("{}", s));
    }

    #[test]
    fn display_int() {
        let s: Value<DefaultScalarValue> = graphql_value!(123);
        assert_eq!("123", format!("{}", s));
    }

    #[test]
    fn display_float() {
        let s: Value<DefaultScalarValue> = graphql_value!(123.456);
        assert_eq!("123.456", format!("{}", s));
    }

    #[test]
    fn display_string() {
        let s: Value<DefaultScalarValue> = graphql_value!("foo");
        assert_eq!("\"foo\"", format!("{}", s));
    }

    #[test]
    fn display_bool() {
        let s: Value<DefaultScalarValue> = graphql_value!(false);
        assert_eq!("false", format!("{}", s));

        let s: Value<DefaultScalarValue> = graphql_value!(true);
        assert_eq!("true", format!("{}", s));
    }

    #[test]
    fn display_list() {
        let s: Value<DefaultScalarValue> = graphql_value!([1, None, "foo"]);
        assert_eq!("[1, null, \"foo\"]", format!("{}", s));
    }

    #[test]
    fn display_list_one_element() {
        let s: Value<DefaultScalarValue> = graphql_value!([1]);
        assert_eq!("[1]", format!("{}", s));
    }

    #[test]
    fn display_list_empty() {
        let s: Value<DefaultScalarValue> = graphql_value!([]);
        assert_eq!("[]", format!("{}", s));
    }

    #[test]
    fn display_object() {
        let s: Value<DefaultScalarValue> = graphql_value!({
            "int": 1,
            "null": None,
            "string": "foo",
        });
        assert_eq!(
            r#"{"int": 1, "null": null, "string": "foo"}"#,
            format!("{}", s)
        );
    }

    #[test]
    fn display_object_one_field() {
        let s: Value<DefaultScalarValue> = graphql_value!({
            "int": 1,
        });
        assert_eq!(r#"{"int": 1}"#, format!("{}", s));
    }

    #[test]
    fn display_object_empty() {
        let s = Value::<DefaultScalarValue>::object(Object::with_capacity(0));
        assert_eq!(r#"{}"#, format!("{}", s));
    }
}
