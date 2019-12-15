# Complex fields

If you've got a struct that can't be mapped directly to GraphQL, that contains
computed fields or circular structures, you have to use a more powerful tool:
the `object` procedural macro. This macro lets you define GraphQL object
fields in a Rust `impl` block for a type. Continuing with the
example from the last chapter, this is how you would define `Person` using the
macro:

```rust

struct Person {
    name: String,
    age: i32,
}

#[juniper::object]
impl Person {
    fn name(&self) -> &str {
        self.name.as_str()
    }

    fn age(&self) -> i32 {
        self.age
    }
}

// Note that this syntax generates an implementation of the GraphQLType trait,
// the base impl of your struct can still be written like usual:
impl Person {
    pub fn hidden_from_graphql(&self) {
        // [...]
    }
}

# fn main() { }
```

While this is a bit more verbose, it lets you write any kind of function in the
field resolver. With this syntax, fields can also take arguments:


```rust
#[derive(juniper::GraphQLObject)]
struct Person {
    name: String,
    age: i32,
}

struct House {
    inhabitants: Vec<Person>,
}

#[juniper::object]
impl House {
    // Creates the field inhabitantWithName(name), returning a nullable person
    fn inhabitant_with_name(&self, name: String) -> Option<&Person> {
        self.inhabitants.iter().find(|p| p.name == name)
    }
}

# fn main() {}
```

To access global data such as database connections or authentication
information, a _context_ is used. To learn more about this, see the next
chapter: [Using contexts](using_contexts.md).

## Description, renaming, and deprecation

Like with the derive attribute, field names will be converted from `snake_case`
to `camelCase`. If you need to override the conversion, you can simply rename
the field. Also, the type name can be changed with an alias:

```rust

struct Person {
}

/// Doc comments are used as descriptions for GraphQL.
#[juniper::object(
    // With this attribtue you can change the public GraphQL name of the type.
    name = "PersonObject",
    // You can also specify a description here, which will overwrite 
    // a doc comment description.
    description = "...",
)]
impl Person {

    /// A doc comment on the field will also be used for GraphQL.
    #[graphql(
        // Or provide a description here.
        description = "...",
    )]
    fn doc_comment(&self) -> &str {
        ""
    }

    // Fields can also be renamed if required.
    #[graphql(
        name = "myCustomFieldName",
    )]
    fn renamed_field() -> bool {
        true
    }

    // Deprecations also work as you'd expect.
    // Both the standard Rust syntax and a custom attribute is accepted.
    #[deprecated(note = "...")]
    fn deprecated_standard() -> bool {
        false
    }

    #[graphql(deprecated = "...")]
    fn deprecated_graphql() -> bool {
        true
    }
}

# fn main() { }
```

## Customizing arguments

Method field arguments can also be customized.

They can have custom descriptions and default values.

**Note**: The syntax for this is currently a little awkward. 
This will become better once the [Rust RFC 2565](https://github.com/rust-lang/rust/issues/60406) is implemented.

```rust

struct Person {}

#[juniper::object]
impl Person {
    fn field1(
        &self,
        #[graphql(default = true, description = "The first argument...")] arg1: bool,
        #[graphql(default = 0)] arg2: i32,
    ) -> String {
        format!("{} {}", arg1, arg2)
    }
}

# fn main() { }
```

## More features

GraphQL fields expose more features than Rust's standard method syntax gives us:

* Per-field description and deprecation messages
* Per-argument default values
* Per-argument descriptions

These, and more features, are described more thorougly in [the reference
documentation](https://docs.rs/juniper/latest/juniper/macro.object.html).
