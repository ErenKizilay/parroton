use axum::extract::{Path, Query, State};
use serde::Deserialize;
use crate::action::model::Action;
use crate::api::{ApiResponse, AppError};
use crate::persistence::repo::{QueryResult, Repository};

pub async fn list_actions(
    Path(test_case_id): Path<String>,
    params: Query<ActionQueryParams>,
    State(repository): State<Repository>,
) -> Result<ApiResponse<QueryResult<Action>>, AppError> {
    let result = match params.before_order {
        None => {
            repository
                .actions()
                .list("eren".to_string(), test_case_id.to_string(), None)
                .await
        }
        Some(order) => {
            repository
                .actions()
                .list_previous("eren".to_string(), test_case_id.to_string(), order, None)
                .await
        }
    };
    ApiResponse::from(result)
}
#[derive(Deserialize)]
pub struct ActionQueryParams {
    before_order: Option<usize>,
}