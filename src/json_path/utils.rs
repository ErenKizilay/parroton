use crate::parameter::model::{Parameter, ParameterType};
use crate::persistence::repo::Repository;
use regex::Regex;
use serde::Deserialize;
use serde_json::{Map, Value};
use serde_json_path::JsonPath;
use crate::json_path::api::AutoCompleteRequest;
use crate::json_path::model::Expression;

#[derive(Debug, PartialEq)]
enum SuggestionStrategy {
    ActionNames,
    InputOrOutput,
    JsonPath,
}

pub async fn auto_complete(repository: &Repository, request: AutoCompleteRequest) -> Vec<String> {
    let strategy_option = crate::json_path::utils::find_matching_suggestion_strategy(&request.latest_input);
    println!("input: {:?}, stg: {:?}", request.latest_input, strategy_option);
    match strategy_option {
        None => {
            vec![]
        }
        Some(strategy) => {
            match strategy {
                crate::json_path::utils::SuggestionStrategy::ActionNames => repository
                    .actions()
                    .list_previous(
                        request.customer_id.clone(),
                        request.test_case_id.clone(),
                        request.source_action_order.unwrap_or(1000),
                        None,
                    )
                    .await
                    .unwrap()
                    .items
                    .iter()
                    .map(|a| format!("$.{}", a.name))
                    .collect(),
                crate::json_path::utils::SuggestionStrategy::InputOrOutput => {
                    let input_parts = request.latest_input.split(".").collect::<Vec<&str>>();
                    vec![format!("{}.{}.{}", input_parts[0], input_parts[1], "input".to_string()),
                         format!("{}.{}.{}", input_parts[0], input_parts[1], "output".to_string())]
                }
                crate::json_path::utils::SuggestionStrategy::JsonPath => {
                    let param_type = if request.latest_input.contains("output.") {
                        ParameterType::Output
                    } else {
                        ParameterType::Input
                    };

                    let target_action_name = crate::json_path::utils::substring_between(
                        request.latest_input.clone(),
                        "$.".to_string(),
                        ".".to_string(),
                    );
                    let target_action = repository
                        .actions()
                        .get_action_by_name(
                            request.customer_id.clone(),
                            request.test_case_id.clone(),
                            target_action_name,
                        )
                        .await
                        .unwrap();
                    let suffix = crate::json_path::utils::remove_prefix(&request.latest_input);
                    let input_parts = request.latest_input.split(".").collect::<Vec<&str>>();
                    let result_prefix = format!("{}.{}.{}", input_parts[0], input_parts[1], input_parts[2]);
                    repository
                        .parameters()
                        .query_by_path(
                            request.customer_id.clone(),
                            request.test_case_id.clone(),
                            target_action.id,
                            param_type,
                            suffix.clone(),
                            None,
                        )
                        .await
                        .unwrap()
                        .items
                        .iter()
                        .map(|p| format!("{}.{}", result_prefix, p.get_path().replace("$.", "")))
                        .collect()
                }
            }
        }
    }
}

fn substring_between(input: String, start: String, end: String) -> String {
    input
        .split_once(start.as_str())
        .and_then(|(_, after_start)| {
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
            let eval_result = evaluate_expression(context, exp);
            match eval_result {
                Ok(values) => {
                    if parameter.value.is_array() {
                        Ok(Value::Array(values))
                    } else {
                        match values.iter().next() {
                            Some(val) => Ok(val.clone()),
                            None => Err(format!("expression \"{}\" produces empty result", exp.value)),
                        }
                    }
                }
                Err(err) => {
                    Err(err)
                }
            }
        }
    };
    result
}

pub fn evaluate_expression(context: &Value, exp: &Expression) -> Result<Vec<Value>, String> {
    let json_path_result = JsonPath::parse(exp.value.as_str());
    match json_path_result {
        Ok(json_path) => {
            Ok(json_path.query(context)
                .all()
                .iter()
                .cloned()
                .map(|node| node.clone())
                .collect())
        }
        Err(err) => {
            Err(err.to_string())
        }
    }
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

fn remove_prefix(s: &String) -> String {
    let regex = Regex::new("^((.*).(output|input)\\.)").unwrap();
    format!(
        "$.{}",
        regex
            .captures(s.as_str())
            .iter()
            .map(|caps| {
                s.strip_prefix(caps.get(1).unwrap().as_str().trim_matches('"'))
                    .unwrap_or(s.as_str())
            })
            .next()
            .unwrap_or("")
    )
}

fn find_matching_suggestion_strategy(input: &String) -> Option<SuggestionStrategy> {
    let dot_count = input.chars().filter(|c| *c == '.').count();
    match dot_count {
        0 => None,
        1 => Some(SuggestionStrategy::ActionNames),
        2 => Some(SuggestionStrategy::InputOrOutput),
        _ => Some(SuggestionStrategy::JsonPath),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn auto_complete_matching_strategy() {
        let input1 = String::from("$.");
        let input2 = String::from("$.action");
        let input3 = String::from("$.action.");
        let input4 = String::from("$.action.out");
        let input5 = String::from("$.action.output.");
        let input6 = String::from("$.action.output.param");

        let actual1 = find_matching_suggestion_strategy(&input1);
        assert_eq!(actual1, Some(SuggestionStrategy::ActionNames));

        let actual2 = find_matching_suggestion_strategy(&input2);
        assert_eq!(actual2, Some(SuggestionStrategy::ActionNames));

        let actual3 = find_matching_suggestion_strategy(&input3);
        assert_eq!(actual3, Some(SuggestionStrategy::InputOrOutput));

        let actual4 = find_matching_suggestion_strategy(&input4);
        assert_eq!(actual4, Some(SuggestionStrategy::InputOrOutput));

        let actual5 = find_matching_suggestion_strategy(&input5);
        assert_eq!(actual5, Some(SuggestionStrategy::JsonPath));

        let actual6 = find_matching_suggestion_strategy(&input6);
        assert_eq!(actual6, Some(SuggestionStrategy::JsonPath));
    }
}
