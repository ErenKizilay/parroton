use axum::extract::{Path, Query, State};
use axum::Json;
use serde::Deserialize;
use crate::api::{ApiResponse, AppError};
use crate::auth::model::AuthenticationProvider;
use crate::auth::service::SetHeaderRequest;
use crate::persistence::repo::{QueryResult, Repository};

pub async fn set_auth_header_value(
    Path(id): Path<String>,
    State(repository): State<Repository>,
    Json(payload): Json<SetHeaderPayload>,
) -> Result<ApiResponse<AuthenticationProvider>, AppError> {
    let result = repository.auth_providers().set_header(SetHeaderRequest {
        customer_id: "eren".to_string(),
        id,
        name: payload.name,
        value: payload.value,
    }).await;
    ApiResponse::from(result)
}

pub async fn set_auth_header_enablement(
    Path(id): Path<String>,
    State(repository): State<Repository>,
    Json(payload): Json<SetHeaderEnablementPayload>,
) -> Result<ApiResponse<AuthenticationProvider>, AppError> {
    let result = repository.auth_providers().set_header_enablement("eren".to_string(),
                                                                   id,
                                                                   payload.name,
                                                                   payload.disabled).await;
    ApiResponse::from(result)
}
pub async fn delete_auth_provider(
    Path(id): Path<String>,
    State(repository): State<Repository>,
) -> Result<ApiResponse<Option<AuthenticationProvider>>, AppError> {
    let result = repository
        .auth_providers()
        .delete(&"eren".to_string(), id)
        .await;
    ApiResponse::from(result)
}

//todo eren: support multiple base urls
pub async fn list_auth_providers(
    params: Query<AuthProvidersQueryParams>,
    State(repository): State<Repository>,
) -> Result<ApiResponse<QueryResult<AuthenticationProvider>>, AppError> {
    let result = repository
        .auth_providers()
        .list(&"eren".to_string(), params.test_case_id.clone(), None)
        .await;
    ApiResponse::from(result)
}

#[derive(Deserialize)]
pub struct AuthProvidersQueryParams {
    test_case_id: Option<String>,
    base_url: Option<String>,
}

#[derive(Deserialize, Clone)]
pub struct SetHeaderPayload {
    pub name: String,
    pub value: String
}

#[derive(Deserialize, Clone)]
pub struct SetHeaderEnablementPayload {
    pub name: String,
    pub disabled: bool
}