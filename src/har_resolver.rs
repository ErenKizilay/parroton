use crate::persistence::repo::Repository;
use har::v1_2::{Entries, Headers, Request};
use har::Spec;
use regex::Regex;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use uuid::Uuid;
use crate::action::model::Action;
use crate::assertion::model::{Assertion, AssertionItem, ComparisonType};
use crate::auth::model::{AuthHeaderValue, AuthenticationProvider};
use crate::case::model::TestCase;
use crate::har_resolver::FlattenKeyPrefixType::{AssertionExpression, Input, Output};
use crate::json_path::model::Expression;
use crate::parameter::model::{Parameter, ParameterLocation, ParameterType};

pub async fn build_test_case(
    repository: &Repository,
    spec: &Spec,
    customer_id: &String,
    test_case_name: &String,
    description: &String,
    excluded_path_parts: Vec<String>,
    auth_providers: Vec<String>,
) {
    let entries = filter_entries(excluded_path_parts, spec);
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
        id: Uuid::new_v4().to_string(),
        name: test_case_name.clone(),
        description: description.clone(),
    };
    let created_test_case = repository.test_cases().create_test_case(case).await;

    let mut actions = vec![];
    let existing_auth_providers = if auth_providers.is_empty() {
        vec![]
    } else {
        repository.auth_providers()
            .batch_get(customer_id, auth_providers)
            .await
            .unwrap_or(vec![])
    };
    let mut auth_headers_by_base_url: HashMap<String, Vec<HashMap<String, AuthHeaderValue>>> =
        HashMap::new();
    for i in 0..entries.len() {
        let current = entries.get(i).unwrap();
        println!("{:#?}", current.request.url);
        let action = build_action(i, &created_test_case, current);
        let input_parameters = build_action_input(&action, &current.request, &response_indexes);
        let output_parameters = build_output_parameters(&action, current);
        let assertions = build_assertions(&action, &request_indexes, &response_indexes);
        repository.assertions().batch_create(assertions).await;
        actions.push(action);
        repository.parameters().batch_create(input_parameters).await;
        repository
            .parameters()
            .batch_create(output_parameters)
            .await;
        let base_url = obtain_base_url(&current.request.url.as_str());
        let matched_provider = existing_auth_providers.iter()
            .find(|auth_provider| { auth_provider.base_url.eq(&base_url) });

        match matched_provider {
            None => {
                let auth_headers = build_auth_headers(&current.request);
                auth_headers_by_base_url
                    .entry(base_url)
                    .or_insert_with(Vec::new)
                    .push(auth_headers);
            }
            Some(auth_provider) => {
                repository.auth_providers()
                    .link(customer_id, &auth_provider.id, &created_test_case.id).await;
            }
        }
    }
    create_auth_providers(repository, created_test_case.clone(), &mut auth_headers_by_base_url).await;
    repository.actions().batch_create(actions).await;
}

pub fn filter_entries(excluded_path_parts: Vec<String>, spec: &Spec) -> Vec<&Entries> {
    println!("{:?}", excluded_path_parts.clone());
    println!("{:?}", excluded_path_parts.is_empty());
    match spec {
        Spec::V1_2(log_v1) => {
            let entries: Vec<&Entries> = log_v1
                .entries
                .iter()
                .filter(|entry| {
                    excluded_path_parts.is_empty()
                        || !excluded_path_parts
                            .iter()
                            .any(|part| entry.request.url.contains(part))
                })
                .collect();
            entries
        }
        Spec::V1_3(log_v2) => {
            vec![]
        }
    }
}

async fn create_auth_providers(
    repository: &Repository,
    created_test_case: TestCase,
    auth_headers_by_base_url: &mut HashMap<String, Vec<HashMap<String, AuthHeaderValue>>>,
) {
    let auth_providers = auth_headers_by_base_url
        .iter()
        .map(|(base_url, headers)| {
            let mut headers_by_name: HashMap<String, AuthHeaderValue> = HashMap::new();
            headers.iter().for_each(|map| {
                map.iter().for_each(|(k, v)| {
                    headers_by_name.insert(k.to_string(), v.clone());
                })
            });
            let mut test_case_ids = HashSet::new();
            test_case_ids.insert(created_test_case.id.clone());
            AuthenticationProvider {
                customer_id: created_test_case.customer_id.clone(),
                id: Uuid::new_v4().to_string(),
                base_url: base_url.clone(),
                headers_by_name: headers_by_name.clone(),
                linked_test_case_ids: test_case_ids,
            }
        })
        .collect::<Vec<AuthenticationProvider>>();
    repository
        .auth_providers()
        .batch_create_auth_providers(auth_providers)
        .await;
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
        let action_name = build_action_name(order, &entry.request);
        let response_value = serde_json::from_str::<Value>(&text).unwrap();
        build_response_index_from_value(&action_name, &response_value)
    })
}

pub fn build_response_index_from_value(
    action_name: &String,
    response_value: &Value,
) -> HashMap<String, Value> {
    build_index_from_value(action_name, response_value, Output)
}

pub fn build_request_index_from_value(
    action_name: &String,
    input_map: &Value,
) -> HashMap<String, Value> {
    build_index_from_value(action_name, input_map, AssertionExpression)
}

fn build_index_from_value(
    action_name: &String,
    response_value: &Value,
    flatten_key_prefix_type: FlattenKeyPrefixType,
) -> HashMap<String, Value> {
    let mut result = HashMap::<String, Value>::new();
    flatten_json_value(
        &action_name,
        &flatten_key_prefix_type,
        &response_value,
        "".to_string(),
        &mut result,
    );
    result
}

fn build_output_parameters(action: &Action, entry: &Entries) -> Vec<Parameter> {
    let response = &entry.response;
    let content = &response.content;
    let option = &content.text;
    let mut parameters = vec![];
    option.as_ref().iter().for_each(|text| {
        let response_value = serde_json::from_str::<Value>(&text).unwrap();
        parameters.extend(build_output_parameters_from_value(&action, &response_value));
    });
    parameters
}

pub enum FlattenKeyPrefixType {
    Output,
    Input,
    AssertionExpression,
}

pub fn build_output_parameters_from_value(
    action: &Action,
    response_value: &Value,
) -> Vec<Parameter> {
    let mut parameters = vec![];
    let mut result = HashMap::<String, Value>::new();
    flatten_json_value(
        &action.name,
        &Input,
        &response_value,
        "".to_string(),
        &mut result,
    );
    result.iter().for_each(|(key, value)| {
        let parameter = build_parameter(
            action,
            None,
            value.clone(),
            ParameterLocation::Body(key.to_string()),
            ParameterType::Output,
        );
        parameters.push(parameter);
    });
    parameters
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
                result = build_request_index_from_value(&action_name, &input_map);
            }
        }
    }
    result
}

fn build_action_name(order: usize, request: &Request) -> String {
    let url = &request.url;
    build_action_name_from_url(order, url)
}

pub fn build_action_name_from_url(order: usize, url: &String) -> String {
    let formatted_name = url.replace("-", "_");
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
    //query_params.extend(cookie_params);
    query_params
}

pub fn build_assertions(
    action: &Action,
    request_indexes: &Vec<HashMap<String, Value>>,
    response_indexes: &Vec<HashMap<String, Value>>,
) -> Vec<Assertion> {
    let mut assertions: Vec<Assertion> = vec![];

    response_indexes
        .get(action.order)
        .iter()
        .for_each(|response| {
            response.iter().for_each(|(path, res_value)| {
                if should_build_assertion_for_response_value(res_value) {
                    let mut slice = request_indexes[0..action.order].to_vec();
                    slice.reverse();
                    let expression_result =
                        resolve_value_expression_from_slice_index(&res_value, &slice);
                    if let Some(expression) = expression_result {
                        let assertion = Assertion {
                            customer_id: action.customer_id.clone(),
                            test_case_id: action.test_case_id.clone(),
                            id: Uuid::new_v4().to_string(),
                            left: AssertionItem::from_expression(expression),
                            right: AssertionItem::from_expression(Expression {
                                value: path.to_string(),
                            }),
                            comparison_type: ComparisonType::EqualTo,
                            negate: false,
                        };
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
                parameters.extend(build_body_parameters_from_value(
                    &action,
                    response_indexes,
                    &value,
                ));
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
                        ParameterType::Input,
                    );
                    parameters.push(parameter);
                })
        }
    });
    parameters
}

pub fn build_body_parameters_from_value(
    action: &Action,
    response_indexes: &Vec<HashMap<String, Value>>,
    value: &Value,
) -> Vec<Parameter> {
    let mut parameters: Vec<Parameter> = vec![];
    let mut flatten_result: HashMap<String, Value> = HashMap::new();
    flatten_json_value(
        &action.name,
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
            ParameterType::Input,
        );
        parameters.push(parameter);
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
        let query_string_value = &query_string.value;
        let query_key = &query_string.name;
        let parameter = build_query_param(action, response_indexes, query_string_value, query_key);
        parameters.push(parameter);
    });
    parameters
}

pub fn build_query_param(
    action: &Action,
    response_indexes: &Vec<HashMap<String, Value>>,
    query_string_value: &String,
    query_key: &String,
) -> Parameter {
    let expression = resolve_value_expression_from_prev(
        action.order,
        &Value::String(query_string_value.clone()),
        response_indexes,
    );
    let parameter = build_parameter(
        action,
        expression,
        Value::String(query_string_value.clone()),
        ParameterLocation::Query(query_key.clone()),
        ParameterType::Input,
    );
    parameter
}

fn build_parameter(
    action: &Action,
    expression: Option<Expression>,
    value: Value,
    location: ParameterLocation,
    parameter_type: ParameterType,
) -> Parameter {
    Parameter {
        customer_id: action.customer_id.clone(),
        test_case_id: action.test_case_id.clone(),
        action_id: action.id.clone(),
        value_expression: expression,
        id: uuid::Uuid::new_v4().to_string(),
        parameter_type: parameter_type,
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
        if let Some(parameter) = build_header_parameter(
            action,
            response_indexes,
            &resolve_header_name(header),
            &header.value,
        ) {
            parameters.push(parameter);
        }
    });
    parameters
}

fn build_header_parameter(
    action: &Action,
    response_indexes: &Vec<HashMap<String, Value>>,
    header_name: &String,
    header_val: &String,
) -> Option<Parameter> {
    if is_auth_related_header(&header_name) {
        None
    } else {
        let expression = resolve_value_expression_from_prev(
            action.order,
            &Value::String(header_val.clone()),
            response_indexes,
        );

        Some(build_parameter(
            action,
            expression,
            Value::String(header_val.clone()),
            ParameterLocation::Header(header_name.clone()),
            ParameterType::Input,
        ))
    }
}

fn build_auth_headers(request: &Request) -> HashMap<String, AuthHeaderValue> {
    let mut auth_headers_by_name: HashMap<String, AuthHeaderValue> = HashMap::new();
    request
        .headers
        .iter()
        .filter(|header| is_auth_related_header(&header.name))
        .for_each(|header| {
            auth_headers_by_name.insert(
                resolve_header_name(header),
                AuthHeaderValue {
                    value: header.value.clone(),
                    disabled: false,
                },
            );
        });
    auth_headers_by_name
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
            ParameterType::Input,
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
                        Output => {
                            format!("$.{}.{}.{}", action_name, "output", key)
                        }
                        Input => {
                            format!("$.{}", key)
                        }
                        AssertionExpression => {
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

fn is_auth_related_header(key: &String) -> bool {
    vec![
        "authorization",
        "token",
        "session",
        "csrf",
        "user",
        "origin",
        "cookie",
        "auth",
    ]
    .iter()
    .any(|x| key.contains(x))
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

fn resolve_header_name(header: &Headers) -> String {
    header.name.replace(":", "")
}
