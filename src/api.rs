use crate::action::api::list_actions;
use crate::action_execution::api::get_action_executions;
use crate::assertion::api::{batch_get_assertions, delete_assertion, get_assertion, list_assertions, put_assertion, update_assertion_comparison, update_assertion_expression, update_assertion_negation};
use crate::auth::api::{add_auth_header_value, create_auth_provider, delete_auth_provider, get_auth_provider, list_auth_providers, list_auth_providers_with_multiple_urls, set_auth_header_enablement, set_auth_header_value};
use crate::case::api::{delete_test_case, filter_paths, get_test_case, list_test_cases, update_test_case, update_test_case_description, update_test_case_name, upload_test_case};
use crate::http::ApiClient;
use crate::json_path::api::auto_complete;
use crate::parameter::api::{list_parameters, update_parameter_expression};
use crate::persistence::repo::Repository;
use crate::run::api::{get_run, list_runs, run_test_case};
use axum::body::Body;
use axum::extract::{DefaultBodyLimit, FromRef};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, patch, post};
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
        .route("/test-cases/:test_case_id/assertions/:id/:location/expression", patch(update_assertion_expression))
        .route("/test-cases/:test_case_id/assertions/:id/comparison-type", patch(update_assertion_comparison))
        .route("/test-cases/:test_case_id/assertions/:id/negate", patch(update_assertion_negation))
        .route("/test-cases/:test_case_id/assertions/:id", get(get_assertion).delete(delete_assertion))
        .route("/test-cases/:id/assertions/batch-get", post(batch_get_assertions))
        .route("/test-cases/:id/assertions", get(list_assertions).put(put_assertion))
        .route("/test-cases/:id/name", patch(update_test_case_name))
        .route("/test-cases/:id/description", patch(update_test_case_description))
        .route("/test-cases/:id", get(get_test_case).delete(delete_test_case).patch(update_test_case))
        .route("/auth-providers/:id", delete(delete_auth_provider).get(get_auth_provider))
        .route("/auth-providers/:id/headers", patch(add_auth_header_value))
        .route("/auth-providers/:id/value", patch(set_auth_header_value))
        .route("/auth-providers/:id/disabled", patch(set_auth_header_enablement))
        .route("/auth-providers", post(create_auth_provider))
        .route("/test-cases", get(list_test_cases).post(upload_test_case))
        .route("/auth-providers/search-by-urls", post(list_auth_providers_with_multiple_urls))
        .route("/auth-providers", get(list_auth_providers))
        .route("/auto-complete", post(auto_complete))
        .route("/filter-paths", post(filter_paths))
        .layer(cors)
        .layer(DefaultBodyLimit::max(90003944))
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
                //tracing::error!("{}", message);
                Response::builder()
                    .status(500)
                    .header("Content-Type", "application/json")
                    .body(ErrorBody { message: "Internal server error".to_string() }.into())
                    .unwrap()
            }
        }
    }
}