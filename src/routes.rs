use std::sync::Arc;
use crate::models::{ParameterType, TestCase};
use crate::repo::{ParameterIn, Repository};
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use axum::response::IntoResponse;
use serde::Deserialize;

// The query parameters for todos index
#[derive(Debug, Deserialize, Default)]
pub struct Pagination {
    pub offset: Option<usize>,
    pub limit: Option<usize>,
}

pub async fn list_test_cases(State(repository): State<Arc<Repository>>) -> impl IntoResponse {
    let result = repository.list_test_cases("eren".to_string(), None).await;
    (StatusCode::OK, Json(result.items))
}

pub async fn get_test_case(Path(id): Path<String>, State(repository): State<Arc<Repository>>) -> impl IntoResponse {
    (StatusCode::OK, Json(repository.get_test_case("eren".to_string(), id).await))
}

pub async fn list_actions(Path(test_case_id): Path<String>, params: Query<ActionQueryParams>, State(repository): State<Arc<Repository>>) -> impl IntoResponse {
    let result = match params.before_order {
        None => {
            repository.list_actions("eren".to_string(), test_case_id.to_string(), None).await
        }
        Some(order) => {
            repository.list_previous_actions("eren".to_string(), test_case_id.to_string(), order, None).await
        }
    };
    (StatusCode::OK, Json(result.items))
}

pub async fn list_parameters(Path(path_params): Path<(String, String)>,
                             params: Query<ParameterQueryParams>,
                             State(repository): State<Arc<Repository>>) -> impl IntoResponse {
    let test_case_id = path_params.0;
    let action_id = path_params.1;
    let parameter_type = &params.parameter_type;
    let result = match &params.path {
        None => {
            let option = &params.parameter_in;
            repository.list_parameters_of_action("eren".to_string(), test_case_id.to_string(),
                                                 action_id.to_string(),
                                                 parameter_type.clone(),
                                                 option.clone(),
                                                 None).await
        }
        Some(path) => {
            repository.query_parameters_of_action_by_path("eren".to_string(), test_case_id.to_string(),
                                                          action_id.to_string(),
                                                          parameter_type.clone(), path.clone(), None).await
        }
    };
    (StatusCode::OK, Json(result.items))
}

#[derive(Deserialize)]
pub struct ActionQueryParams {
    before_order: Option<usize>,
}

#[derive(Deserialize, Clone)]
pub struct ParameterQueryParams {
    path: Option<String>,
    parameter_type: ParameterType,
    parameter_in: Option<ParameterIn>,
}




