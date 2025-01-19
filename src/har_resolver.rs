use crate::action::model::Action;
use crate::assertion::model::{Assertion, AssertionItem, ComparisonType};
use crate::auth::model::{AuthHeaderValue, AuthenticationProvider};
use crate::case::model::TestCase;
use crate::har_resolver::FlattenKeyPrefixType::{AssertionExpression, Input, Output};
use crate::json_path::model::Expression;
use crate::parameter::model::{Parameter, ParameterLocation, ParameterType};
use crate::persistence::repo::Repository;
use har::v1_2::{Entries, Headers, PostData, Request};
use har::Spec;
use regex::Regex;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use tracing::{info, warn};
use uuid::Uuid;

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

    let case = TestCase::builder()
        .customer_id(customer_id.clone())
        .name(test_case_name.clone())
        .description(description.clone())
        .build();
    let created_test_case = repository.test_cases().create(case).await;

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
        let action = build_action(i, &created_test_case, current, &response_indexes);
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
    let exclusions: Vec<String> = excluded_path_parts.iter()
        .map(|s| s.trim().to_string())
        .collect();
    info!("{:?}", excluded_path_parts.clone());
    match spec {
        Spec::V1_2(log_v1) => {
            let entries: Vec<&Entries> = log_v1
                .entries
                .iter()
                .filter(|entry| {
                    exclusions.is_empty()
                        || !exclusions
                        .iter()
                        .any(|part| entry.request.url.contains(part))
                })
                .filter(|entry| {
                    let request = &entry.request;
                    match &request.post_data {
                        None => {
                            true
                        }
                        Some(post_data) => {
                            post_data.mime_type.contains("json") || post_data.mime_type.contains("form-urlencoded")
                        }
                    }
                })
                .filter(|entry| {
                    let response = &entry.response;
                    let mime_type_opt = response.content.mime_type.clone();
                    mime_type_opt.map_or(true, |mime_type| mime_type.contains("json"))
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
            AuthenticationProvider::builder()
                .customer_id(created_test_case.customer_id.clone())
                .name(build_auth_name_from_url(base_url))
                .base_url(base_url.clone())
                .headers_by_name(headers_by_name)
                .linked_test_case_ids(test_case_ids)
                .build()
        })
        .collect::<Vec<AuthenticationProvider>>();
    repository
        .auth_providers()
        .batch_create(auth_providers)
        .await;
}

fn build_auth_name_from_url(base_url: &String) -> String {
    let re = Regex::new(r"[./]").unwrap();
    let parts: Vec<&str> = re.split(base_url).collect();
    let mut name: String = "".to_string();
    for i in 1..parts.len() - 1 {
        name.push_str(format!(" {}", parts[i]).as_str());
    }
    name.trim().to_string()
}

fn build_action(order: usize, test_case: &TestCase, entry: &Entries, response_indexes: &Vec<HashMap<String, Value>>) -> Action {
    let action_name = build_action_name(order, &entry.request);
    Action::builder()
        .customer_id(test_case.customer_id.clone())
        .test_case_id(test_case.id.clone())
        .order(order)
        .name(action_name.clone())
        .maybe_mime_type(resolve_mime_type(entry))
        .method(entry.request.method.clone())
        .url(build_url_without_query_params(order, &entry.request.url, response_indexes))
        .build()
}

fn build_url_without_query_params(order: usize, url: &String, response_indexes: &Vec<HashMap<String, Value>>) -> String {
    let re = Regex::new(r"\?.*$").unwrap();
    let url = re.replace(url, "").to_string();
    let base_url = obtain_base_url(url.as_str());

    let path = url.clone().replace(base_url.as_str(), "");

    println!("path: {:#?}", path);

    let path_with_expressions = path.split("/")
        .map(|s| {
            if s.is_empty() {
                "".to_string()
            } else {
                resolve_value_expression_from_prev(order, &Value::String(s.to_string()), response_indexes)
                    .map_or(s.to_string(), |expression: Expression| { expression.value })
            }
        })
        .collect::<Vec<String>>()
        .join("/");
    format!("{}{}", base_url, path_with_expressions)
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
        info!("building response index for: {:?} and mime_type: {:?} content: {:?}", action_name, content.mime_type, text);
        match serde_json::from_str::<Value>(&text) {
            Ok(response_value) => {
                build_response_index_from_value(&action_name, &response_value)
            }
            Err(e) => {
                warn!("Empty index will be created for action: {:?} and mime_type: {:?} content: {:?}", action_name, content.mime_type, e);
                HashMap::new()
            }
        }
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
        match serde_json::from_str::<Value>(&text) {
            Ok(response_value) => {
                parameters.extend(build_output_parameters_from_value(&action, &response_value));
            }
            Err(e) => {
                warn!("Will not create output parameters for action: {:?} and mime_type: {:?} and content: {:?}, error: {:?}", action.name, content.mime_type, text, e);
            }
        }
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
    info!("building action name for: {:?}", url);
    build_action_name_from_url(order, url)
}

pub fn build_action_name_from_url(order: usize, url: &String) -> String {
    let formatted_name = url.replace("-", "_");
    let base_name = formatted_name.split("/").last().unwrap();
    let re = Regex::new(r"\?.*$").unwrap(); // Matches '?' and everything after it
    let suffix = re.find_iter(base_name)
        .filter(|m| !m.is_empty())
        .map(|m| m.as_str().to_string().split("=")
            .last()
            .map_or_else(|| "".to_string(), |v| v.to_string()))
        .filter(|v| !v.is_empty())
        .filter(|s| !s.chars().all(char::is_numeric))
        .map(|v| v.chars().map(|c|{
            if c.is_uppercase() {
                format!("_{}", c.to_lowercase())
            } else {
                c.to_string()
            }
        }).collect::<Vec<String>>().join(""))
        .take(2)
        .collect::<Vec<String>>()
        .join("_");
    if suffix.is_empty() {
        format!("{}_{}", re.replace(base_name, "").to_string(), order)
    } else {
        format!("{}{}_{}", re.replace(base_name, "").to_string(), suffix, order)
    }
}

fn build_action_input(
    action: &Action,
    request: &Request,
    response_indexes: &Vec<HashMap<String, Value>>,
) -> Vec<Parameter> {
    let mut query_params = build_query_parameters(action, request, response_indexes);
    let body_params = build_body_parameters(action, request, response_indexes);
    let header_params = build_header_parameters(action, request, response_indexes);
    query_params.extend(body_params);
    query_params.extend(header_params);
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
                        let assertion = Assertion::builder()
                            .customer_id(action.customer_id.clone())
                            .test_case_id(action.test_case_id.clone())
                            .left(AssertionItem::from_expression(expression))
                            .right(AssertionItem::from_expression(Expression {
                                value: path.to_string(),
                            }))
                            .comparison_type(ComparisonType::EqualTo)
                            .negate(false)
                            .build();
                        assertions.push(assertion);
                    }
                }
            })
        });
    assertions
}

fn should_build_assertion_for_response_value(res_value: &Value) -> bool {
    let non_assertable_value = res_value.is_boolean()
        || res_value.is_null()
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
    Parameter::builder()
        .customer_id(action.customer_id.clone())
        .test_case_id(action.test_case_id.clone())
        .action_id(action.id.clone())
        .maybe_value_expression(expression)
        .parameter_type(parameter_type)
        .location(location)
        .value(value)
        .build()
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
    if is_auth_related_header(&header_name) || must_exclude_header(&header_name) {
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
                AuthHeaderValue::builder()
                    .value(header.value.clone())
                    .build(),
            );
        });
    println!("cookies: {:?}", request.cookies);
    request.cookies.iter()
        .filter(|cookie| is_auth_related_header(&cookie.name))
        .for_each(|cookie| {
            info!("cookie: {} value: {}", cookie.name, cookie.value);
            auth_headers_by_name.insert(
                cookie.name.clone(),
                AuthHeaderValue::builder()
                    .value(cookie.value.clone())
                    .build(),
            );
        });
    auth_headers_by_name
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

fn must_exclude_header(key: &String) -> bool {
    vec![
        "content-length",
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


#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn build_header_name() {
        let provider_name = build_auth_name_from_url(&String::from("https://layima.app.opsgenie.com"));
        assert_eq!("layima app opsgenie", provider_name.as_str());
    }

    #[tokio::test]
    async fn build_auth_headers_test() {
        let har = har::from_path("resources/test/layima.atlassian.net.har").unwrap();
        let spec = har.log;
        match spec {
            Spec::V1_2(log) => {
                log.entries.iter()
                    .for_each(|entries: &Entries| {
                        let map = build_auth_headers(&entries.request);
                        println!("{:#?}", map);
                    })
            }
            Spec::V1_3(_) => {}
        }
    }

    #[tokio::test]
    async fn build_action_url() {
        let action0_index = HashMap::from([(String::from("$.action0.output.issueKey"), Value::String(String::from("TEST-1")))]);
        let response_indexes: Vec<HashMap<String, Value>> = Vec::from([action0_index]);
        let actual = build_url_without_query_params(1, &"https://abc.xyz/TEST-1/comment".to_string(), &response_indexes);
        assert_eq!("https://abc.xyz/$.action0.output.issueKey/comment", actual.as_str());
    }

    #[tokio::test]
    async fn test_build_action_url_with_params() {
        let action0_index = HashMap::from([(String::from("$.action0.output.issueKey"), Value::String(String::from("")))]);
        let response_indexes: Vec<HashMap<String, Value>> = Vec::from([action0_index]);
        let actual = build_url_without_query_params(1, &"https://layima.atlassian.net/rest/dev-status/1.0/issue/create-branch-targets?issueId=10000".to_string(), &response_indexes);
        assert_eq!("https://layima.atlassian.net/rest/dev-status/1.0/issue/create-branch-targets", actual.as_str());
    }

    #[tokio::test]
    async fn test_build_action_name() {
        let actual = build_action_name_from_url(1, &"https://layima.atlassian.net/jsw2/graphql?operation=BoardCardCreate".to_string());
        assert_eq!("graphql_board_card_create_1", actual.as_str());
    }
}
