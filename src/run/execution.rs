use crate::action::model::Action;
use crate::action_execution::model::ActionExecution;
use crate::api::AppError;
use crate::assertion::check::check_assertion;
use crate::assertion::model::AssertionResult;
use crate::auth::model::ListAuthProvidersRequest;
use crate::http::{
    ApiClient, Endpoint, HttpError, HttpMethod, HttpRequest, HttpResult, ReqBody, ReqParam,
};
use crate::json_path::model::Expression;
use crate::json_path::utils::{evaluate_expression, evaluate_value, reverse_flatten_all};
use crate::parameter::model::{Parameter, ParameterIn};
use crate::persistence::repo::Repository;
use crate::run::model::{Run, RunStatus};
use aws_sdk_dynamodb::config::retry::ShouldAttempt::No;
use aws_sdk_dynamodb::primitives::DateTime;
use aws_sdk_dynamodb::primitives::DateTimeFormat::DateTimeWithOffset;
use serde_json::{Map, Value};
use std::str::FromStr;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{error, info};
use uuid::Uuid;

pub struct RunTestCaseCommand {
    pub customer_id: String,
    pub test_case_id: String,
}

pub async fn run_test(
    repo: Arc<Repository>,
    api_client: Arc<ApiClient>,
    command: RunTestCaseCommand,
) -> Result<Run, AppError> {
    let get_test_case_result = repo
        .test_cases()
        .get(command.customer_id.clone(), command.test_case_id.clone())
        .await;
    match get_test_case_result {
        Ok(test_case_option) => {
            match test_case_option {
                None => {
                    Err(AppError::NotFound("Test case not found!".to_string()))
                }
                Some(test_case) => {
                    info!("Running case {}", test_case.id);
                    let run = repo.runs()
                        .create(Run::builder()
                            .customer_id(command.customer_id.clone())
                            .test_case_id(command.test_case_id.clone())
                            .status(RunStatus::InProgress)
                            .started_at(current_timestamp())
                            .build())
                        .await;

                    let cloned_run = run.clone();
                    let repo_cloned = Arc::clone(&repo);
                    let api_client_cloned = Arc::clone(&api_client);
                    tokio::spawn(async move {
                        let mut context = Map::new();
                        let mut actions = &mut repo_cloned
                            .clone().actions()
                            .list(test_case.customer_id, test_case.id, None)
                            .await
                            .unwrap().items;
                        actions.sort();
                        for action in actions {
                            execute(
                                repo_cloned.clone(),
                                api_client_cloned.clone(),
                                &cloned_run,
                                &action,
                                &mut context)
                                .await;
                        }
                        let assertions = repo_cloned.assertions()
                            .list(&cloned_run.customer_id, &cloned_run.test_case_id).await
                            .unwrap().items;
                        let assertion_context = Value::Object(context.clone());
                        let assertion_results: Vec<AssertionResult> = assertions.iter()
                            .map(|assertion| { check_assertion(assertion, &assertion_context) })
                            .collect();
                        repo_cloned.runs()
                            .update(
                                &cloned_run.customer_id,
                                &cloned_run.test_case_id,
                                &cloned_run.id,
                                &RunStatus::Finished,
                                assertion_results,
                            )
                            .await;
                    });
                    Ok(run)
                }
            }
        }
        Err(err) => Err(err)
    }
}

async fn execute(
    repository: Arc<Repository>,
    client: Arc<ApiClient>,
    run: &Run,
    action: &Action,
    context: &mut Map<String, Value>,
) {
    info!(
        "will execute action: {}, {:?}",
        action.name.clone(),
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_millis()
    );
    let run_cloned = run.clone();
    let action_cloned = action.clone();
    let started_at = current_timestamp();
    let http_request =
        build_http_request(&repository, action, &Value::Object(context.clone())).await;
    let request_body = resolve_request_body_from_request(&http_request);
    let req_params = resolve_request_params_from_request(&http_request);
    let result = client.execute(http_request).await;
    info!(
        "executed action: {}, {:?}",
        action.name.clone(),
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_millis()
    );
    let finished_at = current_timestamp();
    let arc_repo_clone = Arc::clone(&repository);
    let status_code = resolve_status_code(&result);
    let error = resolve_error_from_result(&result);
    let response_body = resolve_response_from_result(&result);
    let request_body_cloned = request_body.clone();
    tokio::spawn(async move {
        let action_execution = ActionExecution::builder()
            .run_id(run_cloned.id.clone())
            .customer_id(run_cloned.customer_id.clone())
            .test_case_id(run_cloned.test_case_id.clone())
            .action_id(action_cloned.id.clone())
            .status_code(status_code)
            .maybe_error(error)
            .started_at(started_at)
            .finished_at(finished_at)
            .maybe_response_body(response_body)
            .maybe_request_body(request_body_cloned)
            .query_params(req_params)
            .build();
        arc_repo_clone
            .action_executions()
            .create(action_execution)
            .await;
    });
    let action_context = match result {
        Ok(http_result) => http_result.res_body.value,
        Err(_) => Value::Null,
    };
    let mut temp = Map::new();
    temp.insert("output".to_string(), action_context);
    temp.insert("input".to_string(), request_body.unwrap_or(Value::Null));
    context.insert(action.name.clone(), Value::Object(temp));
}

fn resolve_request_body_from_request(http_request: &HttpRequest) -> Option<Value> {
    match &http_request.req_body.value {
        None => None,
        Some(val) => Some(val.clone()),
    }
}

fn resolve_request_params_from_request(http_request: &HttpRequest) -> Vec<(String, String)> {
    http_request
        .endpoint
        .query_params
        .iter()
        .map(|param| (param.key.clone(), param.value.clone()))
        .collect()
}

fn resolve_status_code(result: &Result<HttpResult<Value>, HttpError>) -> u16 {
    match result {
        Ok(http_result) => http_result.status_code,
        Err(err) => match err {
            HttpError::Status(status_error, _) => status_error.clone(),
            HttpError::Io(_) => 0,
        },
    }
}

fn resolve_response_from_result(result: &Result<HttpResult<Value>, HttpError>) -> Option<Value> {
    match result {
        Ok(http_res) => {
            let body = &http_res.res_body;
            let value = &body.value;
            Some(value.clone())
        }
        Err(_) => None,
    }
}

fn resolve_error_from_result(result: &Result<HttpResult<Value>, HttpError>) -> Option<String> {
    match result {
        Ok(_) => None,
        Err(err) => Some(err.get_message()),
    }
}

async fn build_http_request(
    repository: &Repository,
    action: &Action,
    context: &Value,
) -> HttpRequest {
    let parameters = repository.parameters().list_all_inputs_of_action(action.customer_id.clone(), action.test_case_id.clone(), action.id.clone())
        .await
        .unwrap();
    let req_params = build_http_params(&parameters, context, ParameterIn::Query);
    let mut headers = build_http_params(&parameters, context, ParameterIn::Header);
    repository.auth_providers()
        .list(ListAuthProvidersRequest::builder()
            .customer_id(action.customer_id.clone())
            .test_case_id(action.test_case_id.clone())
            .base_url(obtain_base_url(&action.url))
            .build())
        .await
        .unwrap().items
        .iter()
        .for_each(|provider| {
            provider
                .headers_by_name
                .iter()
                .filter(|(_, value)| !value.disabled)
                .for_each(|(key, value)| {
                    headers.push(ReqParam::new(key.clone(), value.value.clone()))
                })
        });
    let req_body = build_http_request_body(&parameters, context);
    let endpoint = Endpoint::new(
        HttpMethod::from_str(&action.method).unwrap(),
        build_http_url(&action.url, context),
        vec![],
        req_params,
        headers,
    );
    HttpRequest::new(
        endpoint,
        req_body,
        action
            .mime_type
            .clone()
            .unwrap_or("application/json".to_string()),
    )
}

fn build_http_params(
    parameters: &Vec<Parameter>,
    context: &Value,
    parameter_in: ParameterIn,
) -> Vec<ReqParam> {
    parameters
        .iter()
        .filter(|param| { param.get_parameter_in() == parameter_in })
        .map(|parameter: &Parameter| (parameter, evaluate_value(parameter, context)))
        .filter(|(parameter, eval_result)| {
            if let Err(err) = eval_result {
                println!(
                    "could not eval for param: {:?}, error: {}",
                    parameter.get_path(),
                    err
                );
            }
            eval_result.is_ok()
        })
        .map(|(parameter, eval_result)| {
            ReqParam::new(
                parameter.get_path(),
                eval_result
                    .clone()
                    .unwrap()
                    .to_string()
                    .trim_matches('"')
                    .to_string(),
            )
        })
        .collect()
}

fn build_http_url(
    raw_url: &String,
    context: &Value,
) -> String {
    raw_url.split("/")
        .map(|part|{
            if part.starts_with("$.") {
                evaluate_expression(context, &Expression {
                    value: part.to_string(),
                }).map_or("".to_string(), |value| {value.get(0)
                    .map_or("".to_string(), |v| v.to_string().trim_matches('"').to_string())})
            } else {
                part.to_string()
            }
        }).collect::<Vec<String>>()
        .join("/")
}

fn build_http_request_body(
    parameters: &Vec<Parameter>,
    context: &Value,
) -> ReqBody {
    let tuples: Vec<(String, Value)> = parameters
        .iter()
        .filter(|p| { p.get_parameter_in() == ParameterIn::Body })
        .map(|parameter: &Parameter| (parameter, evaluate_value(parameter, context)))
        .filter(|(parameter, eval_result)| {
            if let Err(err) = eval_result {
                error!(
                    "could not eval for param: {:?}, error: {}",
                    parameter.get_path(),
                    err
                );
            }
            eval_result.is_ok()
        })
        .map(|(parameter, eval_result)| (parameter.get_path(), eval_result.unwrap()))
        .collect();
    if tuples.is_empty() {
        ReqBody::empty()
    } else {
        ReqBody::new(reverse_flatten_all(tuples))
    }
}

fn obtain_base_url(url: &str) -> String {
    // Step 1: Find the scheme (http:// or https://)
    if let Some(scheme_end) = url.find("://") {
        // Step 2: Find the part after the scheme and the domain/subdomain
        let domain_start = scheme_end + 3; // Skip past "://"

        // Step 3: Find where the domain ends (after domain comes `/`, `?`, or `#`)
        if let Some(first_delim) = url[domain_start..].find(&['/', '?', '#'][..]) {
            // Return the base URL including the scheme and the domain only
            return url[0..=domain_start + first_delim - 1].to_string();
        }
        // If no delimiter is found, return the full URL (i.e., no path/query)
        return url.to_string();
    }

    // If no scheme is found, return the input as is
    url.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::json_path::model::Expression;
    use crate::parameter::model::{ParameterLocation, ParameterType};
    use serde_json::json;

    #[test]
    fn test_build_request_body() {
        let param_with_expression = Parameter::builder()
            .customer_id("".to_string())
            .test_case_id("".to_string())
            .action_id("".to_string())
            .parameter_type(ParameterType::Input)
            .location(ParameterLocation::Body(String::from("$.x.y.z")))
            .value(Default::default())
            .value_expression(Expression {
                value: String::from("$.action1.output.aMap.inner.aField"),
            })
            .build();

        let param_with_no_expression = Parameter::builder()
            .customer_id("".to_string())
            .test_case_id("".to_string())
            .action_id("".to_string())
            .parameter_type(ParameterType::Input)
            .location(ParameterLocation::Body(String::from("$.aList[0]")))
            .value(json!("anItem"))
            .build();

        let parameters = vec![param_with_expression, param_with_no_expression];
        let context = json!({
            "action1": {
                "output": {
                    "aMap": {
                        "inner": {
                            "aField": "val1"
                        }
                    }
                }
            }
        });
        let actual = build_http_request_body(&parameters, &context);
        println!("actual: {:?}", actual.value);
        assert_eq!(actual.value.is_some(), true);
        assert_eq!(actual.value.unwrap(), json!({
            "x": {
                "y": {
                    "z": "val1"
                }
            },
            "aList": ["anItem"]
        }))
    }

    #[test]
    fn test_build_http_param() {
        let param_with_expression = Parameter::builder()
            .customer_id("".to_string())
            .test_case_id("".to_string())
            .action_id("".to_string())
            .parameter_type(ParameterType::Input)
            .location(ParameterLocation::Query(String::from("nextPage")))
            .value(Default::default())
            .value_expression(Expression { value: String::from("$.action1.output.pageKey") })
            .build();

        let param_with_no_expression = Parameter::builder()
            .customer_id("".to_string())
            .test_case_id("".to_string())
            .action_id("".to_string())
            .parameter_type(ParameterType::Input)
            .location(ParameterLocation::Header(String::from("x-header1")))
            .value(json!("header-val1"))
            .build();
        let parameters = vec![param_with_expression, param_with_no_expression];
        let context = json!({
            "action1": {
                "output": {
                    "pageKey": "p123"
                }
            }
        });
        let actual_query_params = build_http_params(&parameters, &context, ParameterIn::Query);
        let actual_header_params = build_http_params(&parameters, &context, ParameterIn::Header);
        assert_eq!(actual_query_params, vec![ReqParam {
            key: "nextPage".to_string(),
            value: "p123".to_string(),
        }]);
        assert_eq!(actual_header_params, vec![ReqParam {
            key: "x-header1".to_string(),
            value: "header-val1".to_string(),
        }]);
    }
}

fn current_timestamp() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64
}
