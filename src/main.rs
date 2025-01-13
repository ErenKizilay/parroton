mod har_resolver;
mod http;
mod api;
mod proxy;
mod auth;
mod assertion;
mod run;
mod case;
mod parameter;
mod action_execution;
mod action;
mod persistence;
mod json_path;

use crate::api::build_api;

#[tokio::main]
async fn main() {
    println!("Hello, world!");
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    let router = build_api().await;
    axum::serve(listener, router).await.unwrap();
}
