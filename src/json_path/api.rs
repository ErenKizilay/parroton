use crate::json_path::utils;
use crate::persistence::repo::Repository;
use axum::extract::State;
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;

pub async fn auto_complete(
    State(repository): State<Repository>,
    Json(auto_complete_request): Json<AutoCompleteRequest>,
) -> impl IntoResponse {
    let result = utils::auto_complete(&repository, auto_complete_request).await;
    Json(result)
}

#[derive(Deserialize)]
pub struct AutoCompleteRequest {
    pub customer_id: String,
    pub test_case_id: String,
    pub source_action_order: Option<usize>,
    pub latest_input: String,
}