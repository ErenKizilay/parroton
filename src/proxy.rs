use std::collections::HashMap;
use crate::har_resolver::{build_action_name_from_url, build_assertions, build_body_parameters_from_value, build_output_parameters_from_value, build_query_param, build_request_index_from_value, build_response_index_from_value};
use crate::http::{ApiClient, HttpRequest, HttpResult};
use crate::models::{Action, ActionExecution, Assertion, Parameter, ProxyRecord, Run, RunStatus, TestCase};
use crate::persistence::repo::Repository;
use axum::http;
use serde_json::Value;
use std::sync::Arc;
use uuid::uuid;

async fn handler(parts: http::request::Parts) {}

fn build_http_request(parts: http::request::Parts) -> HttpRequest {
    todo!()
}

async fn start_record(repository: Arc<Repository>, request: CreateProxyRecordRequest) -> ProxyRecord {
    let test_case_id = uuid::Uuid::new_v4();
    let test_case = TestCase {
        customer_id: request.customer_id.clone(),
        id: test_case_id.clone().to_string(),
        name: request.name,
        description: request.description,
    };
    let run_id = uuid::Uuid::new_v4();
    let run = Run {
        customer_id: test_case.customer_id.clone(),
        test_case_id: test_case.id.clone(),
        id: run_id.clone().to_string(),
        status: RunStatus::InProgress,
        started_at: "".to_string(),
        finished_at: None,
    };
    let repo_clone = repository.clone();
    let repo_clone2 = repository.clone();
    tokio::task::spawn(async move {
        repo_clone.test_cases()
            .create_test_case(test_case).await;
    });

    tokio::task::spawn(async move {
        repo_clone2.runs()
            .create(run).await;
    });

    ProxyRecord {
        customer_id: request.customer_id.clone(),
        test_case_id: test_case_id.clone().to_string(),
        run_id: run_id.clone().to_string(),
        id: uuid::Uuid::new_v4().to_string(),
    }
}

async fn end_record(repository: Arc<Repository>, action: &Action, run: &Run) {
    let action_executions = repository.action_executions()
        .list(&action.customer_id, &action.test_case_id, &run.id)
        .await.unwrap();
    let action_param_result = build_action_parameters(action, action_executions);
    let repo_cloned = repository.clone();
    let repo_cloned2 = repository.clone();
    tokio::task::spawn(async move {
       repo_cloned.parameters()
           .batch_create(action_param_result.parameters)
           .await;
    });

    tokio::task::spawn(async move {
        repo_cloned2.assertions()
            .batch_create(action_param_result.assertions)
            .await;
    });
}

async fn record_request(repository: Arc<Repository>, client: Arc<ApiClient>, parts: http::request::Parts, run: &Run, test_case: &TestCase) {
    let http_request = build_http_request(parts);
    let action_executions = repository.action_executions()
        .list(&run.customer_id, &run.test_case_id, &run.id)
        .await.unwrap();
    let action = build_action(&test_case, &http_request, action_executions.len());
    let action_exec = build_action_execution(&run, &action.id, &http_request, None);
    let http_result = client.execute(http_request).await;
    if let Ok(http_result) = http_result {
        let updated_exec = update_execution(action_exec, http_result);
        repository.action_executions()
            .create(updated_exec).await;
    }
}

fn build_action_parameters(action: &Action, executions: Vec<ActionExecution>) -> BuildActionParamResult {
    let indexes: (Vec<HashMap<String, Value>>, Vec<HashMap<String, Value>>) = executions.iter()
        .map(|execution| {
            (build_request_index_from_value(&action.name, &execution.clone().request_body.unwrap_or(Value::Null)),
            build_response_index_from_value(&action.name, &execution.clone().response_body.unwrap_or(Value::Null)))
        }).collect();

    let mut parameters: Vec<Parameter> = Vec::new();
    let mut assertions: Vec<Assertion> = Vec::new();
    for execution in executions {
        let query_parameters: Vec<Parameter> = execution.query_params.iter()
            .map(|param| {
                build_query_param(action, &indexes.1, &param.0, &param.1)
            })
            .collect();
        parameters.extend(query_parameters);
        //todo!("add headers to action execution model to resolve parameters here or find another way");
        if let Some(body_value) = execution.request_body {
            let body_parameters = build_body_parameters_from_value(action, &indexes.1, &body_value);
            parameters.extend(body_parameters);
            assertions.extend(build_assertions(&action, &indexes.0, &indexes.1));
        }
        if let Some(res_value) = execution.response_body{
            let output_parameters = build_output_parameters_from_value(action, &res_value);
            parameters.extend(output_parameters);
        }
    }
    BuildActionParamResult {
        parameters,
        assertions,
    }
}

fn build_action(test_case: &TestCase, http_req: &HttpRequest, order: usize) -> Action {
    let url = http_req.endpoint.to_url();
    Action {
        customer_id: test_case.customer_id.clone(),
        test_case_id: test_case.id.clone(),
        id: uuid::Uuid::new_v4().to_string(),
        order,
        url: url.clone(),
        name: build_action_name_from_url(order, &url),
        mime_type: Some(http_req.content_type.clone()),
        method: http_req.endpoint.method.to_string(),
    }
}

fn build_action_execution(run: &Run, action_id: &String, http_req: &HttpRequest, http_result: Option<HttpResult<Value>>) -> ActionExecution {
    let response_pair = http_result.map_or((0, None), |http_result: HttpResult<Value>|
        { (http_result.status_code, Some(http_result.res_body.value)) });
    ActionExecution {
        run_id: run.id.clone(),
        customer_id: run.customer_id.clone(),
        test_case_id: run.test_case_id.clone(),
        action_id: action_id.clone(),
        id: uuid::Uuid::new_v4().to_string(),
        status_code: response_pair.0,
        error: None,
        response_body: response_pair.1,
        request_body: http_req.get_body(),
        query_params: http_req.endpoint.query_params.iter()
            .map(|rp| { (rp.key.clone(), rp.value.clone()) })
            .collect(),
        started_at: "".to_string(),
        finished_at: "".to_string(),
        assertion_results: vec![],
    }
}

fn update_execution(mut exec: ActionExecution, http_result: HttpResult<Value>) -> ActionExecution {
    let response_pair = (http_result.status_code, http_result.res_body.value);
    exec.status_code = response_pair.0;
    exec.response_body = Some(response_pair.1);
    exec
}

struct BuildActionParamResult {
    parameters: Vec<Parameter>,
    assertions: Vec<Assertion>
}

pub struct CreateProxyRecordRequest {
    pub customer_id: String,
    pub id: String,
    pub name: String,
    pub description: String,
}
