use crate::api::AppError;
use crate::assertions::check_assertion;
use crate::http::{
    ApiClient, Endpoint, HttpError, HttpMethod, HttpRequest, HttpResult, ReqBody, ReqParam,
};
use crate::json_path_utils::{evaluate_value, reverse_flatten_all};
use crate::models::{Action, ActionExecution, AssertionResult, Parameter, Run, RunStatus};
use crate::persistence::repo::{ParameterIn, Repository};
use aws_sdk_dynamodb::primitives::DateTime;
use aws_sdk_dynamodb::primitives::DateTimeFormat::DateTimeWithOffset;
use serde_json::{Map, Value};
use std::str::FromStr;
use std::sync::Arc;
use std::time::SystemTime;
use tracing::info;
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
                    let run_id = Uuid::new_v4().to_string();
                    let run = repo.runs()
                        .create(Run {
                            customer_id: command.customer_id.clone(),
                            test_case_id: command.test_case_id.clone(),
                            id: run_id,
                            status: RunStatus::InProgress,
                            started_at: DateTime::from(SystemTime::now()).fmt(DateTimeWithOffset).unwrap(),
                            finished_at: None,
                        })
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
                        repo_cloned.runs()
                            .update(
                                &cloned_run.customer_id,
                                &cloned_run.test_case_id,
                                &cloned_run.id,
                                &RunStatus::Finished,
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
    let started_at = SystemTime::now();
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
    let finished_at = SystemTime::now();
    let arc_repo_clone = Arc::clone(&repository);
    let status_code = resolve_status_code(&result);
    let error = resolve_error_from_result(&result);
    let response_body = resolve_response_from_result(&result);
    let assertion_context = Value::Object(context.clone());
    tokio::spawn(async move {
        let assertions = arc_repo_clone.assertions()
            .list(&run_cloned.customer_id.clone(), &run_cloned.test_case_id.clone()).await
            .unwrap().items;
        let assertion_results: Vec<AssertionResult> = assertions.iter()
            .map(|assertion| { check_assertion(assertion, &assertion_context) })
            .collect();
        arc_repo_clone
            .action_executions()
            .create(ActionExecution {
                run_id: run_cloned.id.clone(),
                customer_id: run_cloned.customer_id.clone(),
                test_case_id: run_cloned.test_case_id.clone(),
                action_id: action_cloned.id.clone(),
                id: Uuid::new_v4().to_string(),
                status_code,
                error,
                started_at: DateTime::from(started_at).fmt(DateTimeWithOffset).unwrap(),
                finished_at: DateTime::from(finished_at).fmt(DateTimeWithOffset).unwrap(),
                response_body,
                request_body,
                query_params: req_params,
                assertion_results,
            })
            .await;
    });
    let action_context = match result {
        Ok(http_result) => http_result.res_body.value,
        Err(_) => Value::Null,
    };
    let mut temp = Map::new();
    temp.insert("output".to_string(), action_context);
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
    let req_params = build_http_params(&parameters, context, ParameterIn::Query).await;
    let mut headers = build_http_params(&parameters, context, ParameterIn::Header).await;
    repository.auth_providers()
        .list(
            &action.customer_id,
            Some(action.test_case_id.clone()),
            Some(obtain_base_url(&action.url)),
        )
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
    let req_body = build_http_request_body(&parameters, context).await;
    let endpoint = Endpoint::new(
        HttpMethod::from_str(&action.method).unwrap(),
        action.url.clone(),
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

async fn build_http_params(
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

async fn build_http_request_body(
    parameters: &Vec<Parameter>,
    context: &Value,
) -> ReqBody {
    let tuples: Vec<(String, Value)> = parameters
        .iter()
        .filter(|p| { p.get_parameter_in() == ParameterIn::Body })
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
