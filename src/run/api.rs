use crate::api::{ApiResponse, AppError, AppState};
use crate::persistence::model::QueryResult;
use crate::run::execution::{run_test, RunTestCaseCommand};
use crate::run::model::Run;
use axum::extract::{Path, State};

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
        .repository
        .runs()
        .get(&"eren".to_string(), &path_params.0, &path_params.1)
        .await;

    ApiResponse::from_option(result)
}

pub async fn list_runs(
    Path(test_case_id): Path<String>,
    State(app_state): State<AppState>,
) -> Result<ApiResponse<QueryResult<Run>>, AppError> {
    let result = app_state
        .repository
        .runs()
        .list(&"eren".to_string(), &test_case_id)
        .await;
    ApiResponse::from(result)
}