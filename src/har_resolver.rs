use crate::models::FlattenKeyPrefixType::{AssertionExpression, Input, Output};
use crate::models::{Action, Assertion, AssertionItem, AuthHeaderValue, AuthenticationProvider, ComparisonType, Expression, FlattenKeyPrefixType, Parameter, ParameterLocation, ParameterType, TestCase};
use crate::persistence::repo::Repository;
use har::v1_2::{Entries, Headers, Request};
use har::Spec;
use regex::Regex;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

pub async fn build_test_case(
    repository: &Repository,
    spec: &Spec,
    customer_id: &String,
    test_case_name: &String,
    description: &String,
    excluded_path_parts: Vec<String>,
) {
    match spec {
        Spec::V1_2(log_v1) => {
            let entries: Vec<&Entries> = log_v1
                .entries
                .iter()
                .filter(|entry| {
                    !excluded_path_parts
                        .iter()
                        .any(|part| entry.request.url.contains(part))
                })
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
                description: description.clone(),
            };
            let created_test_case = repository.test_cases().create_test_case(case).await;

            let mut actions = vec![];
            let mut auth_headers_by_base_url: HashMap<
                String,
                Vec<HashMap<String, AuthHeaderValue>>,
            > = HashMap::new();
            for i in 0..entries.len() {
                let current = entries.get(i).unwrap();
                println!("{:#?}", current.request.url);
                let action = build_action(i, &created_test_case, current);
                let input_parameters =
                    build_action_input(&action, &current.request, &response_indexes);
                let output_parameters = build_output_parameters(&action, current);
                let assertions = build_assertions(&action, i, &request_indexes, &response_indexes);
                repository.assertions().batch_create(assertions).await;
                actions.push(action);
                repository.parameters().batch_create(input_parameters).await;
                repository.parameters().batch_create(output_parameters).await;
                let auth_headers = build_auth_headers(&current.request);
                let base_url = obtain_base_url(&current.request.url.as_str());
                auth_headers_by_base_url
                    .entry(base_url)
                    .or_insert_with(Vec::new)
                    .push(auth_headers);
            }
            create_auth_providers(repository, created_test_case, &mut auth_headers_by_base_url).await;
            repository.actions().batch_create(actions).await;
        }
        Spec::V1_3(_) => {}
    }
}

async fn create_auth_providers(repository: &Repository, created_test_case: TestCase, auth_headers_by_base_url: &mut HashMap<String, Vec<HashMap<String, AuthHeaderValue>>>) {
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
    repository.auth_providers().batch_create_auth_providers(auth_providers).await;
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

fn build_output_parameters(action: &Action, entry: &Entries) -> Vec<Parameter> {
    let response = &entry.response;
    let content = &response.content;
    let option = &content.text;
    let mut parameters = vec![];
    option.as_ref().iter().for_each(|text| {
        let mut result = HashMap::<String, Value>::new();
        let response_value = serde_json::from_str::<Value>(&text).unwrap();
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
    //query_params.extend(cookie_params);
    query_params
}

fn build_assertions(
    action: &Action,
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
                    let assertion = Assertion {
                        customer_id: action.customer_id.clone(),
                        test_case_id: action.test_case_id.clone(),
                        action_id: action.id.clone(),
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
                        ParameterType::Input,
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
                        ParameterType::Input,
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
            ParameterType::Input,
        ));
    });
    parameters
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
    request
        .headers
        .iter()
        .filter(|header| !is_auth_related_header(&header.name))
        .for_each(|header| {
            let expression = resolve_value_expression_from_prev(
                action.order,
                &Value::String(header.value.clone()),
                response_indexes,
            );
            parameters.push(build_parameter(
                action,
                expression,
                Value::String(header.value.clone()),
                ParameterLocation::Header(resolve_header_name(header)),
                ParameterType::Input,
            ));
        });
    parameters
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
