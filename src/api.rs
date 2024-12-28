use crate::http::ApiClient;
use crate::persistence::repo::Repository;
use crate::routes::{auto_complete, delete_test_case, get_action_executions, get_run, get_test_case, list_actions, list_assertions, list_auth_providers, list_parameters, list_runs, list_test_cases, run_test_case, update_parameter_expression, upload, upload_test_case};
use axum::body::Body;
use axum::extract::{DefaultBodyLimit, FromRef};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, patch, post};
use axum::Router;
use serde::{Deserialize, Serialize};
use std::ops::Deref;
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::{DefaultMakeSpan, DefaultOnRequest, DefaultOnResponse, TraceLayer};
use tower_http::LatencyUnit;
use tracing::Level;

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

pub async fn build_api() -> Router {
    tracing_subscriber::fmt::init();
    let repository = Repository::new().await;

    let cors = CorsLayer::new()
        .allow_origin(Any) // Allow all origins (not recommended for production)
        .allow_methods(Any) // Allow specific HTTP methods
        .allow_headers(Any); // Allow specific headers


    let app_state = AppState {
        repository: Arc::new(repository),
        api_client: Arc::new(ApiClient::new()),
    };

    Router::new()
        .route("/test-cases/:test_case_id/actions/:action_id/parameters/:id/expression", patch(update_parameter_expression))
        .route("/test-cases/:test_case_id/actions/:id/parameters", get(list_parameters))
        .route("/test-cases/:test_case_id/actions", get(list_actions))
        .route("/test-cases/:id/runs/:run_id/action-executions", get(get_action_executions))
        .route("/test-cases/:id/runs/:run_id", get(get_run))
        .route("/test-cases/:id/run", post(run_test_case))
        .route("/test-cases/:id/runs", get(list_runs))
        .route("/test-cases/:id/assertions", get(list_assertions))
        .route("/test-cases/:id", get(get_test_case).delete(delete_test_case))
        .route("/test-cases", get(list_test_cases).post(upload_test_case))
        .route("/auth-providers", get(list_auth_providers))
        .route("/auto-complete", post(auto_complete))
        .route("/upload", post(upload))
        .layer(cors)
        .layer(DefaultBodyLimit::max(5003944))
        .layer(TraceLayer::new_for_http()
            .make_span_with(
                DefaultMakeSpan::new().include_headers(true))
            .on_request(
                DefaultOnRequest::new()
                    .level(Level::INFO))
            .on_response(
                DefaultOnResponse::new()
                    .level(Level::INFO)
                    .latency_unit(LatencyUnit::Micros)
            ))
        .with_state(app_state)
}
pub struct ApiResponse<T>(pub T);

impl<T> ApiResponse<T> {
    pub fn from(result: Result<T, AppError>) -> Result<ApiResponse<T>, AppError> {
        match result {
            Ok(t) => { Ok(ApiResponse(t)) }
            Err(e) => { Err(e) }
        }
    }
    pub fn from_option(result: Result<Option<T>, AppError>) -> Result<ApiResponse<T>, AppError> {
        match result {
            Ok(t) => {
                match t {
                    None => {
                        Err(AppError::NotFound("Not found".to_string()))
                    }
                    Some(val) => {
                        Ok(ApiResponse(val))
                    }
                }
            }
            Err(e) => { Err(e) }
        }
    }
}

impl<T> IntoResponse for ApiResponse<T>
where
    T: Serialize,
{
    fn into_response(self) -> Response {
        match serde_json::to_string(&self.0) {
            Ok(json) => Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "application/json")
                .body(json.into())
                .unwrap(),
            Err(_) => Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body("Failed to serialize response".into())
                .unwrap(),
        }
    }
}

#[derive(Debug)]
pub enum AppError {
    NotFound(String),
    Validation(String),
    Processing(String),
    Internal(String),
}

#[derive(Deserialize, Serialize, Clone)]
pub struct ErrorBody {
    pub message: String,
}

impl Into<Body> for ErrorBody {
    fn into(self) -> Body {
        Body::from(serde_json::to_string(&self).unwrap())
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        match self {
            AppError::NotFound(message) => {
                Response::builder()
                    .status(404)
                    .header("Content-Type", "application/json")
                    .body(ErrorBody { message }.into())
                    .unwrap()
            }
            AppError::Validation(message) => {
                Response::builder()
                    .status(400)
                    .header("Content-Type", "application/json")
                    .body(ErrorBody { message }.into())
                    .unwrap()
            }
            AppError::Processing(message) => {
                Response::builder()
                    .status(422)
                    .header("Content-Type", "application/json")
                    .body(ErrorBody { message }.into())
                    .unwrap()
            }
            AppError::Internal(message) => {
                tracing::error!("{}", message);
                Response::builder()
                    .status(500)
                    .header("Content-Type", "application/json")
                    .body(ErrorBody { message: "Internal server error".to_string() }.into())
                    .unwrap()
            }
        }
    }
}