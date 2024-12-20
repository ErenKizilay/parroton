use crate::http::{ApiClient, Endpoint, HttpMethod, HttpRequest, ReqBody, ReqParam};
use crate::testing::FlattenKeyPrefixType::{AssertionExpression, Input, Output};
use futures::StreamExt;
use har::v1_2::{Entries, Log, Request};
use har::Spec;
use regex::Regex;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Map, Value};
use serde_json_path::JsonPath;
use std::collections::HashMap;
use std::fmt::Debug;
use std::str::FromStr;
use std::time::Duration;

#[derive(Serialize, Deserialize)]
enum ParameterLocation {
    Header(String),
    Cookie(String),
    Query(String),
    Body(String),
    StatusCode,
}

enum FlattenKeyPrefixType {
    Output,
    Input,
    AssertionExpression,
}

#[derive(Serialize, Deserialize)]
struct Parameter {
    location: ParameterLocation,
    value: Value,
    expression: Option<Expression>,
}

impl Parameter {
    fn is_query_param(&self) -> bool {
        matches!(self.location, ParameterLocation::Query(_))
    }
    fn is_body_param(&self) -> bool {
        matches!(self.location, ParameterLocation::Body(_))
    }
    fn is_header_param(&self) -> bool {
        matches!(self.location, ParameterLocation::Header(_))
    }

    fn is_cookie(&self) -> bool {
        matches!(self.location, ParameterLocation::Cookie(_))
    }

    fn get_sort_key(&self) -> String {
        match &self.location {
            ParameterLocation::Header(h) => h.clone(),
            ParameterLocation::Query(q) => q.clone(),
            ParameterLocation::Body(b) => b.clone(),
            ParameterLocation::StatusCode => "".to_string(),
            ParameterLocation::Cookie(c) => c.clone(),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct Action {
    url: String,
    name: String,
    mime_type: Option<String>,
    method: String,
    input: Vec<Parameter>,
    output: Vec<Parameter>,
    assertions: Vec<Assertion>,
}

pub struct ActionExecution {
    action_name: String,
    duration: Duration,
    status_code: usize,
}

#[derive(Serialize, Deserialize)]
struct Expression {
    value: String,
}

struct TestCase {
    name: String,
    actions: Vec<Action>,
}
#[derive(Serialize, Deserialize)]
enum Assertion {
    EqualTo(Expression, Expression),
}

struct Configuration {
    url_inclusion_pattern: String,
}
pub fn create_test_case(spec: &Spec) -> Vec<Action> {
    match spec {
        Spec::V1_2(log_v1) => build_actions(log_v1),
        Spec::V1_3(log_v2) => {
            vec![]
        }
    }
}

pub async fn execute_actions(actions: Vec<Action>) {
    let mut context = Map::new();
    for action in actions {
        execute(&action, &mut context).await;
    }
}

fn build_actions(log: &Log) -> Vec<Action> {
    let entries: Vec<&Entries> = log
        .entries
        .iter()
        .filter(|entry| url_included(&entry.request.url, &".*app.opsgenie.com.*".to_string()))
        .filter(|entry| !url_excluded(&entry.request.url, &".*/gateway/api.*".to_string()))
        .filter(|entry| !url_excluded(&entry.request.url, &".*/jira/.*".to_string()))
        .collect();
    let response_indexes: Vec<HashMap<String, Value>> = entries
        .iter()
        .enumerate()
        .map(|(i, entry)| build_response_index(i, entry))
        .collect();

    let request_indexes: Vec<HashMap<String, Value>> = entries
        .iter()
        .enumerate()
        .map(|(i, entry)| build_request_index(i, entry))
        .collect();

    let mut actions = vec![];
    for i in 0..entries.len() {
        let current = entries.get(i).unwrap();
        println!("{:#?}", current.request.url);
        let action = build_action(i, current, &request_indexes, &response_indexes);
        serde_json::to_writer_pretty(std::io::stdout(), &action).unwrap();
        actions.push(action);
        println!();
        println!("---------");
    }
    actions
}

fn url_included(url: &String, pattern: &String) -> bool {
    Regex::new(pattern).unwrap().is_match(url)
}

fn url_excluded(url: &String, pattern: &String) -> bool {
    Regex::new(pattern).unwrap().is_match(url)
}

fn build_action(
    order: usize,
    entry: &Entries,
    request_indexes: &Vec<HashMap<String, Value>>,
    response_indexes: &Vec<HashMap<String, Value>>,
) -> Action {
    let action_name = build_action_name(order, &entry.request);
    Action {
        url: build_url_without_query_params(&entry.request.url),
        name: action_name.clone(),
        mime_type: resolve_mime_type(entry),
        method: entry.request.method.clone(),
        input: build_action_input(order, &entry.request, &response_indexes),
        output: vec![],
        assertions: build_assertions(&action_name, order, request_indexes, response_indexes),
    }
}

fn build_url_without_query_params(url: &String) -> String {
    let re = Regex::new(r"\?.*$").unwrap();
    re.replace(url, "").to_string()
}

fn resolve_mime_type(entry: &Entries) -> Option<String> {
    let request = &entry.request;
    let post_data = &request.post_data;
    post_data
        .as_ref()
        .map(|post_data| post_data.mime_type.clone())
}

fn build_response_index(order: usize, entry: &Entries) -> HashMap<String, Value> {
    let response = &entry.response;
    let content = &response.content;
    let option = &content.text;
    option.as_ref().map_or(HashMap::new(), |text| {
        let mut result = HashMap::<String, Value>::new();
        let response_value = serde_json::from_str::<Value>(&text).unwrap();
        let action_name = build_action_name(order, &entry.request);
        flatten_json_value(
            &action_name,
            &Output,
            &response_value,
            "".to_string(),
            &mut result,
        );
        result
    })
}

fn build_request_index(order: usize, entry: &Entries) -> HashMap<String, Value> {
    let request = &entry.request;
    let optional_post_data = request.post_data.as_ref();
    let mut result = HashMap::<String, Value>::new();
    if let Some(post_data) = optional_post_data {
        if let Some(text) = post_data.text.as_ref() {
            if post_data.mime_type.contains("application/json") {
                let input_map = serde_json::from_str::<Value>(text).unwrap();
                let action_name = build_action_name(order, &request);
                flatten_json_value(
                    &action_name,
                    &AssertionExpression,
                    &input_map,
                    "".to_string(),
                    &mut result,
                );
            }
        }
    }
    result
}

fn build_action_name(order: usize, request: &Request) -> String {
    let formatted_name = request.url.replace("-", "_");
    let base_name = formatted_name.split("/").last().unwrap();
    let re = Regex::new(r"\?.*$").unwrap(); // Matches '?' and everything after it
    format!("{}_{}", re.replace(base_name, "").to_string(), order)
}

fn build_action_input(
    order: usize,
    request: &Request,
    response_indexes: &Vec<HashMap<String, Value>>,
) -> Vec<Parameter> {
    let mut query_params = build_query_parameters(order, request, response_indexes);
    let body_params = build_body_parameters(order, request, response_indexes);
    let header_params = build_header_parameters(order, request, response_indexes);
    let cookie_params = build_cookie_parameters(order, request, response_indexes);
    query_params.extend(body_params);
    query_params.extend(header_params);
    query_params.extend(cookie_params);
    query_params
}

fn build_assertions(
    action_name: &String,
    order: usize,
    request_indexes: &Vec<HashMap<String, Value>>,
    response_indexes: &Vec<HashMap<String, Value>>,
) -> Vec<Assertion> {
    let mut assertions: Vec<Assertion> = vec![];

    response_indexes.get(order).iter().for_each(|response| {
        response.iter().for_each(|(path, res_value)| {
            if should_build_assertion_for_response_value(res_value) {
                let mut  slice = request_indexes[0..order].to_vec();
                slice.reverse();
                let expression_result =
                    resolve_value_expression_from_slice_index(&res_value, &slice);
                if let Some(expression) = expression_result {
                    let assertion = Assertion::EqualTo(
                        expression,
                        Expression {
                            value: path.to_string(),
                        },
                    );
                    assertions.push(assertion);
                }
            }
        })
    });
    assertions
}

fn should_build_assertion_for_response_value(res_value: &Value) -> bool {
    let non_assertable_value = res_value.is_boolean() && (res_value.is_string() && res_value.as_str().unwrap().len() == 0) &&
        (res_value.is_array() && res_value.as_array().unwrap().len() == 0);
    !non_assertable_value
}

fn build_body_parameters(
    order: usize,
    request: &Request,
    response_indexes: &Vec<HashMap<String, Value>>,
) -> Vec<Parameter> {
    let mut parameters: Vec<Parameter> = vec![];
    request.post_data.as_ref().inspect(|post_data| {
        if let Some(content) = &post_data.text {
            if let Ok(value) = serde_json::from_str::<Value>(content) {
                let mut flatten_result: HashMap<String, Value> = HashMap::new();
                flatten_json_value(
                    &build_action_name(order, request),
                    &Input,
                    &value,
                    "".to_string(),
                    &mut flatten_result,
                );
                flatten_result.iter().for_each(|(key, value)| {
                    let expression_result =
                        resolve_value_expression_from_prev(order, &value, response_indexes);
                    let parameter = Parameter {
                        expression: expression_result,
                        value: value.clone(),
                        location: ParameterLocation::Body(key.to_string()),
                    };
                    parameters.push(parameter);
                })
            }
        }
        if let Some(params) = &post_data.params {
            params
                .iter()
                .filter(|p| p.value.is_some())
                .for_each(|param| {
                    let expression_result = resolve_value_expression_from_prev(
                        order,
                        &Value::String(param.value.as_ref().unwrap().clone()),
                        response_indexes,
                    );
                    let parameter = Parameter {
                        expression: expression_result,
                        value: Value::String(param.value.clone().unwrap()),
                        location: ParameterLocation::Body(param.name.clone()),
                    };
                    parameters.push(parameter);
                })
        }
    });
    parameters
}

fn build_query_parameters(
    order: usize,
    request: &Request,
    response_indexes: &Vec<HashMap<String, Value>>,
) -> Vec<Parameter> {
    let mut parameters: Vec<Parameter> = vec![];
    request.query_string.iter().for_each(|query_string| {
        let expression = resolve_value_expression_from_prev(
            order,
            &Value::String(query_string.value.clone()),
            response_indexes,
        );
        parameters.push(Parameter {
            expression,
            value: Value::String(query_string.value.clone()),
            location: ParameterLocation::Query(query_string.name.clone()),
        });
    });
    parameters
}

fn build_header_parameters(
    order: usize,
    request: &Request,
    response_indexes: &Vec<HashMap<String, Value>>,
) -> Vec<Parameter> {
    let mut parameters: Vec<Parameter> = vec![];
    request.headers.iter().for_each(|header| {
        let expression = resolve_value_expression_from_prev(
            order,
            &Value::String(header.value.clone()),
            response_indexes,
        );
        parameters.push(Parameter {
            expression,
            value: Value::String(header.value.clone()),
            location: ParameterLocation::Header(header.name.clone()),
        });
    });
    parameters
}

fn build_cookie_parameters(
    order: usize,
    request: &Request,
    response_indexes: &Vec<HashMap<String, Value>>,
) -> Vec<Parameter> {
    let mut parameters: Vec<Parameter> = vec![];
    request.cookies.iter().for_each(|cookie| {
        let expression = resolve_value_expression_from_prev(
            order,
            &Value::String(cookie.value.clone()),
            response_indexes,
        );
        parameters.push(Parameter {
            expression,
            value: Value::String(cookie.value.clone()),
            location: ParameterLocation::Cookie(cookie.name.clone()),
        });
    });
    parameters
}

fn resolve_value_expression_from_prev(
    order: usize,
    value: &Value,
    response_indexes: &Vec<HashMap<String, Value>>,
) -> Option<Expression> {
    let prev_indexes: &[HashMap<String, Value>] = &response_indexes[0..order];
    resolve_value_expression_from_slice_index(&value, prev_indexes)
}

fn resolve_value_expression_from_prev_and_equal(
    order: usize,
    value: &Value,
    response_indexes: &Vec<HashMap<String, Value>>,
) -> Option<Expression> {
    let next_indexes: &[HashMap<String, Value>] = &response_indexes[0..=order];
    resolve_value_expression_from_slice_index(&value, next_indexes)
}

fn resolve_value_expression_from_slice_index(
    value: &&Value,
    indexes: &[HashMap<String, Value>],
) -> Option<Expression> {
    indexes
        .iter()
        .rev()
        .enumerate()
        .flat_map(|(i, indexes)| indexes)
        .filter(|(_, indexed_value)| indexed_value.eq(value))
        .map(|(key, value)| Expression { value: key.clone() })
        .next()
}

fn flatten_json_value(
    action_name: &String,
    prefix_type: &FlattenKeyPrefixType,
    value: &Value,
    prefix_so_far: String,
    result: &mut HashMap<String, Value>,
) {
    match value {
        Value::Object(map) => {
            for (key, val) in map {
                let new_prefix = if prefix_so_far.is_empty() {
                    match prefix_type {
                        FlattenKeyPrefixType::Output => {
                            format!("$.{}.{}.{}", action_name, "output", key)
                        }
                        FlattenKeyPrefixType::Input => {
                            format!("$.{}", key)
                        }
                        FlattenKeyPrefixType::AssertionExpression => {
                            format!("$.{}.{}.{}", action_name, "input", key)
                        }
                    }
                } else {
                    format!("{}.{}", prefix_so_far, key)
                };
                flatten_json_value(action_name, prefix_type, val, new_prefix, result);
            }
        }
        Value::Array(arr) => {
            for (index, val) in arr.iter().enumerate() {
                let new_prefix = format!("{}[{}]", prefix_so_far, index);
                flatten_json_value(action_name, prefix_type, val, new_prefix, result);
            }
        }
        _ => {
            result.insert(prefix_so_far, value.clone());
        }
    }
}

/// Merges a path-value pair into the given base map
fn merge_path(base: &mut Map<String, Value>, path: String, value: Value) {
    let parts: Vec<&str> = path.trim_start_matches('$').split('.').collect();

    let mut current = base;
    for key in &parts[..parts.len() - 1] {
        current = current
            .entry(key.to_string())
            .or_insert_with(|| Value::Object(Map::new()))
            .as_object_mut()
            .unwrap();
    }

    current.insert(parts.last().unwrap().to_string(), value);
}

/// Merges multiple parameters into a single nested map
fn reverse_flatten_all(path_value_pairs: Vec<(String, Value)>) -> Value {
    let mut root = Map::new();
    let array_key_regex = Regex::new(r"^([^\[]+)\[(\d+)\](?:\.(.+))?$").unwrap();

    for (key, mut value) in path_value_pairs {
        // Remove the leading "$." from the key
        let key = key.strip_prefix("$.").unwrap_or(&key);
        let parts: Vec<&str> = key.split('.').collect();
        let mut current = &mut root;

        for (i, part) in parts.iter().enumerate() {
            if i == parts.len() - 1 {
                // Last part of the key
                if let Some(captures) = array_key_regex.captures(part) {
                    let array_name = captures.get(1).unwrap().as_str();
                    let array_index: usize = captures.get(2).unwrap().as_str().parse().unwrap();
                    let nested_field = captures.get(3).map(|m| m.as_str());

                    // Work on the array part
                    let array = current
                        .entry(array_name)
                        .or_insert_with(|| Value::Array(vec![]));
                    if let Value::Array(ref mut vec) = array {
                        if vec.len() <= array_index {
                            vec.resize(array_index + 1, Value::Object(Map::new()));
                        }
                        let ref mut current_array_item_val: Value = vec[array_index];
                        if let Value::Object(ref mut obj) = current_array_item_val {
                            if let Some(field_name) = nested_field {
                                obj.insert(field_name.to_string(), value.clone());
                            } else {
                                *current_array_item_val = value.clone();
                            }
                        }
                    }
                } else {
                    current.insert(part.to_string(), value.clone());
                }
            } else {
                // Intermediate parts
                if let Some(captures) = array_key_regex.captures(part) {
                    let array_name = captures.get(1).unwrap().as_str();
                    let array_index: usize = captures.get(2).unwrap().as_str().parse().unwrap();

                    // Precompute array entry
                    let array = current
                        .entry(array_name)
                        .or_insert_with(|| Value::Array(vec![]));
                    current = if let Value::Array(ref mut vec) = array {
                        if vec.len() <= array_index {
                            vec.resize(array_index + 1, Value::Object(Map::new()));
                        }
                        vec[array_index]
                            .as_object_mut()
                            .expect("Expected an object in the array")
                    } else {
                        panic!("Expected an array");
                    };
                } else {
                    current = current
                        .entry(part.to_string())
                        .or_insert_with(|| Value::Object(Map::new()))
                        .as_object_mut()
                        .expect("Expected an object for the intermediate part");
                }
            }
        }
    }

    Value::Object(root)
}

fn build_http_request_params(action: &Action, context: &Value) -> Vec<ReqParam> {
    action
        .input
        .iter()
        .filter(|param| param.is_query_param())
        .map(|parameter: &Parameter| match &parameter.location {
            ParameterLocation::Query(name) => ReqParam::new(
                name.clone(),
                evaluate_value(parameter, context)
                    .as_str()
                    .unwrap()
                    .to_string(),
            ),
            _ => {
                unreachable!()
            }
        })
        .collect()
}

fn build_http_header_params(action: &Action, context: &Value) -> Vec<ReqParam> {
    action
        .input
        .iter()
        .filter(|param| param.is_header_param())
        .map(|parameter: &Parameter| match &parameter.location {
            ParameterLocation::Header(name) => ReqParam::new(
                name.clone(),
                evaluate_value(parameter, context)
                    .as_str()
                    .unwrap()
                    .to_string(),
            ),
            _ => {
                unreachable!()
            }
        })
        .collect()
}

fn build_http_cookie_params(action: &Action, context: &Value) -> Vec<ReqParam> {
    action
        .input
        .iter()
        .filter(|param| param.is_cookie())
        .map(|parameter: &Parameter| match &parameter.location {
            ParameterLocation::Cookie(name) => ReqParam::new(
                name.clone(),
                evaluate_value(parameter, context)
                    .as_str()
                    .unwrap()
                    .to_string(),
            ),
            _ => {
                unreachable!()
            }
        })
        .collect()
}

fn build_http_request_body(action: &Action, context: &Value) -> ReqBody {
    let tuples: Vec<(String, Value)> = action
        .input
        .iter()
        .filter(|param| param.is_body_param())
        .map(|parameter: &Parameter| (parameter.get_sort_key(), evaluate_value(parameter, context)))
        .collect();
    ReqBody::new(reverse_flatten_all(tuples))
}

fn evaluate_value(parameter: &Parameter, context: &Value) -> Value {
    let result = match &parameter.expression {
        None => parameter.value.clone(),
        Some(exp) => {
            let json_path = JsonPath::parse(exp.value.as_str()).unwrap();
            let node_list = json_path.query(context);
            let x = if parameter.value.is_array() {
                let values: Vec<Value> = node_list
                    .all()
                    .iter()
                    .cloned()
                    .map(|node| node.clone())
                    .collect();
                Value::Array(values)
            } else {
                node_list.exactly_one().unwrap().clone()
            };
            println!("expr: {}, value: {}", exp.value.as_str(), x.clone());
            x
        }
    };
    result
}

fn build_http_request(action: &Action, context: &Value) -> HttpRequest {
    let req_params = build_http_request_params(action, context);
    let headers = build_http_header_params(action, context);
    let cookies = build_http_cookie_params(action, context);
    let req_body = build_http_request_body(action, context);
    println!(
        "query params: {}",
        serde_json::to_string_pretty(&req_params).unwrap()
    );
    println!(
        "req body: {}",
        serde_json::to_string_pretty(&req_body).unwrap()
    );
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

async fn execute(action: &Action, context: &mut Map<String, Value>) {
    println!("will execute action: {}", action.name.clone());
    let client = ApiClient::new(("authxxx".to_string(), "asdxxx".to_string()));
    let http_request = build_http_request(action, &Value::Object(context.clone()));
    let result = client.execute(http_request).await;
    let action_context = match result {
        Ok(http_result) => http_result.res_body.value,
        Err(_) => Value::Null,
    };
    let mut temp = Map::new();
    temp.insert("output".to_string(), action_context);
    context.insert(action.name.clone(), Value::Object(temp));
}
