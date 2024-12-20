use crate::models::{Parameter, ParameterType};
use crate::repo::Repository;
use futures::FutureExt;
use regex::Regex;
use serde::Deserialize;
use serde_json::{Map, Value};
use serde_json_path::JsonPath;

#[derive(Deserialize)]
pub struct AutoCompleteRequest {
    customer_id: String,
    test_case_id: String,
    source_action_id: String,
    source_action_name: String,
    source_action_order: usize,
    latest_input: String,
}

#[derive(Debug)]
enum SuggestionStrategy {
    ActionNames,
    InputOrOutput,
    JsonPath,
}

pub async fn auto_complete(repository: &Repository, request: AutoCompleteRequest) -> Vec<String> {
    let regexes: Vec<(SuggestionStrategy, Regex)> = vec![
        (
            SuggestionStrategy::ActionNames,
            Regex::new(r"^(\$.)$").unwrap(),
        ),
        (
            SuggestionStrategy::InputOrOutput,
            Regex::new(r"(^\$\.([a-zA-Z0-9_]+)$)").unwrap(),
        ),
        (
            SuggestionStrategy::JsonPath,
            Regex::new(r"(output|input)").unwrap(),
        ),
    ];

    for (strategy, regex) in regexes {
        if regex.is_match(&request.latest_input) {
            println!("matched strategy: {:?} for input {}", strategy, request.latest_input);
            let suggestions = match strategy {
                SuggestionStrategy::ActionNames => repository
                    .list_previous_actions(
                        request.customer_id.clone(),
                        request.test_case_id.clone(),
                        request.source_action_order,
                        None,
                    )
                    .await
                    .items
                    .iter()
                    .map(|a| a.name.clone())
                    .collect(),
                SuggestionStrategy::InputOrOutput => {
                    vec!["input".to_string(), "output".to_string()]
                }
                SuggestionStrategy::JsonPath => {
                    let param_type = if request.latest_input.contains("output.") {
                        ParameterType::Output
                    } else {
                        ParameterType::Input
                    };

                    let target_action_name = substring_between(
                        request.latest_input.clone(),
                        "$.".to_string(),
                        ".".to_string(),
                    );
                    println!("target_action_name: {}", target_action_name);
                    let target_action = repository
                        .get_action_by_name(
                            request.customer_id.clone(),
                            request.test_case_id.clone(),
                            target_action_name,
                        )
                        .await
                        .unwrap();
                    repository
                        .query_parameters_of_action_by_path(
                            request.customer_id.clone(),
                            request.test_case_id.clone(),
                            target_action.id,
                            param_type,
                            remove_prefix(&request.latest_input, "^((.*).(output|input)\\.)"),
                            None,
                        )
                        .await
                        .items
                        .iter()
                        .map(|p| p.get_path()
                        )
                        .collect()
                }
            };
            return suggestions;
        } else { println!("no suggestion for input {}", request.latest_input); }
    }
    vec![]
}

fn substring_between(input: String, start: String, end: String) -> String {
    // First, split the string into two parts based on the start substring
    input
        .split_once(start.as_str())
        .and_then(|(_, after_start)| {
            // Then, split the resulting string again based on the end substring
            after_start
                .split_once(end.as_str())
                .map(|(before_end, _)| before_end)
        })
        .unwrap()
        .to_string()
}

pub fn evaluate_value(parameter: &Parameter, context: &Value) -> Result<Value, String> {
    let result = match &parameter.value_expression {
        None => Ok(parameter.value.clone()),
        Some(exp) => {
            let json_path = JsonPath::parse(exp.value.as_str()).unwrap();
            let node_list = json_path.query(context);
            if parameter.value.is_array() {
                let values: Vec<Value> = node_list
                    .all()
                    .iter()
                    .cloned()
                    .map(|node| node.clone())
                    .collect();
                Ok(Value::Array(values))
            } else {
                match node_list.exactly_one() {
                    Ok(val) => Ok(val.clone()),
                    Err(err) => Err(err.to_string()),
                }
            }
        }
    };
    result
}

pub fn reverse_flatten_all(path_value_pairs: Vec<(String, Value)>) -> Value {
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

fn remove_prefix(s: &String, pattern: &str) -> String {
    let regex = Regex::new("^((.*).(output|input)\\.)").unwrap();
    println!("s: [{}] p: {}", s, pattern);
    format!(
        "$.{}",
        regex
            .captures(s.as_str())
            .iter()
            .map(|caps| {
                println!("caps: {:?}", caps);
                println!("cap 1: {:?}", caps.get(1).unwrap().as_str());
                s.strip_prefix(caps.get(1).unwrap().as_str().trim_matches('"'))
                    .unwrap_or(s.as_str())
            })
            .next()
            .unwrap_or("")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    // Import the current module's items for testing.
    #[test]
    fn test() {
        let string = "$.apiIntegrations_0.output.asdasd".to_string();
        let regex = Regex::new("^((.*).(output|input)\\.)").unwrap();
        let x = regex
            .captures(string.as_str())
            .iter()
            .map(|caps| {
                println!("caps: {:?}", caps);
                println!("cap 1: {:?}", caps.get(1).unwrap().as_str());
                string
                    .strip_prefix(caps.get(1).unwrap().as_str().trim_matches('"'))
                    .unwrap_or(string.as_str())
            })
            .next()
            .unwrap_or("");
        println!("result: {:?}", x);
    }
}
