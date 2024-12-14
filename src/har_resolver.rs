use crate::models::FlattenKeyPrefixType::{AssertionExpression, Input, Output};
use crate::models::{
    Action, Assertion, Expression, FlattenKeyPrefixType, Parameter, ParameterLocation,
    ParameterType, TestCase,
};
use crate::repo::Repository;
use futures::StreamExt;
use har::v1_2::{Entries, Request};
use har::Spec;
use regex::Regex;
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::fmt::Debug;
use std::str::FromStr;

pub async fn build_test_case(
    repository: &Repository,
    spec: &Spec,
    customer_id: &String,
    test_case_name: &String,
) {
    match spec {
        Spec::V1_2(log_v1) => {
            let entries: Vec<&Entries> = log_v1
                .entries
                .iter()
                .filter(|entry| {
                    url_included(&entry.request.url, &".*app.opsgenie.com.*".to_string())
                })
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
            let case = TestCase {
                customer_id: customer_id.clone(),
                id: uuid::Uuid::new_v4().to_string(),
                name: test_case_name.clone(),
            };
            let created_test_case = repository.create_test_case(case).await;

            let mut actions = vec![];
            for i in 0..entries.len() {
                let current = entries.get(i).unwrap();
                println!("{:#?}", current.request.url);
                let action = build_action(i, &created_test_case, current);
                let input_parameters =
                    build_action_input(&action, &current.request, &response_indexes);
                actions.push(action);
                repository.batch_create_parameters(input_parameters).await;
            }
            repository.batch_create_actions(actions).await;
        }
        Spec::V1_3(log_v2) => {}
    }
}

fn url_included(url: &String, pattern: &String) -> bool {
    Regex::new(pattern).unwrap().is_match(url)
}

fn url_excluded(url: &String, pattern: &String) -> bool {
    Regex::new(pattern).unwrap().is_match(url)
}

fn build_action(order: usize, test_case: &TestCase, entry: &Entries) -> Action {
    let action_name = build_action_name(order, &entry.request);
    Action {
        customer_id: test_case.customer_id.clone(),
        test_case_id: test_case.id.clone(),
        id: uuid::Uuid::new_v4().to_string(),
        order: order,
        url: build_url_without_query_params(&entry.request.url),
        name: action_name.clone(),
        mime_type: resolve_mime_type(entry),
        method: entry.request.method.clone(),
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
    action: &Action,
    request: &Request,
    response_indexes: &Vec<HashMap<String, Value>>,
) -> Vec<Parameter> {
    let mut query_params = build_query_parameters(action, request, response_indexes);
    let body_params = build_body_parameters(action, request, response_indexes);
    let header_params = build_header_parameters(action, request, response_indexes);
    let cookie_params = build_cookie_parameters(action, request, response_indexes);
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
                let mut slice = request_indexes[0..order].to_vec();
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
    let non_assertable_value = res_value.is_boolean()
        || (res_value.is_string() && res_value.as_str().unwrap().len() == 0)
        || (res_value.is_array() && res_value.as_array().unwrap().len() == 0);
    !non_assertable_value
}

fn build_body_parameters(
    action: &Action,
    request: &Request,
    response_indexes: &Vec<HashMap<String, Value>>,
) -> Vec<Parameter> {
    let mut parameters: Vec<Parameter> = vec![];
    request.post_data.as_ref().inspect(|post_data| {
        if let Some(content) = &post_data.text {
            if let Ok(value) = serde_json::from_str::<Value>(content) {
                let mut flatten_result: HashMap<String, Value> = HashMap::new();
                flatten_json_value(
                    &build_action_name(action.order, request),
                    &Input,
                    &value,
                    "".to_string(),
                    &mut flatten_result,
                );
                flatten_result.iter().for_each(|(key, value)| {
                    let expression_result =
                        resolve_value_expression_from_prev(action.order, &value, response_indexes);
                    let parameter = build_parameter(
                        action,
                        expression_result,
                        value.clone(),
                        ParameterLocation::Body(key.to_string()),
                    );
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
                        action.order,
                        &Value::String(param.value.as_ref().unwrap().clone()),
                        response_indexes,
                    );
                    let parameter = build_parameter(
                        action,
                        expression_result,
                        Value::String(param.value.clone().unwrap()),
                        ParameterLocation::Body(param.name.clone()),
                    );
                    parameters.push(parameter);
                })
        }
    });
    parameters
}

fn build_query_parameters(
    action: &Action,
    request: &Request,
    response_indexes: &Vec<HashMap<String, Value>>,
) -> Vec<Parameter> {
    let mut parameters: Vec<Parameter> = vec![];
    request.query_string.iter().for_each(|query_string| {
        let expression = resolve_value_expression_from_prev(
            action.order,
            &Value::String(query_string.value.clone()),
            response_indexes,
        );
        parameters.push(build_parameter(
            action,
            expression,
            Value::String(query_string.value.clone()),
            ParameterLocation::Query(query_string.name.clone()),
        ));
    });
    parameters
}

fn build_parameter(
    action: &Action,
    expression: Option<Expression>,
    value: Value,
    location: ParameterLocation,
) -> Parameter {
    Parameter {
        customer_id: action.customer_id.clone(),
        test_case_id: action.test_case_id.clone(),
        action_id: action.id.clone(),
        value_expression: expression,
        id: uuid::Uuid::new_v4().to_string(),
        parameter_type: ParameterType::Input,
        location,
        value,
    }
}

fn build_header_parameters(
    action: &Action,
    request: &Request,
    response_indexes: &Vec<HashMap<String, Value>>,
) -> Vec<Parameter> {
    let mut parameters: Vec<Parameter> = vec![];
    request.headers.iter().for_each(|header| {
        let expression = resolve_value_expression_from_prev(
            action.order,
            &Value::String(header.value.clone()),
            response_indexes,
        );
        parameters.push(build_parameter(
            action,
            expression,
            Value::String(header.value.clone()),
            ParameterLocation::Header(header.name.clone()),
        ));
    });
    parameters
}

fn build_cookie_parameters(
    action: &Action,
    request: &Request,
    response_indexes: &Vec<HashMap<String, Value>>,
) -> Vec<Parameter> {
    let mut parameters: Vec<Parameter> = vec![];
    request.cookies.iter().for_each(|cookie| {
        let expression = resolve_value_expression_from_prev(
            action.order,
            &Value::String(cookie.value.clone()),
            response_indexes,
        );
        parameters.push(build_parameter(
            action,
            expression,
            Value::String(cookie.value.clone()),
            ParameterLocation::Cookie(cookie.name.clone()),
        ));
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
