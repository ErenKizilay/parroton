use crate::api::{ApiResponse, AppError, AppState};
use crate::execution::{run_test, RunTestCaseCommand};
use crate::har_resolver::build_test_case;
use crate::json_path_utils;
use crate::json_path_utils::AutoCompleteRequest;
use crate::models::{Action, ActionExecution, Assertion, AssertionItem, AuthenticationProvider, ComparisonType, Expression, Parameter, ParameterType, Run, TestCase};
use crate::persistence::repo::{ParameterIn, QueryResult, Repository};
use axum::extract::{Multipart, Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use har::Har;
use serde::Deserialize;
use std::io::Cursor;

pub async fn list_test_cases(State(repository): State<Repository>) -> Result<ApiResponse<QueryResult<TestCase>>, AppError> {
    let result = repository.test_cases().list("eren".to_string(), None).await;
    ApiResponse::from(result)
}

pub async fn list_auth_providers(
    params: Query<AuthProvidersQueryParams>,
    State(repository): State<Repository>,
) -> Result<ApiResponse<QueryResult<AuthenticationProvider>>, AppError> {
    let result = repository.auth_providers()
        .list(&"eren".to_string(), params.test_case_id.clone(), None)
        .await;
    ApiResponse::from(result)
}

pub async fn get_test_case(
    Path(id): Path<String>,
    State(repository): State<Repository>,
) -> Result<ApiResponse<TestCase>, AppError> {
    let result = repository.test_cases().get("eren".to_string(), id).await;
    ApiResponse::from_option(result)
}

pub async fn delete_test_case(
    Path(id): Path<String>,
    State(repository): State<Repository>,
) -> impl IntoResponse {
    repository.test_cases().delete(&"eren".to_string(), &id).await;
    StatusCode::NO_CONTENT
}

pub async fn run_test_case(
    Path(id): Path<String>,
    State(app_state): State<AppState>,
) -> Result<ApiResponse<Run>, AppError> {
    let result = run_test(
        app_state.repository,
        app_state.api_client,
        RunTestCaseCommand {
            customer_id: "eren".to_string(),
            test_case_id: id,
        },
    )
        .await;
    ApiResponse::from(result)
}

pub async fn get_run(
    Path(path_params): Path<(String, String)>,
    State(app_state): State<AppState>,
) -> Result<ApiResponse<Run>, AppError> {
    let result = app_state
        .repository.runs()
        .get(&"eren".to_string(), &path_params.0, &path_params.1)
        .await;

    ApiResponse::from_option(result)
}

pub async fn get_action_executions(
    Path(path_params): Path<(String, String)>,
    State(app_state): State<AppState>,
) -> Result<ApiResponse<Vec<ActionExecution>>, AppError> {
    let result = app_state
        .repository.action_executions()
        .list(&"eren".to_string(), &path_params.0, &path_params.1)
        .await;
    ApiResponse::from(result)
}

pub async fn list_runs(
    Path(test_case_id): Path<String>,
    State(app_state): State<AppState>,
) -> Result<ApiResponse<QueryResult<Run>>, AppError> {
    let result = app_state
        .repository.runs()
        .list(&"eren".to_string(), &test_case_id)
        .await;
    ApiResponse::from(result)
}

pub async fn list_assertions(
    Path(test_case_id): Path<String>,
    State(app_state): State<AppState>,
) -> Result<ApiResponse<QueryResult<Assertion>>, AppError> {
    let result = app_state
        .repository.assertions()
        .list(&"eren".to_string(), &test_case_id)
        .await;
    ApiResponse::from(result)
}

pub async fn list_actions(
    Path(test_case_id): Path<String>,
    params: Query<ActionQueryParams>,
    State(repository): State<Repository>,
) -> Result<ApiResponse<QueryResult<Action>>, AppError> {
    let result = match params.before_order {
        None => {
            repository.actions()
                .list("eren".to_string(), test_case_id.to_string(), None)
                .await
        }
        Some(order) => {
            repository.actions()
                .list_previous("eren".to_string(), test_case_id.to_string(), order, None)
                .await
        }
    };
    ApiResponse::from(result)
}

pub async fn list_parameters(
    Path(path_params): Path<(String, String)>,
    params: Query<ParameterQueryParams>,
    State(repository): State<Repository>,
) -> Result<ApiResponse<QueryResult<Parameter>>, AppError> {
    let test_case_id = path_params.0;
    let action_id = path_params.1;
    let parameter_type = &params.parameter_type;
    let result = match &params.path {
        None => {
            let option = &params.parameter_in;
            repository
                .parameters()
                .list_by_action(
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
                .parameters()
                .query_by_path(
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
    ApiResponse::from(result)
}

pub async fn update_parameter_expression(
    Path(path_params): Path<ParametersPathParam>,
    State(repository): State<Repository>,
    Json(expression): Json<Option<Expression>>,
) -> Result<ApiResponse<Parameter>, AppError> {
    let result = repository.parameters()
        .update_expression("eren".to_string(), path_params.test_case_id, path_params.action_id, path_params.id, expression).await;
    ApiResponse::from(result)
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

#[derive(Deserialize)]
pub struct ParametersPathParam {
    test_case_id: String,
    action_id: String,
    id: String,
}

#[derive(Deserialize, Clone)]
pub struct ParameterQueryParams {
    path: Option<String>,
    parameter_type: ParameterType,
    parameter_in: Option<ParameterIn>,
}

#[derive(Deserialize, Clone)]
pub struct AssertionRequest {
    pub left: AssertionItem,
    pub right: AssertionItem,
    pub comparison_type: ComparisonType,
    pub negate: bool,
}
