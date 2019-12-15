# master

### New way to customize arguments

See [#441](https://github.com/graphql-rust/juniper/pull/441).

You can now customize arguments by annotating them with `#[graphql(...)]` directly. Example:

```rust
#[juniper::object]
impl Query {
    fn some_field_with_a_description(
        #[graphql(
            name = newNameForArg,
            description = "My argument description",
            default = false,
        )]
        arg: bool
    ) -> bool {
        // ...
    }
}
```

The old style `#[graphql(arguments(...))]` is no longer supported.

Note that this requires Rust 1.39.

### Other changes

- Correctly handle raw identifiers in field and argument names.

# [[0.14.1] 2019-10-24](https://github.com/graphql-rust/juniper/releases/tag/juniper-0.14.1)

- Fix panic when an invalid scalar is used by a client [#434](https://github.com/graphql-rust/juniper/pull/434)
- `EmptyMutation` now implements `Send` [#443](https://github.com/graphql-rust/juniper/pull/443)

# [[0.14.0] 2019-09-29](https://github.com/graphql-rust/juniper/releases/tag/juniper-0.14.0)

- Require `url` 2.x if `url` feature is enabled.
- Improve lookahead visitability.
- Add ability to parse 'subscription'.

# [[0.13.1] 2019-07-29](https://github.com/graphql-rust/juniper/releases/tag/juniper-0.13.1)

- Fix a regression when using lookaheads with fragments containing nested types [#404](https://github.com/graphql-rust/juniper/pull/404)

- Allow `mut` arguments for resolver functions in `#[object]` macros [#402](https://github.com/graphql-rust/juniper/pull/402)

# [[0.13.0] 2019-07-19](https://github.com/graphql-rust/juniper/releases/tag/juniper-0.13.0)

### newtype ScalarValue derive

See [#345](https://github.com/graphql-rust/juniper/pull/345).

The newtype pattern can now be used with the `GraphQLScalarValue` custom derive
to easily implement custom scalar values that just wrap another scalar,
similar to serdes `#[serde(transparent)]` functionality.

Example:

```rust
#[derive(juniper::GraphQLScalarValue)]
struct UserId(i32);
```

### Other Changes

- The `ID` scalar now implements Serde's `Serialize` and `Deserialize`
- Add support for `dyn` trait object syntax to procedural macros

# [[0.12.0] 2019-05-16](https://github.com/graphql-rust/juniper/releases/tag/juniper-0.12.0)

### object macro

The `graphql_object!` macro is deprecated and will be removed in the future.
It is replaced by the new [object](https://docs.rs/juniper/latest/juniper/macro.object.html) procedural macro.

[#333](https://github.com/graphql-rust/juniper/pull/333)

### 2018 Edition

All crates were refactored to the Rust 2018 edition.

This should not have any impact on your code, since juniper already was 2018 compatible.

[#345](https://github.com/graphql-rust/juniper/pull/345)

### Other changes

- The minimum required Rust version is now `1.34.0`.
- The `GraphQLType` impl for () was removed to improve compile time safefty. [#355](https://github.com/graphql-rust/juniper/pull/355)
- The `ScalarValue` custom derive has been renamed to `GraphQLScalarValue`.
- Added built-in support for the canonical schema introspection query via
  `juniper::introspect()`.
  [#307](https://github.com/graphql-rust/juniper/issues/307)
- Fix introspection query validity
  The DirectiveLocation::InlineFragment had an invalid literal value,
  which broke third party tools like apollo cli.
- Added GraphQL Playground integration.
  The `DirectiveLocation::InlineFragment` had an invalid literal value,
  which broke third party tools like apollo cli.
- The return type of `value::object::Object::iter/iter_mut` has changed to `impl Iter`. [#312](https://github.com/graphql-rust/juniper/pull/312)
- Add `GraphQLRequest::operation_name` [#353](https://github.com/graphql-rust/juniper/pull/353)

# [0.11.1] 2018-12-19

## Changes

- The minimum required Rust version is now `1.30`.
- All macros and the custom derives now support the macro system changes properly
  and also support Rust 2018 edition crates.

  [#298](https://github.com/graphql-rust/juniper/pull/298)

# [0.11.0] 2018-12-17

## Changes

- The minimum required Rust version is now `1.30.0`.

  [#271](https://github.com/graphql-rust/juniper/pull/271)

- Juniper is now generic about the exact representation of scalar values. This
  allows downstream crates to add support for own scalar value representations.

  There are two use cases for this feature:

  - You want to support new scalar types not representable by the provided default
    scalar value representation like for example `i64`
  - You want to support a type from a third party crate that is not supported by juniper

  **Note:** This may need some changes in down stream code, especially if working with
  generic code. To retain the current behaviour use `DefaultScalarValue` as scalar value type

  [#251](https://github.com/graphql-rust/juniper/pull/251)

- The `GraphQLObject` and `GraphQLEnum` derives will mark graphql fields as
  `@deprecated` when struct fields or enum variants are marked with the
  builtin `#[deprecated]` attribute.

  The deprecation reason can be set using the `note = ...` meta item
  (e.g. `#[deprecated(note = "Replaced by betterField")]`).
  The `since` attribute is ignored.

  [#269](https://github.com/graphql-rust/juniper/pull/269)

* There is an alternative syntax for setting a field's _description_ and
  _deprecation reason_ in the `graphql_object!` and `graphql_interface!` macros.

  To **deprecate** a graphql field:

  ```rust
  // Original syntax for setting deprecation reason
  field deprecated "Reason" my_field() -> { ... }

  // New alternative syntax for deprecation reason.
  #[deprecated(note = "Reason")]
  field my_field() -> { ... }

  // You can now also deprecate without a reason.
  #[deprecated]
  field my_field() -> { ... }
  ```

  To set the **description** of a graphql field:

  ```rust
  // Original syntax for field descriptions
  field my_field() as "Description" -> { ... }

  // Original syntax for argument descriptions
  field my_field(
    floops: i32 as "The number of starfish to be returned. \
                    Can't be more than 100.",
  ) -> {
    ...
  }

  // New alternative syntax for field descriptions
  /// Description
  field my_field() -> { ... }

  // New alternative syntax for argument descriptions
  field my_field(
    /// The number of starfish to be returned.
    /// Can't be more than 100.
    arg: i32,
  ) -> {
    ...
  }

  // You can also use raw strings and const &'static str.
  //
  // Multiple docstrings will be collapsed into a single
  // description separated by newlines.
  /// This is my field.
  ///
  /// Make sure not to filtz the bitlet.
  /// Flitzing without a bitlet has undefined behaviour.
  ///
  #[doc = my_consts::ADDED_IN_VERSION_XYZ]
  field my_field() -> { ... }
  ```

  [#269](https://github.com/graphql-rust/juniper/pull/269)

# [0.10.0] 2018-09-13

## Changes

- Changed serialization of `NaiveDate` when using the optional `chronos` support.

  **Note:** while this is not a Rust breaking change, if you relied on the serialization format (perhaps by storing serialized data in a database or making asumptions in your client code written in another language) it could be a breaking change for your application.

  [#151](https://github.com/graphql-rust/juniper/pull/151)

- The `GraphQLObject`, `GraphQLInputObject`, and `GraphQLEnum` custom derives will reject
  invalid [names](http://facebook.github.io/graphql/October2016/#Name) at compile time.

  [#170](https://github.com/graphql-rust/juniper/pull/170)

- Large integers (> signed 32bit) are now deserialized as floats. Previously,
  they produced an "integer out of range" error. For languages that do not
  have a distinction between integer and floating point types (such as
  javascript), this meant large whole floating point values could not be
  decoded (because they were represented without a fractional value such as `.0`).

  [#179](https://github.com/graphql-rust/juniper/pull/179)

- The `GraphQLObject`, `GraphQLInputObject`, and `GraphQLEnum` custom derives
  now parse doc strings and use them as descriptions. This behavior can be
  overridden by using an explicit GraphQL `description` annotation such as
  `#[graphql(description = "my description")]`. [View documentation](https://graphql-rust.github.io/types/objects/defining_objects.html#defining-objects).

  [#194](https://github.com/graphql-rust/juniper/issues/194)

- Introduced `IntoFieldError` trait to allow custom error handling
  i.e. custom result type. The error type must implement this trait resolving
  the errors into `FieldError`. [View documentation](https://graphql-rust.github.io/types/objects/error_handling.html#structured-errors).

  [#40](https://github.com/graphql-rust/juniper/issues/40)

- `GraphQLType` and `ToInputValue` are now implemented for Arc<T>

  [#212](https://github.com/graphql-rust/juniper/pull/212)

- Error responses no longer have a _data_ field, instead, error details are stored in the _extensions_ field

  **Note:** while this is a breaking change, it is a necessary one to better align with the latest [GraphQL June 2018](https://facebook.github.io/graphql/June2018/#sec-Errors) specification, which defines the reserved _extensions_ field for error details. [View documentation](https://graphql-rust.github.io/types/objects/error_handling.html#structured-errors).

  [#219](https://github.com/graphql-rust/juniper/pull/219)

* The `GraphQLObject` and `GraphQLInputObject` custom derives
  now support lifetime annotations.

  [#225](https://github.com/graphql-rust/juniper/issues/225)

* When using the `GraphQLObject` custom derive, fields can now be omitted by annotating the field with `#[graphql(skip)]`. [View documentation](https://graphql-rust.github.io/types/objects/defining_objects.html#skipping-fields).

  [#220](https://github.com/graphql-rust/juniper/issues/220)

* Due to newer dependencies, the oldest Rust version supported is now 1.22.0

  [#231](https://github.com/graphql-rust/juniper/pull/231)

# [0.9.2] 2018-01-13

## Changes

### `__typename` for unions

The [`__typename`](http://graphql.org/learn/queries/#meta-fields) query meta-field now works on unions.

[#112](https://github.com/graphql-rust/juniper/issues/112)

### Debug impls.

http::GraphQLRequest now implements `Debug`.

# [0.9.0] 2017-12-03

## Changes

This is the first release in a long time.
Quite a few changes have accumulated since `0.8.1`, including multiple breaking
changes.

### Custom derive & macros

Juniper has gained custom derive implementations for input objects, objects and
enums.

- `#[derive(GraphQLInputObject)]`
- `#[derive(GraphQLEnum)]`
- `#[derive(GraphQLObject)]`

The `graphql_enum!` and `graphql_input_object!` macros did not provide any more
benefits, so they have been removed!
All functionality is now covered by custom derive.
Check the [docs](https://graphql-rust.github.io) to find out more.

### Web framework integrations - Iron & Rocket

The iron and rocket integrations were removed from the main crate, and are now
available via the [juniper_iron](https://crates.io/crates/juniper_iron) and
[juniper_rocket](https://crates.io/crates/juniper_rocket) crates.

### FieldError rewrite (custom data)

The `FieldError` type now supports custom data with the `Value` type from
serde_json. Use this to populate the `data` field in returned errors.

This also means that try! and `?` now work in resolvers, which is quite nice.

Also, the `ResultExt` extension and the `jtry!` macro were removed, since they
are redundant now!

### Dynamic Schemas

Juniper has gained support for dynamic schemas, thanks to @srijs.

That also means the type of `RootNode` has changed to include a lifetime.

The repository was restructured to a multi crate workspace to enable several new
features like custom_derive and an extracted parser.

#[#66](https://github.com/graphql-rust/juniper/pull/66)

### Data Type Integrations

Integrations with multiple popular crates was added to make working with them
easier.

- uuid
- url
- chrono

### Field Order

To better comply with the specification, order of requested fields is
now preserved.

[#82](https://github.com/graphql-rust/juniper/issues/82

### From/ToInputValue

The `::from` and `::to` methods in `From/ToInputValue` were renamed to
`from/to_input_value()` to not conflict with other methods.

[#90](https://github.com/graphql-rust/juniper/pull/90)

### Other changes

- Several small performance improvements
- Use [fnv](https://github.com/servo/rust-fnv) hash map for better performance

## Contributors

A big shoutout to the many contributors for this version, sorted alphabetically.

- Cameron Eldridge <mailto:cameldridge+git@gmail.com>
- Christian Legnitto <mailto:christian@legnitto.com>
- Jacob Haslehurst <mailto:jacob@haslehurst.net>
- Jane Keibler <mailto:wanderingelf14@gmail.com>
- Magnus Hallin <mailto:mhallin@fastmail.com>
- rushmorem <mailto:rushmore@webenchanter.com>
- Rushmore Mushambi <mailto:rushmore@webenchanter.com>
- Sagie Gur-Ari <mailto:sagiegurari@gmail.com>
- Sam Rijs <mailto:srijs@airpost.net>
- Stanko Krtalić <mailto:stanko.krtalic@gmail.com>
- theduke <mailto:chris@theduke.at>
- thomas-jeepe <mailto:penguinSoccer@outlook.com>

# [0.8.1] – 2017-06-15

Tiny release to fix broken crate metadata on crates.io.

# [0.8.0] – 2017-06-15

## Breaking changes

- To better comply with the specification, and to avoid weird bugs with very
  large positive or negative integers, support for `i64` has been completely
  dropped and replaced with `i32`. `i64` is no longer a valid GraphQL type in
  Juniper, and `InputValue`/`Value` can only represent 32 bit integers.

  If an incoming integer is out of range for a 32 bit signed integer type, an
  error will be returned to the client.
  ([#52](https://github.com/graphql-rust/juniper/issues/52),
  [#49](https://github.com/graphql-rust/juniper/issues/49))

- Serde has been updated to 1.0. If your application depends on an older
  version, you will need to first update your application before you can upgrade
  to a more recent Juniper. ([#43](https://github.com/graphql-rust/juniper/pull/43))

- `rustc_serialize` support has been dropped since this library is now
  deprecated. ([#51](https://github.com/graphql-rust/juniper/pull/51))

## New features

- A new `rocket-handlers` feature now includes some tools to use the
  [Rocket](https://rocket.rs) framework. [A simple
  example](juniper_rocket/examples/rocket-server.rs) has been added to the examples folder.

## Bugfixes

- A panic in the parser has been replaced with a proper error
  ([#44](https://github.com/graphql-rust/juniper/pull/44))

# [0.7.0] – 2017-02-26

### Breaking changes

- The `iron-handlers` feature now depends on Iron 0.5
  ([#30](https://github.com/graphql-rust/juniper/pull/30)). Because of
  this, support for Rust 1.12 has been dropped. It might still work if
  you're not using the Iron integrations feature, however.

### New features

- Input objects defined by the `graphql_input_object!` can now be used
  as default values to field arguments and other input object fields.

# [0.6.3] – 2017-02-19

### New features

- Add support for default values on input object fields
  ([#28](https://github.com/graphql-rust/juniper/issues/28))

# [0.6.2] – 2017-02-05

### New features

- The `null` literal is now supported in the GraphQL
  language. ([#26](https://github.com/graphql-rust/juniper/pull/26))
- Rustc-serialize is now optional, but enabled by default. If you
  _only_ want Serde support, include Juniper without default features
  and enable
  Serde. ([#12](https://github.com/graphql-rust/juniper/pull/12))
- The built-in `ID` type now has a public constructor and derives a
  few traits (`Clone`, `Debug`, `Eq`, `PartialEq`, `From<String>`,
  `Deref<Target=str>`). ([#19](https://github.com/graphql-rust/juniper/pull/19))
- Juniper is now built and tested against all Rust compilers since
  version 1.12.1.

### Minor breaking change

- Serde has been updated to
  0.9. ([#25](https://github.com/graphql-rust/juniper/pull/25))

### Bugfixes

- The built-in GraphiQL handler had a bug in variable serialization.
  ([#16](https://github.com/graphql-rust/juniper/pull/16))
- The example should now build and run without problems on
  Windows. ([#15](https://github.com/graphql-rust/juniper/pull/15))
- Object types now properly implement
  `__typename`. ([#22](https://github.com/graphql-rust/juniper/pull/22))
- String variables are now properly parsed into GraphQL
  enums. ([#17](https://github.com/graphql-rust/juniper/pull/17))

# [0.6.1] – 2017-01-06

### New features

- Optional Serde support
  ([#8](https://github.com/graphql-rust/juniper/pull/8))

### Improvements

- The `graphql_input_object!` macro can now be used to define input
  objects as public Rust structs.
- GraphiQL in the Iron GraphiQL handler has been updated to 0.8.1
  (#[#11](https://github.com/graphql-rust/juniper/pull/11))

### Minor breaking changes

Some undocumented but public APIs were changed.

- `to_snake_case` correctly renamed to `to_camel_case`
  ([#9](https://github.com/graphql-rust/juniper/pull/9))
- JSON serialization of `GraphQLError` changed to be more consistent
  with how other values were serialized
  ([#10](https://github.com/graphql-rust/juniper/pull/10)).

# [0.6.0] – 2017-01-02

TL;DR: Many big changes in how context types work and how they
interact with the executor. Not too much to worry about if you're only
using the macros and not deriving `GraphQLType` directly.

### Breaking changes

- The `executor` argument in all resolver methods is now
  immutable. The executor instead uses interior mutability to store
  errors in a thread-safe manner.

  This change could open up for asynchronous or multi-threaded
  execution: you can today use something like rayon in your resolve
  methods to let child nodes be concurrently resolved.

  **How to fix:** All field resolvers that looked like `field name(&mut executor` now should say `field name(&executor`.

- The context type of `GraphQLType` is moved to an associated type;
  meaning it's no longer generic. This only affects people who
  implement the trait manually, _not_ macro users.

  This greatly simplifies a lot of code by ensuring that there only
  can be one `GraphQLType` implementation for any given Rust
  type. However, it has the downside that support for generic contexts
  previously used in scalars, has been removed. Instead, use the new
  context conversion features to accomplish the same task.

  **How to fix:** Instead of `impl GraphQLType<MyContext> for ...`,
  you use `impl GraphQLType for ... { type Context = MyContext;`.

- All context types must derive the `Context` marker trait. This is
  part of an overarching change to allow different types to use
  different contexts.

  **How to fix:** If you have written e.g. `graphql_object!(MyType: MyContext ...)` you will need to add `impl Context for MyContext {}`. Simple as that.

- `Registry` and all meta type structs now takes one lifetime
  parameter, which affects `GraphQLType`'s `meta` method. This only
  affects people who implement the trait manually.

  **How to fix:** Change the type signature of `meta()` to read `fn meta<'r>(registry: &mut Registry<'r>) -> MetaType<'r>`.

- The type builder methods on `Registry` no longer return functions
  taking types or fields. Due to how the borrow checker works with
  expressions, you will have to split up the instantiation into two
  statements. This only affects people who implement the `GraphQLType`
  trait manually.

  **How to fix:** Change the contents of your `meta()` methods to
  something like this:

  ```rust
  fn meta<'r>(registry: &mut Registry<r>) -> MetaType<'r> {
      let fields = &[ /* your fields ... */ ];

      registry.build_object_type::<Self>(fields).into_meta()
  }
  ```

### Added

- Support for different contexts for different types. As GraphQL
  schemas tend to get large, narrowing down the context type to
  exactly what a given type needs is great for
  encapsulation. Similarly, letting different subsystems use different
  resources thorugh the context is also useful for the same reasons.

  Juniper supports two different methods of doing this, depending on
  your needs: if you have two contexts where one can be converted into
  the other _without any extra knowledge_, you can implement the new
  `FromContext` trait. This is useful if you have multiple crates or
  modules that all belong to the same GraphQL schema:

  ```rust
  struct TopContext {
    db: DatabaseConnection,
    session: WebSession,
    current_user: User,
  }

  struct ModuleOneContext {
    db: DatabaseConnection, // This module only requires a database connection
  }

  impl Context for TopContext {}
  impl Context for ModuleOneContext {}

  impl FromContext<TopContext> for ModuleOneContext {
    fn from(ctx: &TopContext) -> ModuleOneContext {
      ModuleOneContext {
        db: ctx.db.clone()
      }
    }
  }

  graphql_object!(Query: TopContext |&self| {
    field item(&executor) -> Item {
      executor.context().db.get_item()
    }
  });

  // The `Item` type uses another context type - conversion is automatic
  graphql_object!(Item: ModuleOneContext |&self| {
    // ...
  });
  ```

  The other way is to manually perform the conversion in a field
  resolver. This method is preferred when the child context needs
  extra knowledge than what exists in the parent context:

  ```rust
  // Each entity has its own context
  struct TopContext {
    entities: HashMap<i32, EntityContext>,
    db: DatabaseConnection,
  }

  struct EntityContext {
    // fields
  }

  impl Context for TopContext {}
  impl Context for EntityContext {}

  graphql_object!(Query: TopContext |&self| {
    // By returning a tuple (&Context, GraphQLType), you can tell the executor
    // to switch out the context for the returned value. You can wrap the
    // tuple in Option<>, FieldResult<>, FieldResult<Option<>>, or just return
    // the tuple without wrapping it.
    field entity(&executor, key: i32) -> Option<(&EntityContext, Entity)> {
      executor.context().entities.get(&key)
        .map(|ctx| (ctx, executor.context().db.get_entity(key)))
    }
  });

  graphql_object!(Entity: EntityContext |&self| {
    // ...
  });
  ```

### Improvements

- Parser and query execution has now reduced the allocation overhead
  by reusing as much as possible from the query source and meta type
  information.

# [0.5.3] – 2016-12-05

### Added

- `jtry!`: Helper macro to produce `FieldResult`s from regular
  `Result`s. Wherever you would be using `try!` in a regular function
  or method, you can use `jtry!` in a field resolver:

  ```rust
  graphql_object(MyType: Database |&self| {
    field count(&executor) -> FieldResult<i32> {
      let txn = jtry!(executor.context().transaction());

      let count = jtry!(txn.execute("SELECT COUNT(*) FROM user"));

      Ok(count[0][0])
    }
  });
  ```

### Changes

- Relax context type trait requirements for the iron handler: your
  contexts no longer have to be `Send + Sync`.

- `RootNode` is now `Send` and `Sync` if both the mutation and query
  types implement `Send` and `Sync`.

### Bugfixes

- `return` statements inside field resolvers no longer cause syntax
  errors.

## 0.5.2 – 2016-11-13

### Added

- Support for marking fields and enum values deprecated.
- `input_object!` helper macro

### Changes

- The included example server now uses the simple Star Wars schema
  used in query/introspection tests.

### Bugfixes

- The query validators - particularly ones concerned with validation
  of input data and variables - have been improved significantly. A
  large number of test cases have been added.

- Macro syntax stability has also been improved. All syntactical edge
  cases of the macros have gotten tests to verify their correctness.

[0.8.0]: https://github.com/graphql-rust/juniper/compare/0.8.0...0.8.1
[0.8.0]: https://github.com/graphql-rust/juniper/compare/0.7.0...0.8.0
[0.7.0]: https://github.com/graphql-rust/juniper/compare/0.6.3...0.7.0
[0.6.3]: https://github.com/graphql-rust/juniper/compare/0.6.2...0.6.3
[0.6.2]: https://github.com/graphql-rust/juniper/compare/0.6.1...0.6.2
[0.6.1]: https://github.com/graphql-rust/juniper/compare/0.6.0...0.6.1
[0.6.0]: https://github.com/graphql-rust/juniper/compare/0.5.3...0.6.0
[0.5.3]: https://github.com/graphql-rust/juniper/compare/0.5.2...0.5.3
