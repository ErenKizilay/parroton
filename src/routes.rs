use crate::execution::{run_test, RunTestCaseCommand};
use crate::har_resolver::build_test_case;
use crate::json_path_utils::AutoCompleteRequest;
use crate::models::ParameterType;
use crate::repo::{ParameterIn, Repository};
use crate::{json_path_utils, AppState};
use axum::extract::{Multipart, Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use har::Har;
use serde::Deserialize;
use serde_json::Value;
use std::io::Cursor;

// The query parameters for todos index
#[derive(Debug, Deserialize, Default)]
pub struct Pagination {
    pub offset: Option<usize>,
    pub limit: Option<usize>,
}

pub async fn list_test_cases(State(repository): State<Repository>) -> impl IntoResponse {
    let result = repository.list_test_cases("eren".to_string(), None).await;
    (StatusCode::OK, Json(result.items))
}

pub async fn list_auth_providers(
    params: Query<AuthProvidersQueryParams>,
    State(repository): State<Repository>,
) -> impl IntoResponse {
    let result = repository
        .list_auth_providers(&"eren".to_string(), params.test_case_id.clone(), None)
        .await;
    (StatusCode::OK, Json(result))
}

pub async fn get_test_case(
    Path(id): Path<String>,
    State(repository): State<Repository>,
) -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(repository.get_test_case("eren".to_string(), id).await),
    )
}

pub async fn run_test_case(
    Path(id): Path<String>,
    State(app_state): State<AppState>,
) -> impl IntoResponse {
    let result = run_test(
        app_state.repository,
        app_state.api_client,
        RunTestCaseCommand {
            customer_id: "eren".to_string(),
            test_case_id: id,
        },
    )
    .await;
    match result {
        Ok(run) => (StatusCode::OK, Json(serde_json::json!(run))),
        Err(err) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(Value::String(err.to_string())),
        ),
    }
}

pub async fn get_run(
    Path(path_params): Path<(String, String)>,
    State(app_state): State<AppState>,
) -> impl IntoResponse {
    let result = app_state
        .repository
        .get_run(&"eren".to_string(), &path_params.0, &path_params.1)
        .await;

    match result {
        None => (
            StatusCode::NOT_FOUND,
            Json(Value::String("run does not exist".to_string())),
        ),
        Some(run) => (StatusCode::OK, Json(serde_json::json!(run))),
    }
}

pub async fn get_action_executions(
    Path(path_params): Path<(String, String)>,
    State(app_state): State<AppState>,
) -> impl IntoResponse {
    let executions = app_state
        .repository
        .get_action_executions(&"eren".to_string(), &path_params.0, &path_params.1)
        .await;
    (StatusCode::OK, Json(executions))
}

pub async fn list_runs(
    Path(test_case_id): Path<String>,
    State(app_state): State<AppState>,
) -> impl IntoResponse {
    let executions = app_state
        .repository
        .list_runs(&"eren".to_string(), &test_case_id)
        .await.items;
    (StatusCode::OK, Json(executions))
}

pub async fn list_actions(
    Path(test_case_id): Path<String>,
    params: Query<ActionQueryParams>,
    State(repository): State<Repository>,
) -> impl IntoResponse {
    let mut result = match params.before_order {
        None => {
            repository
                .list_actions("eren".to_string(), test_case_id.to_string(), None)
                .await
        }
        Some(order) => {
            repository
                .list_previous_actions("eren".to_string(), test_case_id.to_string(), order, None)
                .await
        }
    };
    result.items.sort();
    (StatusCode::OK, Json(result.items))
}

pub async fn list_parameters(
    Path(path_params): Path<(String, String)>,
    params: Query<ParameterQueryParams>,
    State(repository): State<Repository>,
) -> impl IntoResponse {
    let test_case_id = path_params.0;
    let action_id = path_params.1;
    let parameter_type = &params.parameter_type;
    let result = match &params.path {
        None => {
            let option = &params.parameter_in;
            repository
                .list_parameters_of_action(
                    "eren".to_string(),
                    test_case_id.to_string(),
                    action_id.to_string(),
                    parameter_type.clone(),
                    option.clone(),
                    None,
                )
                .await
        }
        Some(path) => {
            repository
                .query_parameters_of_action_by_path(
                    "eren".to_string(),
                    test_case_id.to_string(),
                    action_id.to_string(),
                    parameter_type.clone(),
                    path.clone(),
                    None,
                )
                .await
        }
    };
    (StatusCode::OK, Json(result.items))
}

pub async fn auto_complete(
    State(repository): State<Repository>,
    Json(auto_complete_request): Json<AutoCompleteRequest>,
) -> impl IntoResponse {
    let result = json_path_utils::auto_complete(&repository, auto_complete_request).await;
    Json(result)
}

pub async fn upload_test_case(
    State(repository): State<Repository>,
    mut multipart: Multipart,
) -> impl IntoResponse {
    let mut provided_har: Option<Har> = None;
    let mut provided_name: String = "".to_string();
    let mut provided_description: String = "".to_string();
    let mut provided_excluded_path_parts: Vec<String> = vec![];
    while let Some(mut field) = multipart.next_field().await.unwrap() {
        let name = field.name().unwrap().to_string();
        match name.as_str() {
            "name" => {
                provided_name = field.text().await.unwrap();
            }
            "description" => {
                provided_description = field.text().await.unwrap();
            }
            "excluded_paths" => {
                provided_excluded_path_parts = field
                    .text()
                    .await
                    .unwrap()
                    .split(",")
                    .map(|s| s.to_string())
                    .collect();
            }
            "file" => {
                let data = field.bytes().await.unwrap();
                provided_har = Some(har::from_reader(Cursor::new(data)).unwrap());
            }
            _ => {}
        }
    }

    match provided_har {
        Some(har) => {
            build_test_case(
                &repository,
                &har.log,
                &"eren".to_string(),
                &provided_name,
                &provided_description,
                provided_excluded_path_parts.clone(),
            )
            .await;
        }
        None => {}
    }
}

pub async fn upload(mut multipart: Multipart) {
    while let Some(mut field) = multipart.next_field().await.unwrap() {
        let name = field.name().unwrap().to_string();
        let data = field.bytes().await.unwrap();
        println!("Length of `{}` is {} bytes", name, data.len());
    }
}

#[derive(Deserialize)]
pub struct TestCaseForm {
    pub name: String,
    pub description: String,
    pub excluded_endpoint_parts: Vec<String>,
}

#[derive(Deserialize)]
pub struct ActionQueryParams {
    before_order: Option<usize>,
}

#[derive(Deserialize)]
pub struct AuthProvidersQueryParams {
    test_case_id: Option<String>,
}

#[derive(Deserialize, Clone)]
pub struct ParameterQueryParams {
    path: Option<String>,
    parameter_type: ParameterType,
    parameter_in: Option<ParameterIn>,
}
