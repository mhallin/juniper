#![deny(warnings)]

use std::env;

use juniper::{
    tests::fixtures::starwars::schema::{Database, Query},
    EmptyMutation, EmptySubscription, RootNode,
};
use tokio::task::LocalSet;
use warp::{http::Response, Filter};

type Schema = RootNode<'static, Query, EmptyMutation<Database>, EmptySubscription<Database>>;

fn schema() -> Schema {
    Schema::new(
        Query,
        EmptyMutation::<Database>::new(),
        EmptySubscription::<Database>::new(),
    )
}

#[tokio::main]
async fn main() {
    env::set_var("RUST_LOG", "warp_server");
    env_logger::init();

    let log = warp::log("warp_server");

    let homepage = warp::path::end().map(|| {
        Response::builder()
            .header("content-type", "text/html")
            .body(format!(
                "<html><h1>juniper_warp</h1><div>visit <a href=\"/graphiql\">/graphiql</a></html>"
            ))
    });

    log::info!("Listening on 127.0.0.1:8080");

    let state = warp::any().map(move || Database::new());
    let graphql_filter = juniper_warp::make_graphql_filter(schema(), state.boxed());

    LocalSet::new()
        .run_until(
            warp::serve(
                warp::get()
                    .and(warp::path("graphiql"))
                    .and(juniper_warp::graphiql_filter("/graphql", None))
                    .or(homepage)
                    .or(warp::path("graphql").and(graphql_filter))
                    .with(log),
            )
            .run(([127, 0, 0, 1], 8080)),
        )
        .await
}
