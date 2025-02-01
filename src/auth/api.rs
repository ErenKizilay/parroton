use crate::api::{ApiResponse, AppError};
use crate::auth::model::{AuthHeaderValue, AuthenticationProvider, ListAuthProvidersRequest};
use crate::auth::service::SetHeaderRequest;
use crate::persistence::model::QueryResult;
use crate::persistence::repo::Repository;
use axum::extract::{Path, Query, State};
use axum::Json;
use serde::Deserialize;
use std::collections::HashSet;

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

pub async fn add_auth_header_value(
    Path(id): Path<String>,
    State(repository): State<Repository>,
    Json(payload): Json<SetHeaderPayload>,
) -> Result<ApiResponse<AuthenticationProvider>, AppError> {
    let result = repository.auth_providers().add_header(SetHeaderRequest {
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

pub async fn get_auth_provider(
    Path(id): Path<String>,
    State(repository): State<Repository>,
) -> Result<ApiResponse<AuthenticationProvider>, AppError> {
    let result = repository
        .auth_providers()
        .get(&"eren".to_string(), id)
        .await;
    ApiResponse::from_option(result)
}

pub async fn create_auth_provider(
    State(repository): State<Repository>,
    Json(payload): Json<CreateAuthProviderPayload>,
) -> Result<ApiResponse<AuthenticationProvider>, AppError> {
    let provider = AuthenticationProvider::builder()
        .name(payload.name)
        .base_url(payload.url)
        .customer_id("eren".to_string())
        .headers_by_name(payload.headers.iter()
            .map(|h| (h.name.clone(), AuthHeaderValue::builder()
                .value(h.value.clone())
                .build()))
            .collect())
        .linked_test_case_ids(HashSet::new())
        .build();
    let result = repository
        .auth_providers()
        .create(provider)
        .await;
    ApiResponse::from(result)
}

pub async fn list_auth_providers(
    params: Query<AuthProvidersQueryParams>,
    State(repository): State<Repository>,
) -> Result<ApiResponse<QueryResult<AuthenticationProvider>>, AppError> {
    let result = repository
        .auth_providers()
        .list(ListAuthProvidersRequest::builder()
            .customer_id("eren".to_string())
            .maybe_test_case_id(params.test_case_id.clone())
            .maybe_next_page_key(params.next_page_key.clone())
            .maybe_keyword(params.keyword.clone())
            .build())
        .await;
    ApiResponse::from(result)
}

pub async fn list_auth_providers_with_multiple_urls(State(repository): State<Repository>, Json(payload): Json<SearchByMultiBaseUrlPayload>) -> Result<ApiResponse<Vec<AuthenticationProvider>>, AppError> {
    let result = repository.auth_providers()
        .list_by_multi_base_url(&"eren".to_string(), payload.urls).await;
    ApiResponse::from(result)
}

#[derive(Deserialize)]
pub struct AuthProvidersQueryParams {
    test_case_id: Option<String>,
    base_url: Option<String>,
    next_page_key: Option<String>,
    keyword: Option<String>,
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

#[derive(Deserialize, Clone)]
pub struct CreateAuthProviderPayload {
    pub name: String,
    pub url: String,
    pub headers: Vec<SetHeaderPayload>,
}

#[derive(Deserialize, Clone)]
pub struct SearchByMultiBaseUrlPayload {
    pub urls: Vec<String>
}

