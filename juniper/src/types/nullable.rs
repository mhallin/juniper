use crate::{
    ast::{FromInputValue, InputValue, Selection, ToInputValue},
    executor::{ExecutionResult, Executor, Registry},
    schema::meta::MetaType,
    types::{
        async_await::GraphQLValueAsync,
        base::{GraphQLType, GraphQLValue},
        marker::IsInputType,
    },
    value::{ScalarValue, Value},
};

/// `Nullable` can be used in situations where you need to distinguish between an implicitly and
/// explicitly null input value.
///
/// The GraphQL spec states that these two field calls are similar, but are not identical:
///
/// ```graphql
/// {
///   field(arg: null)
///   field
/// }
/// ```
///
/// The first has explicitly provided null to the argument “arg”, while the second has implicitly
/// not provided a value to the argument “arg”. These two forms may be interpreted differently. For
/// example, a mutation representing deleting a field vs not altering a field, respectively.
///
/// In cases where you do not need to be able to distinguish between the two types of null, you
/// should simply use `Option<T>`.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Nullable<T> {
    /// No value
    ImplicitNull,
    /// No value, explicitly specified to be null
    ExplicitNull,
    /// Some value `T`
    Some(T),
}

impl<T> Default for Nullable<T> {
    fn default() -> Self {
        Self::ImplicitNull
    }
}

impl<T> Nullable<T> {
    /// Returns `true` if the nullable is a `ExplicitNull` value.
    #[inline]
    pub fn is_explicit_null(&self) -> bool {
        matches!(self, Self::ExplicitNull)
    }

    /// Returns `true` if the nullable is a `ImplicitNull` value.
    #[inline]
    pub fn is_implicit_null(&self) -> bool {
        matches!(self, Self::ImplicitNull)
    }

    /// Returns `true` if the nullable is a `Some` value.
    #[inline]
    pub fn is_some(&self) -> bool {
        matches!(self, Self::Some(_))
    }

    /// Returns `true` if the nullable is not a `Some` value.
    #[inline]
    pub fn is_null(&self) -> bool {
        !matches!(self, Self::Some(_))
    }

    /// Converts from `&mut Nullable<T>` to `Nullable<&mut T>`.
    #[inline]
    pub fn as_mut(&mut self) -> Nullable<&mut T> {
        match *self {
            Self::Some(ref mut x) => Nullable::Some(x),
            Self::ImplicitNull => Nullable::ImplicitNull,
            Self::ExplicitNull => Nullable::ExplicitNull,
        }
    }

    /// Returns the contained `Some` value, consuming the `self` value.
    ///
    /// # Panics
    ///
    /// Panics if the value is not a `Some` with a custom panic message provided by `msg`.
    #[inline]
    #[track_caller]
    pub fn expect(self, msg: &str) -> T {
        self.some().expect(msg)
    }

    /// Returns the contained `Some` value or a provided default.
    #[inline]
    pub fn unwrap_or(self, default: T) -> T {
        self.some().unwrap_or(default)
    }

    /// Returns the contained `Some` value or computes it from a closure.
    #[inline]
    pub fn unwrap_or_else<F: FnOnce() -> T>(self, f: F) -> T {
        self.some().unwrap_or_else(f)
    }

    /// Maps a `Nullable<T>` to `Nullable<U>` by applying a function to a contained value.
    #[inline]
    pub fn map<U, F: FnOnce(T) -> U>(self, f: F) -> Nullable<U> {
        match self {
            Self::Some(x) => Nullable::Some(f(x)),
            Self::ImplicitNull => Nullable::ImplicitNull,
            Self::ExplicitNull => Nullable::ExplicitNull,
        }
    }

    /// Applies a function to the contained value (if any), or returns the provided default (if
    /// not).
    #[inline]
    pub fn map_or<U, F: FnOnce(T) -> U>(self, default: U, f: F) -> U {
        self.some().map_or(default, f)
    }

    /// Applies a function to the contained value (if any), or computes a default (if not).
    #[inline]
    pub fn map_or_else<U, D: FnOnce() -> U, F: FnOnce(T) -> U>(self, default: D, f: F) -> U {
        self.some().map_or_else(default, f)
    }

    /// Transforms the `Nullable<T>` into a `Result<T, E>`, mapping `Some(v)` to `Ok(v)` and
    /// `ImplicitNull` or `ExplicitNull` to `Err(err)`.
    #[inline]
    pub fn ok_or<E>(self, err: E) -> Result<T, E> {
        self.some().ok_or(err)
    }

    /// Transforms the `Nullable<T>` into a `Result<T, E>`, mapping `Some(v)` to `Ok(v)` and
    /// `ImplicitNull` or `ExplicitNull` to `Err(err())`.
    #[inline]
    pub fn ok_or_else<E, F: FnOnce() -> E>(self, err: F) -> Result<T, E> {
        self.some().ok_or_else(err)
    }

    /// Returns the nullable if it contains a value, otherwise returns `b`.
    #[inline]
    pub fn or(self, b: Self) -> Self {
        match self {
            Self::Some(_) => self,
            _ => b,
        }
    }

    /// Returns the nullable if it contains a value, otherwise calls `f` and
    /// returns the result.
    #[inline]
    pub fn or_else<F: FnOnce() -> Nullable<T>>(self, f: F) -> Nullable<T> {
        match self {
            Self::Some(_) => self,
            _ => f(),
        }
    }

    /// Replaces the actual value in the nullable by the value given in parameter, returning the
    /// old value if present, leaving a `Some` in its place without deinitializing either one.
    #[inline]
    pub fn replace(&mut self, value: T) -> Self {
        std::mem::replace(self, Self::Some(value))
    }

    /// Converts from `Nullable<T>` to `Option<T>`.
    pub fn some(self) -> Option<T> {
        match self {
            Self::Some(v) => Some(v),
            _ => None,
        }
    }

    /// Converts from `Nullable<T>` to `Option<Option<T>>`, mapping `Some(v)` to `Some(Some(v))`,
    /// `ExplicitNull` to `Some(None)`, and `ImplicitNull` to `None`.
    pub fn explicit(self) -> Option<Option<T>> {
        match self {
            Self::Some(v) => Some(Some(v)),
            Self::ExplicitNull => Some(None),
            Self::ImplicitNull => None,
        }
    }
}

impl<T: Copy> Nullable<&T> {
    /// Maps a `Nullable<&T>` to a `Nullable<T>` by copying the contents of the nullable.
    pub fn copied(self) -> Nullable<T> {
        self.map(|&t| t)
    }
}

impl<T: Copy> Nullable<&mut T> {
    /// Maps a `Nullable<&mut T>` to a `Nullable<T>` by copying the contents of the nullable.
    pub fn copied(self) -> Nullable<T> {
        self.map(|&mut t| t)
    }
}

impl<T: Clone> Nullable<&T> {
    /// Maps a `Nullable<&T>` to a `Nullable<T>` by cloning the contents of the nullable.
    pub fn cloned(self) -> Nullable<T> {
        self.map(|t| t.clone())
    }
}

impl<T: Clone> Nullable<&mut T> {
    /// Maps a `Nullable<&mut T>` to a `Nullable<T>` by cloning the contents of the nullable.
    pub fn cloned(self) -> Nullable<T> {
        self.map(|t| t.clone())
    }
}

impl<S, T> GraphQLType<S> for Nullable<T>
where
    T: GraphQLType<S>,
    S: ScalarValue,
{
    fn name(_: &Self::TypeInfo) -> Option<&'static str> {
        None
    }

    fn meta<'r>(info: &Self::TypeInfo, registry: &mut Registry<'r, S>) -> MetaType<'r, S>
    where
        S: 'r,
    {
        registry.build_nullable_type::<T>(info).into_meta()
    }
}

impl<S, T> GraphQLValue<S> for Nullable<T>
where
    S: ScalarValue,
    T: GraphQLValue<S>,
{
    type Context = T::Context;
    type TypeInfo = T::TypeInfo;

    fn type_name(&self, _: &Self::TypeInfo) -> Option<&'static str> {
        None
    }

    fn resolve(
        &self,
        info: &Self::TypeInfo,
        _: Option<&[Selection<S>]>,
        executor: &Executor<Self::Context, S>,
    ) -> ExecutionResult<S> {
        match *self {
            Self::Some(ref obj) => executor.resolve(info, obj),
            _ => Ok(Value::null()),
        }
    }
}

impl<S, T> GraphQLValueAsync<S> for Nullable<T>
where
    T: GraphQLValueAsync<S>,
    T::TypeInfo: Sync,
    S: ScalarValue + Send + Sync,
{
    fn resolve_async<'a>(
        &'a self,
        info: &'a Self::TypeInfo,
        _: Option<&'a [Selection<S>]>,
        executor: &'a Executor<Self::Context, S>,
    ) -> crate::LocalBoxFuture<'a, ExecutionResult<S>> {
        let f = async move {
            let value = match self {
                Self::Some(obj) => executor.resolve_into_value_async(info, obj).await,
                _ => Value::null(),
            };
            Ok(value)
        };
        Box::pin(f)
    }
}

impl<S, T> FromInputValue<S> for Nullable<T>
where
    T: FromInputValue<S>,
    S: ScalarValue,
{
    fn from_input_value(v: &InputValue<S>) -> Option<Nullable<T>> {
        match v {
            &InputValue::Null => Some(Self::ExplicitNull),
            v => v.convert().map(Self::Some),
        }
    }

    fn from_implicit_null() -> Self {
        Self::ImplicitNull
    }
}

impl<S, T> ToInputValue<S> for Nullable<T>
where
    T: ToInputValue<S>,
    S: ScalarValue,
{
    fn to_input_value(&self) -> InputValue<S> {
        match *self {
            Self::Some(ref v) => v.to_input_value(),
            _ => InputValue::null(),
        }
    }
}

impl<S, T> IsInputType<S> for Nullable<T>
where
    T: IsInputType<S>,
    S: ScalarValue,
{
}
