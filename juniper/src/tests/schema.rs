#![allow(missing_docs)]

use crate::{
    executor::Context,
    tests::model::{Character, Database, Droid, Episode, Human},
};

impl Context for Database {}

graphql_interface!(<'a> &'a dyn Character: Database as "Character" |&self| {
    description: "A character in the Star Wars Trilogy"

    field id() -> &str as "The id of the character" {
        self.id()
    }

    field name() -> Option<&str> as "The name of the character" {
        Some(self.name())
    }

    field friends(&executor) -> Vec<&dyn Character>
    as "The friends of the character" {
        executor.context().get_friends(self.as_character())
    }

    field appears_in() -> &[Episode] as "Which movies they appear in" {
        self.appears_in()
    }

    instance_resolvers: |&context| {
        &dyn Human => context.get_human(&self.id()),
        &dyn Droid => context.get_droid(&self.id()),
    }
});

#[crate::object_internal(
    Context = Database,
    Scalar = crate::DefaultScalarValue,
    interfaces = [&dyn Character],
)]
/// A humanoid creature in the Star Wars universe.
impl<'a> &'a dyn Human {
    /// The id of the human
    fn id(&self) -> &str {
        self.id()
    }

    /// The name of the human
    fn name(&self) -> Option<&str> {
        Some(self.name())
    }

    /// The friends of the human
    fn friends(&self, ctx: &Database) -> Vec<&dyn Character> {
        ctx.get_friends(self.as_character())
    }

    /// Which movies they appear in
    fn appears_in(&self) -> &[Episode] {
        self.appears_in()
    }

    /// The home planet of the human
    fn home_planet(&self) -> &Option<String> {
        self.home_planet()
    }
}

#[crate::object_internal(
    Context = Database,
    Scalar = crate::DefaultScalarValue,
    interfaces = [&dyn Character],
)]
/// A mechanical creature in the Star Wars universe.
impl<'a> &'a dyn Droid {
    /// The id of the droid
    fn id(&self) -> &str {
        self.id()
    }

    /// The name of the droid
    fn name(&self) -> Option<&str> {
        Some(self.name())
    }

    /// The friends of the droid
    fn friends(&self, ctx: &Database) -> Vec<&dyn Character> {
        ctx.get_friends(self.as_character())
    }

    /// Which movies they appear in
    fn appears_in(&self) -> &[Episode] {
        self.appears_in()
    }

    /// The primary function of the droid
    fn primary_function(&self) -> &Option<String> {
        self.primary_function()
    }
}

pub struct Query;

#[crate::object_internal(
    Context = Database,
    Scalar = crate::DefaultScalarValue,
)]
/// The root query object of the schema
impl Query {
    fn human(
        database: &Database,
        #[graphql(description = "id of the human")] id: String,
    ) -> Option<&dyn Human> {
        database.get_human(&id)
    }

    fn droid(
        database: &Database,
        #[graphql(description = "id of the droid")] id: String,
    ) -> Option<&dyn Droid> {
        database.get_droid(&id)
    }

    fn hero(
        database: &Database,
        #[graphql(
            description = "If omitted, returns the hero of the whole saga. If provided, returns the hero of that particular episode"
        )]
        episode: Option<Episode>,
    ) -> Option<&dyn Character> {
        Some(database.get_hero(episode).as_character())
    }
}
