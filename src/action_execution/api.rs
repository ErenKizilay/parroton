use axum::extract::{Path, State};
use crate::action_execution::model::ActionExecutionPair;
use crate::api::{ApiResponse, AppError, AppState};

pub async fn get_action_executions(
    Path(path_params): Path<(String, String)>,
    State(app_state): State<AppState>,
) -> Result<ApiResponse<Vec<ActionExecutionPair>>, AppError> {
    let result = app_state
        .repository
        .action_executions()
        .list_with_actions(&"eren".to_string(), &path_params.0, &path_params.1)
        .await;
    ApiResponse::from(result)
}