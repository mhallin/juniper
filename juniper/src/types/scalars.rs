use std::{
    char, convert::From, fmt, marker::PhantomData, ops::Deref, rc::Rc, thread::JoinHandle, u32,
};

use serde::{Deserialize, Serialize};

use crate::{
    ast::{InputValue, Selection, ToInputValue},
    executor::{ExecutionResult, Executor, Registry},
    parser::{LexerError, ParseError, ScalarToken, Token},
    schema::meta::MetaType,
    types::{
        async_await::GraphQLValueAsync,
        base::{GraphQLType, GraphQLValue},
        subscriptions::GraphQLSubscriptionValue,
    },
    value::{ParseScalarResult, ScalarValue, Value},
};

/// An ID as defined by the GraphQL specification
///
/// Represented as a string, but can be converted _to_ from an integer as well.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ID(String);

impl From<String> for ID {
    fn from(s: String) -> ID {
        ID(s)
    }
}

impl ID {
    /// Construct a new ID from anything implementing `Into<String>`
    pub fn new<S: Into<String>>(value: S) -> Self {
        ID(value.into())
    }
}

impl Deref for ID {
    type Target = str;

    fn deref(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ID {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[crate::graphql_scalar(name = "ID")]
impl<S> GraphQLScalar for ID
where
    S: ScalarValue,
{
    fn resolve(&self) -> Value {
        Value::scalar(self.0.clone())
    }

    fn from_input_value(v: &InputValue) -> Option<ID> {
        match *v {
            InputValue::Scalar(ref s) => s
                .as_string()
                .or_else(|| s.as_int().map(|i| i.to_string()))
                .map(ID),
            _ => None,
        }
    }

    fn from_str<'a>(value: ScalarToken<'a>) -> ParseScalarResult<'a, S> {
        match value {
            ScalarToken::String(value) | ScalarToken::Int(value) => Ok(S::from(value.to_owned())),
            _ => Err(ParseError::UnexpectedToken(Token::Scalar(value))),
        }
    }
}

#[crate::graphql_scalar(name = "String")]
impl<S> GraphQLScalar for String
where
    S: ScalarValue,
{
    fn resolve(&self) -> Value {
        Value::scalar(self.clone())
    }

    fn from_input_value(v: &InputValue) -> Option<String> {
        match *v {
            InputValue::Scalar(ref s) => s.as_string(),
            _ => None,
        }
    }

    fn from_str<'a>(value: ScalarToken<'a>) -> ParseScalarResult<'a, S> {
        if let ScalarToken::String(value) = value {
            let mut ret = String::with_capacity(value.len());
            let mut char_iter = value.chars();
            while let Some(ch) = char_iter.next() {
                match ch {
                    '\\' => match char_iter.next() {
                        Some('"') => {
                            ret.push('"');
                        }
                        Some('/') => {
                            ret.push('/');
                        }
                        Some('n') => {
                            ret.push('\n');
                        }
                        Some('r') => {
                            ret.push('\r');
                        }
                        Some('t') => {
                            ret.push('\t');
                        }
                        Some('\\') => {
                            ret.push('\\');
                        }
                        Some('f') => {
                            ret.push('\u{000c}');
                        }
                        Some('b') => {
                            ret.push('\u{0008}');
                        }
                        Some('u') => {
                            ret.push(parse_unicode_codepoint(&mut char_iter)?);
                        }
                        Some(s) => {
                            return Err(ParseError::LexerError(LexerError::UnknownEscapeSequence(
                                format!("\\{}", s),
                            )))
                        }
                        None => return Err(ParseError::LexerError(LexerError::UnterminatedString)),
                    },
                    ch => {
                        ret.push(ch);
                    }
                }
            }
            Ok(ret.into())
        } else {
            Err(ParseError::UnexpectedToken(Token::Scalar(value)))
        }
    }
}

fn parse_unicode_codepoint<'a, I>(char_iter: &mut I) -> Result<char, ParseError<'a>>
where
    I: Iterator<Item = char>,
{
    let escaped_code_point = char_iter
        .next()
        .ok_or_else(|| {
            ParseError::LexerError(LexerError::UnknownEscapeSequence(String::from("\\u")))
        })
        .and_then(|c1| {
            char_iter
                .next()
                .map(|c2| format!("{}{}", c1, c2))
                .ok_or_else(|| {
                    ParseError::LexerError(LexerError::UnknownEscapeSequence(format!("\\u{}", c1)))
                })
        })
        .and_then(|mut s| {
            char_iter
                .next()
                .ok_or_else(|| {
                    ParseError::LexerError(LexerError::UnknownEscapeSequence(format!(
                        "\\u{}",
                        s.clone()
                    )))
                })
                .map(|c2| {
                    s.push(c2);
                    s
                })
        })
        .and_then(|mut s| {
            char_iter
                .next()
                .ok_or_else(|| {
                    ParseError::LexerError(LexerError::UnknownEscapeSequence(format!(
                        "\\u{}",
                        s.clone()
                    )))
                })
                .map(|c2| {
                    s.push(c2);
                    s
                })
        })?;
    let code_point = u32::from_str_radix(&escaped_code_point, 16).map_err(|_| {
        ParseError::LexerError(LexerError::UnknownEscapeSequence(format!(
            "\\u{}",
            escaped_code_point
        )))
    })?;
    char::from_u32(code_point).ok_or_else(|| {
        ParseError::LexerError(LexerError::UnknownEscapeSequence(format!(
            "\\u{}",
            escaped_code_point
        )))
    })
}

impl<S> GraphQLType<S> for str
where
    S: ScalarValue,
{
    fn name(_: &()) -> Option<&'static str> {
        Some("String")
    }

    fn meta<'r>(_: &(), registry: &mut Registry<'r, S>) -> MetaType<'r, S>
    where
        S: 'r,
    {
        registry.build_scalar_type::<String>(&()).into_meta()
    }
}

impl<S> GraphQLValue<S> for str
where
    S: ScalarValue,
{
    type Context = ();
    type TypeInfo = ();

    fn type_name<'i>(&self, info: &'i Self::TypeInfo) -> Option<&'i str> {
        <Self as GraphQLType<S>>::name(info)
    }

    fn resolve(
        &self,
        _: &(),
        _: Option<&[Selection<S>]>,
        _: &Executor<Self::Context, S>,
    ) -> ExecutionResult<S> {
        Ok(Value::scalar(String::from(self)))
    }
}

impl<S> GraphQLValueAsync<S> for str
where
    S: ScalarValue + Send + Sync,
{
    fn resolve_async<'a>(
        &'a self,
        info: &'a Self::TypeInfo,
        selection_set: Option<&'a [Selection<S>]>,
        executor: &'a Executor<Self::Context, S>,
    ) -> crate::LocalBoxFuture<'a, crate::ExecutionResult<S>> {
        use futures::future;
        Box::pin(future::ready(self.resolve(info, selection_set, executor)))
    }
}

impl<'a, S> ToInputValue<S> for &'a str
where
    S: ScalarValue,
{
    fn to_input_value(&self) -> InputValue<S> {
        InputValue::scalar(String::from(*self))
    }
}

#[crate::graphql_scalar(name = "Boolean")]
impl<S> GraphQLScalar for bool
where
    S: ScalarValue,
{
    fn resolve(&self) -> Value {
        Value::scalar(*self)
    }

    fn from_input_value(v: &InputValue) -> Option<bool> {
        match *v {
            InputValue::Scalar(ref b) => b.as_boolean(),
            _ => None,
        }
    }

    fn from_str<'a>(value: ScalarToken<'a>) -> ParseScalarResult<'a, S> {
        // Bools are parsed separately - they shouldn't reach this code path
        Err(ParseError::UnexpectedToken(Token::Scalar(value)))
    }
}

#[crate::graphql_scalar(name = "Int")]
impl<S> GraphQLScalar for i32
where
    S: ScalarValue,
{
    fn resolve(&self) -> Value {
        Value::scalar(*self)
    }

    fn from_input_value(v: &InputValue) -> Option<i32> {
        match *v {
            InputValue::Scalar(ref i) => i.as_int(),
            _ => None,
        }
    }

    fn from_str<'a>(value: ScalarToken<'a>) -> ParseScalarResult<'a, S> {
        if let ScalarToken::Int(v) = value {
            v.parse()
                .map_err(|_| ParseError::UnexpectedToken(Token::Scalar(value)))
                .map(|s: i32| s.into())
        } else {
            Err(ParseError::UnexpectedToken(Token::Scalar(value)))
        }
    }
}

#[crate::graphql_scalar(name = "Float")]
impl<S> GraphQLScalar for f64
where
    S: ScalarValue,
{
    fn resolve(&self) -> Value {
        Value::scalar(*self)
    }

    fn from_input_value(v: &InputValue) -> Option<f64> {
        match *v {
            InputValue::Scalar(ref s) => s.as_float(),
            _ => None,
        }
    }

    fn from_str<'a>(value: ScalarToken<'a>) -> ParseScalarResult<'a, S> {
        match value {
            ScalarToken::Int(v) => v
                .parse()
                .map_err(|_| ParseError::UnexpectedToken(Token::Scalar(value)))
                .map(|s: i32| f64::from(s).into()),
            ScalarToken::Float(v) => v
                .parse()
                .map_err(|_| ParseError::UnexpectedToken(Token::Scalar(value)))
                .map(|s: f64| s.into()),
            ScalarToken::String(_) => Err(ParseError::UnexpectedToken(Token::Scalar(value))),
        }
    }
}

/// Utility type to define read-only schemas
///
/// If you instantiate `RootNode` with this as the mutation, no mutation will be
/// generated for the schema.
#[derive(Debug)]
pub struct EmptyMutation<T: ?Sized = ()>(PhantomData<JoinHandle<Box<T>>>);

// `EmptyMutation` doesn't use `T`, so should be `Send` and `Sync` even when `T` is not.
crate::sa::assert_impl_all!(EmptyMutation<Rc<String>>: Send, Sync);

impl<T: ?Sized> EmptyMutation<T> {
    /// Construct a new empty mutation
    #[inline]
    pub fn new() -> Self {
        Self(PhantomData)
    }
}

impl<S, T> GraphQLType<S> for EmptyMutation<T>
where
    S: ScalarValue,
{
    fn name(_: &()) -> Option<&'static str> {
        Some("_EmptyMutation")
    }

    fn meta<'r>(_: &(), registry: &mut Registry<'r, S>) -> MetaType<'r, S>
    where
        S: 'r,
    {
        registry.build_object_type::<Self>(&(), &[]).into_meta()
    }
}

impl<S, T> GraphQLValue<S> for EmptyMutation<T>
where
    S: ScalarValue,
{
    type Context = T;
    type TypeInfo = ();

    fn type_name<'i>(&self, info: &'i Self::TypeInfo) -> Option<&'i str> {
        <Self as GraphQLType<S>>::name(info)
    }
}

impl<S, T> GraphQLValueAsync<S> for EmptyMutation<T>
where
    Self::TypeInfo: Sync,
    S: ScalarValue + Send + Sync,
{
}

impl<T> Default for EmptyMutation<T> {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

/// Utillity type to define read-only schemas
///
/// If you instantiate `RootNode` with this as the subscription,
/// no subscriptions will be generated for the schema.
pub struct EmptySubscription<T: ?Sized = ()>(PhantomData<JoinHandle<Box<T>>>);

// `EmptySubscription` doesn't use `T`, so should be `Send` and `Sync` even when `T` is not.
crate::sa::assert_impl_all!(EmptySubscription<Rc<String>>: Send, Sync);

impl<T: ?Sized> EmptySubscription<T> {
    /// Construct a new empty subscription
    #[inline]
    pub fn new() -> Self {
        Self(PhantomData)
    }
}

impl<S, T> GraphQLType<S> for EmptySubscription<T>
where
    S: ScalarValue,
{
    fn name(_: &()) -> Option<&'static str> {
        Some("_EmptySubscription")
    }

    fn meta<'r>(_: &(), registry: &mut Registry<'r, S>) -> MetaType<'r, S>
    where
        S: 'r,
    {
        registry.build_object_type::<Self>(&(), &[]).into_meta()
    }
}

impl<S, T> GraphQLValue<S> for EmptySubscription<T>
where
    S: ScalarValue,
{
    type Context = T;
    type TypeInfo = ();

    fn type_name<'i>(&self, info: &'i Self::TypeInfo) -> Option<&'i str> {
        <Self as GraphQLType<S>>::name(info)
    }
}

impl<T, S> GraphQLSubscriptionValue<S> for EmptySubscription<T>
where
    Self::TypeInfo: Sync,
    S: ScalarValue + Send + Sync + 'static,
{
}

impl<T> Default for EmptySubscription<T> {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        parser::ScalarToken,
        value::{DefaultScalarValue, ParseScalarValue},
    };

    use super::{EmptyMutation, EmptySubscription, ID};

    #[test]
    fn test_id_from_string() {
        let actual = ID::from(String::from("foo"));
        let expected = ID(String::from("foo"));
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_id_new() {
        let actual = ID::new("foo");
        let expected = ID(String::from("foo"));
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_id_deref() {
        let id = ID(String::from("foo"));
        assert_eq!(id.len(), 3);
    }

    #[test]
    fn test_id_display() {
        let id = ID(String::from("foo"));
        assert_eq!(format!("{}", id), "foo");
    }

    #[test]
    fn parse_strings() {
        fn parse_string(s: &str, expected: &str) {
            let s =
                <String as ParseScalarValue<DefaultScalarValue>>::from_str(ScalarToken::String(s));
            assert!(s.is_ok(), "A parsing error occurred: {:?}", s);
            let s: Option<String> = s.unwrap().into();
            assert!(s.is_some(), "No string returned");
            assert_eq!(s.unwrap(), expected);
        }

        parse_string("simple", "simple");
        parse_string(" white space ", " white space ");
        parse_string(r#"quote \""#, "quote \"");
        parse_string(r#"escaped \n\r\b\t\f"#, "escaped \n\r\u{0008}\t\u{000c}");
        parse_string(r#"slashes \\ \/"#, "slashes \\ /");
        parse_string(
            r#"unicode \u1234\u5678\u90AB\uCDEF"#,
            "unicode \u{1234}\u{5678}\u{90ab}\u{cdef}",
        );
    }

    #[test]
    fn parse_f64_from_int() {
        for (v, expected) in &[
            ("0", 0),
            ("128", 128),
            ("1601942400", 1601942400),
            ("1696550400", 1696550400),
            ("-1", -1),
        ] {
            let n = <f64 as ParseScalarValue<DefaultScalarValue>>::from_str(ScalarToken::Int(v));
            assert!(n.is_ok(), "A parsing error occurred: {:?}", n.unwrap_err());

            let n: Option<f64> = n.unwrap().into();
            assert!(n.is_some(), "No f64 returned");
            assert_eq!(n.unwrap(), f64::from(*expected));
        }
    }

    #[test]
    fn parse_f64_from_float() {
        for (v, expected) in &[
            ("0.", 0.),
            ("1.2", 1.2),
            ("1601942400.", 1601942400.),
            ("1696550400.", 1696550400.),
            ("-1.2", -1.2),
        ] {
            let n = <f64 as ParseScalarValue<DefaultScalarValue>>::from_str(ScalarToken::Float(v));
            assert!(n.is_ok(), "A parsing error occurred: {:?}", n.unwrap_err());

            let n: Option<f64> = n.unwrap().into();
            assert!(n.is_some(), "No f64 returned");
            assert_eq!(n.unwrap(), *expected);
        }
    }

    #[test]
    fn empty_mutation_is_send() {
        fn check_if_send<T: Send>() {}
        check_if_send::<EmptyMutation<()>>();
    }

    #[test]
    fn empty_subscription_is_send() {
        fn check_if_send<T: Send>() {}
        check_if_send::<EmptySubscription<()>>();
    }

    #[test]
    fn default_is_invariant_over_type() {
        struct Bar;
        let _ = EmptySubscription::<Bar>::default();
        let _ = EmptyMutation::<Bar>::default();
    }
}
