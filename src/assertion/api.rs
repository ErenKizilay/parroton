use axum::extract::{Path, State};
use axum::Json;
use serde::Deserialize;
use uuid::Uuid;
use crate::api::{ApiResponse, AppError, AppState};
use crate::assertion::model::{Assertion, AssertionItem, ComparisonType};
use crate::persistence::repo::{QueryResult, Repository};

pub async fn delete_assertion(
    State(repository): State<Repository>,
    Path(params): Path<AssertionsPathParam>,
) -> Result<ApiResponse<Option<Assertion>>, AppError>{
    let result = repository.assertions()
        .delete("eren".to_string(), params.test_case_id, params.id).await;
    ApiResponse::from(result)
}

pub async fn get_assertion(
    Path((test_case_id, id)): Path<(String, String)>,
    State(repository): State<Repository>,
) -> Result<ApiResponse<Option<Assertion>>, AppError>{
    let result = repository.assertions()
        .get("eren".to_string(), test_case_id, id).await;
    ApiResponse::from(result)
}

pub async fn update_assertion_comparison(
    Path((test_case_id, id)): Path<(String, String)>,
    State(repository): State<Repository>,
    Json(payload): Json<PatchAssertionComparisonType>,
) -> Result<ApiResponse<Assertion>, AppError>{
    let result = repository.assertions()
        .update_comparison_type("eren".to_string(), test_case_id, id, payload.value)
        .await;
    ApiResponse::from(result)
}

pub async fn update_assertion_negation(
    Path((test_case_id, id)): Path<(String, String)>,
    State(repository): State<Repository>,
    Json(payload): Json<PatchAssertionNegation>,
) -> Result<ApiResponse<Assertion>, AppError>{
    let result = repository.assertions()
        .update_comparison_negation("eren".to_string(), test_case_id, id, payload.value)
        .await;
    ApiResponse::from(result)
}

pub async fn put_assertion(
    Path(test_case_id): Path<String>,
    State(repository): State<Repository>,
    Json(payload): Json<PutAssertionPayload>,
) -> Result<ApiResponse<Assertion>, AppError>{
    let result = repository.assertions()
        .put(Assertion {
            customer_id: "eren".to_string(),
            test_case_id,
            id: payload.id.unwrap_or(Uuid::new_v4().to_string()),
            left: payload.left,
            right: payload.right,
            comparison_type: payload.comparison_type,
            negate: payload.negate,
        }).await;
    ApiResponse::from(result)
}

pub async fn list_assertions(
    Path(test_case_id): Path<String>,
    State(app_state): State<AppState>,
) -> Result<ApiResponse<QueryResult<Assertion>>, AppError> {
    let result = app_state
        .repository
        .assertions()
        .list(&"eren".to_string(), &test_case_id)
        .await;
    ApiResponse::from(result)
}

#[derive(Deserialize, Clone)]
pub struct PutAssertionPayload {
    pub id: Option<String>,
    pub left: AssertionItem,
    pub right: AssertionItem,
    pub comparison_type: ComparisonType,
    pub negate: bool,
}

#[derive(Deserialize, Clone)]
pub struct PatchAssertionComparisonType
{
    pub value: ComparisonType,
}

#[derive(Deserialize, Clone)]
pub struct PatchAssertionNegation
{
    pub value: bool,
}

#[derive(Deserialize)]
pub struct AssertionsPathParam {
    test_case_id: String,
    id: String,
}