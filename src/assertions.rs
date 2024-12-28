use crate::json_path_utils::evaluate_expression;
use crate::models::{Assertion, AssertionItem, AssertionResult, ComparisonType, Function, Operation, ValueProvider};
use serde_json::Value;

trait ValueSupplier {
    fn supply(&self, context: &Value) -> Result<Vec<Value>, String>;
}

impl ValueSupplier for ValueProvider {
    fn supply(&self, context: &Value) -> Result<Vec<Value>, String> {
        match &self.value {
            None => {
                match &self.expression {
                    None => {
                        Ok(vec![])
                    }
                    Some(exp) => {
                        evaluate_expression(context, exp)
                    }
                }
            }
            Some(val) => {
                Ok(vec![val.clone()])
            }
        }
    }
}

impl ValueSupplier for Function {
    fn supply(&self, context: &Value) -> Result<Vec<Value>, String> {
        let value_results: Vec<Result<Vec<Value>, String>> = self.parameters.iter()
            .map(|vp: &ValueProvider| { vp.supply(context) })
            .collect();
        if value_results.iter().any(|v| v.is_err()) {
            Err(value_results.iter()
                .filter(|v| v.is_err())
                .map(|v| v.clone().err().unwrap())
                .reduce(|e1, e2| { format!("{},{}", e1, e2) })
                .unwrap_or("".to_string()))
        } else {
            let value_list: Vec<Vec<Value>> = value_results.iter()
                .filter(|v| v.is_ok())
                .map(|v| v.clone().unwrap())
                .collect();
            match &self.operation {
                Operation::Sum => {
                    let sum_result = value_list
                        .iter()
                        .map(|v| { calculate_sum(v.clone()) })
                        .reduce(|a, b| { a + b })
                        .unwrap_or(0.0);
                    Ok(vec![Value::from(sum_result)])
                }
                Operation::Avg => {
                    Ok(vec![Value::Null])
                }
                Operation::Count => {
                    Ok(vec![])
                }
            }
        }
    }
}

fn sum(v1: Vec<Value>, v2: Vec<Value>) -> f64 {
    calculate_sum(v1) + calculate_sum(v2)
}

fn calculate_sum(v1: Vec<Value>) -> f64 {
    v1.iter()
        .map(|i1| {
            i1.as_number()
                .map(|n| { n.as_f64().unwrap_or(0.0) })
                .iter()
                .map(|i2| { i2.clone() })
                .reduce(|a, b| { a.clone() + b.clone() })
                .unwrap_or(0.0)
        }).reduce(|a, b| { a + b }).unwrap_or(0.0)
        .clone()
}

impl ValueSupplier for AssertionItem {
    fn supply(&self, context: &Value) -> Result<Vec<Value>, String> {
        match &self.function {
            None => {
                match &self.value_provider {
                    None => {
                        Err("either function, expression or value must be provided!".to_string())
                    }
                    Some(val_provider) => {
                        val_provider.supply(context)
                    }
                }
            }
            Some(function) => {
                function.supply(context)
            }
        }
    }
}

pub fn check_assertion(assertion: &Assertion, context: &Value) -> AssertionResult {
    let left_result = assertion.left.supply(context);
    match left_result {
        Ok(left_val) => {
            let right_result = assertion.right.supply(context);
            match right_result {
                Ok(right_val) => {
                    check(&assertion.comparison_type, assertion.negate, left_val, right_val)
                }
                Err(err) => { AssertionResult::from_error(err) }
            }
        }
        Err(err) => {
            AssertionResult::from_error(err)
        }
    }
}

fn as_string(val: Vec<Value>) -> String {
    val.iter()
        .map(|i| { i.to_string().trim_matches('"').to_string() })
        .reduce(|s1, s2| { format!("{},{}", s1, s2) })
        .unwrap_or("".to_string())
}

fn check(comparison_type: &ComparisonType, negate: bool, left: Vec<Value>, right: Vec<Value>) -> AssertionResult {
    match comparison_type {
        ComparisonType::EqualTo => {
            let equals = left.eq(&right);
            if equals ^ negate {
                AssertionResult::of_success()
            } else {
                AssertionResult::from_error(format!("{}expected: {:?}, but got: {:?}",
                                                    if negate { "not " } else { "" },
                                                    as_string(left), as_string(right)))
            }
        }
        ComparisonType::Contains => {
            let all_contained = right.iter().all(|v| { left.contains(&v) });
            if all_contained {
                AssertionResult::of_success()
            } else {
                if left.len() == right.len() && left.len() == 1 {
                    let left_item = left.get(0).unwrap();
                    let right_item = right.get(0).unwrap();
                    let contains = left_item.to_string().contains(right_item.to_string().trim_matches('"'));
                    if contains ^ negate {
                        AssertionResult::of_success()
                    } else {
                        AssertionResult::from_error(format!("{} does{} contain {}",
                                                            as_string(left), if negate { "" } else { " not" }, as_string(right), ))
                    }
                } else {
                    AssertionResult::from_error(format!("{} and {} cannot be compared with contains", as_string(left), as_string(right)))
                }
            }
        }
        ComparisonType::GreaterThan => {
            check_greater_than(negate, true, false, left, right)
        }
        ComparisonType::GreaterThanOrEqualTo => {
            check_greater_than(negate, true, true, left, right)
        }
        ComparisonType::LessThan => {
            check_greater_than(negate, false, false, left, right)
        }
        ComparisonType::LessThanOrEqualTo => {
            check_greater_than(negate, false, true, left, right)
        }
    }
}

fn check_greater_than(negate: bool, greater: bool, or_equal: bool, left: Vec<Value>, right: Vec<Value>) -> AssertionResult {
    if left.len() == right.len() && left.len() == 1 {
        let left_item = left.get(0).unwrap().as_number();
        let right_item = right.get(0).unwrap().as_number();
        if left_item.is_some() && right_item.is_some() {
            let success = if greater {
                if or_equal { left_item.unwrap().as_f64().ge(&right_item.unwrap().as_f64()) } else { left_item.unwrap().as_f64().gt(&right_item.unwrap().as_f64()) }
            } else {
                if or_equal { left_item.unwrap().as_f64().le(&right_item.unwrap().as_f64()) } else { left_item.unwrap().as_f64().lt(&right_item.unwrap().as_f64()) }
            };
            if success ^ negate {
                AssertionResult::of_success()
            } else {
                AssertionResult::from_error(format!("{} is{} {} than {} {}",
                                                    as_string(left), if negate { "" } else { " not" },
                                                    if greater {"greater"} else {"less"}, if or_equal {"or equal to"} else {""}, as_string(right)))
            }
        } else {
            AssertionResult::from_error(format!("{} and {} cannot be compared as numbers", as_string(left), as_string(right)))
        }
    } else {
        AssertionResult::from_error("Lists cannot be compared as numbers!".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Expression;
    use serde_json::json;
    #[test]
    fn equality_check() {
        let assertion = Assertion {
            customer_id: "".to_string(),
            test_case_id: "".to_string(),
            action_id: "".to_string(),
            id: "".to_string(),
            left: AssertionItem::from_expression(Expression { value: "$.action1.output.message".to_string() }),
            right: AssertionItem::from_value(Value::String("a message".to_string())),
            comparison_type: ComparisonType::EqualTo,
            negate: false,
        };
        let context = serde_json::to_value(json!({
        "action1": {
                "output": {
                    "message": "a message"
                },
            },
        "location": "Menlo Park, CA",
    })).unwrap();
        let result = check_assertion(&assertion, &context);
        println!("{:?}", result.message);
        assert_eq!(result.success, true);
    }

    #[test]
    fn negate_equality_check() {
        let assertion = Assertion {
            customer_id: "".to_string(),
            test_case_id: "".to_string(),
            action_id: "".to_string(),
            id: "".to_string(),
            left: AssertionItem::from_expression(Expression { value: "$.action1.output.message".to_string() }),
            right: AssertionItem::from_value(Value::String("a message".to_string())),
            comparison_type: ComparisonType::EqualTo,
            negate: true,
        };
        let context = serde_json::to_value(json!({
        "action1": {
                "output": {
                    "message": "a message"
                },
            },
        "location": "Menlo Park, CA",
    })).unwrap();
        let result = check_assertion(&assertion, &context);
        println!("{:?}", result.message);
        assert_eq!(result.success, false);
    }

    #[test]
    fn string_contains() {
        let assertion = Assertion {
            customer_id: "".to_string(),
            test_case_id: "".to_string(),
            action_id: "".to_string(),
            id: "".to_string(),
            left: AssertionItem::from_expression(Expression { value: "$.action1.output.message".to_string() }),
            right: AssertionItem::from_value(Value::String("message".to_string())),
            comparison_type: ComparisonType::Contains,
            negate: false,
        };
        let context = serde_json::to_value(json!({
        "action1": {
                "output": {
                    "message": "a message"
                },
            },
        "location": "Menlo Park, CA",
    })).unwrap();
        let result = check_assertion(&assertion, &context);
        println!("{:?}", result.message);
        assert_eq!(result.success, true);
    }

    #[test]
    fn list_contains() {
        let assertion = Assertion {
            customer_id: "".to_string(),
            test_case_id: "".to_string(),
            action_id: "".to_string(),
            id: "".to_string(),
            left: AssertionItem::from_expression(Expression { value: "$.action1.output.messages".to_string() }),
            right: AssertionItem::from_value(Value::String("a message".to_string())),
            comparison_type: ComparisonType::Contains,
            negate: false,
        };
        let context = serde_json::to_value(json!({
        "action1": {
                "output": {
                    "messages": ["a message", "another message"]
                },
            },
        "location": "Menlo Park, CA",
    })).unwrap();
        let result = check_assertion(&assertion, &context);
        println!("{:?}", result.message);
        assert_eq!(result.success, true);
    }

    #[test]
    fn greater_than() {
        let assertion = Assertion {
            customer_id: "".to_string(),
            test_case_id: "".to_string(),
            action_id: "".to_string(),
            id: "".to_string(),
            left: AssertionItem::from_expression(Expression { value: "$.action1.output.count".to_string() }),
            right: AssertionItem::from_value(json!(5)),
            comparison_type: ComparisonType::GreaterThan,
            negate: false,
        };
        let context = serde_json::to_value(json!({
        "action1": {
                "output": {
                    "count": 17
                },
            },
        "location": "Menlo Park, CA",
    })).unwrap();
        let result = check_assertion(&assertion, &context);
        println!("{:?}", result.message);
        assert_eq!(result.success, true);
    }

    #[test]
    fn less_than_fail_case() {
        let assertion = Assertion {
            customer_id: "".to_string(),
            test_case_id: "".to_string(),
            action_id: "".to_string(),
            id: "".to_string(),
            left: AssertionItem::from_expression(Expression { value: "$.action1.output.count".to_string() }),
            right: AssertionItem::from_value(json!(5)),
            comparison_type: ComparisonType::LessThanOrEqualTo,
            negate: false,
        };
        let context = serde_json::to_value(json!({
        "action1": {
                "output": {
                    "count": 17
                },
            },
        "location": "Menlo Park, CA",
    })).unwrap();
        let result = check_assertion(&assertion, &context);
        println!("{:?}", result.message);
        assert_eq!(result.success, false);
    }
}