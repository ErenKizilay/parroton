mod har_resolver;
mod http;
mod models;
mod routes;
mod json_path_utils;
mod execution;
mod assertions;
mod persistence;
mod api;
mod proxy;

use crate::api::build_api;

#[tokio::main]
async fn main() {
    println!("Hello, world!");
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    let router = build_api().await;
    axum::serve(listener, router).await.unwrap();
}
