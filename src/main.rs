mod har_resolver;
mod http;
mod models;
mod repo;
mod routes;
mod testing;

use crate::repo::Repository;
use crate::routes::{get_test_case, list_actions, list_parameters, list_test_cases};
use axum::routing::get;
use axum::Router;
use std::sync::Arc;
use crate::har_resolver::build_test_case;

#[tokio::main]
async fn main() {
    println!("Hello, world!");
    let repository = Repository::new().await;
    /*let har = har::from_path("resources/tester22.app.opsgenie.com.har").unwrap();
    build_test_case(
        &repository,
        &har.log,
        &"eren".to_string(),
        &"create alert".to_string(),
    )
        .await;*/

    let app = Router::new()
        .route("/test-cases/:test_case_id/actions/:id/parameters", get(list_parameters))
        .route("/test-cases/:test_case_id/actions", get(list_actions))
        .route("/test-cases/:id", get(get_test_case))
        .route("/test-cases", get(list_test_cases))
        .with_state(Arc::new(repository.clone()));

    // run our app with hyper, listening globally on port 3000
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
