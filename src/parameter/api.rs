use crate::api::{ApiResponse, AppError};
use crate::json_path::model::Expression;
use crate::parameter::model::{Parameter, ParameterIn, ParameterType};
use crate::persistence::model::QueryResult;
use crate::persistence::repo::Repository;
use axum::extract::{Path, Query, State};
use axum::Json;
use serde::Deserialize;

#[derive(Deserialize, Clone)]
pub struct ParameterQueryParams {
    path: Option<String>,
    parameter_type: ParameterType,
    parameter_in: Option<ParameterIn>,
}

#[derive(Deserialize)]
pub struct ParametersPathParam {
    test_case_id: String,
    action_id: String,
    id: String,
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
    let result = repository
        .parameters()
        .update_expression(
            "eren".to_string(),
            path_params.test_case_id,
            path_params.action_id,
            path_params.id,
            expression,
        )
        .await;
    ApiResponse::from(result)
}