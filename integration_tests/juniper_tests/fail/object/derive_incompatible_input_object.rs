#[derive(juniper::GraphQLInputObject)]
struct ObjectA {
    test: String,
}

#[derive(juniper::GraphQLObject)]
struct Object {
    field: ObjectA,
}

fn main() {}
