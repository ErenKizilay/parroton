mod har_resolver;
mod http;
mod models;
mod repo;
mod routes;
mod json_path_utils;
mod execution;
mod testing;
mod assertions;

use std::ops::Deref;
use crate::repo::Repository;
use crate::routes::{auto_complete, get_action_executions, get_run, get_test_case, list_actions, list_auth_providers, list_parameters, list_runs, list_test_cases, run_test_case, upload, upload_test_case};
use axum::routing::{get, post};
use axum::Router;
use std::sync::Arc;
use axum::extract::{DefaultBodyLimit, FromRef};
use har::Har;
use tower_http::cors::{CorsLayer, Any};
use crate::har_resolver::build_test_case;
use crate::http::ApiClient;
use crate::testing::create_test_case;

#[tokio::main]
async fn main() {
    println!("Hello, world!");
    let repository = Repository::new().await;

    let cors = CorsLayer::new()
        .allow_origin(Any) // Allow all origins (not recommended for production)
        .allow_methods(Any) // Allow specific HTTP methods
        .allow_headers(Any); // Allow specific headers


    let app_state = AppState {
        repository: Arc::new(repository),
        api_client: Arc::new(ApiClient::new(Default::default())),
    };

    let app = Router::new()
        .route("/test-cases/:test_case_id/actions/:id/parameters", get(list_parameters))
        .route("/test-cases/:test_case_id/actions", get(list_actions))
        .route("/test-cases/:id/runs/:run_id/action-executions", get(get_action_executions))
        .route("/test-cases/:id/runs/:run_id", get(get_run))
        .route("/test-cases/:id/run", post(run_test_case))
        .route("/test-cases/:id/runs", get(list_runs))
        .route("/test-cases/:id", get(get_test_case))
        .route("/test-cases", get(list_test_cases).post(upload_test_case))
        .route("/auth-providers", get(list_auth_providers))
        .route("/auto-complete", post(auto_complete))
        .route("/upload", post(upload))
        .layer(cors)
        .layer(DefaultBodyLimit::max(5003944))
        .with_state(app_state);

    // run our app with hyper, listening globally on port 3000
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

#[derive(Clone)]
pub struct AppState {
    pub repository: Arc<Repository>,
    pub api_client: Arc<ApiClient>,
}

// support converting an `AppState` in an `ApiState`
impl FromRef<AppState> for Repository {
    fn from_ref(app_state: &AppState) -> Repository {
        app_state.repository.deref().clone()
    }
}
