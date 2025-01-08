use serde::{Deserialize, Serialize};
use serde_json::Value;
use crate::json_path::model::Expression;

#[derive(Serialize, Deserialize, Clone)]
pub enum ComparisonType {
    EqualTo,
    Contains,
    GreaterThan,
    GreaterThanOrEqualTo,
    LessThan,
    LessThanOrEqualTo,
}

#[derive(Serialize, Deserialize, Clone)]
pub enum Operation {
    Sum,
    Avg,
    Count,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Function {
    pub operation: Operation,
    pub parameters: Vec<ValueProvider>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ValueProvider {
    pub expression: Option<Expression>,
    pub value: Option<Value>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct AssertionItem {
    pub function: Option<Function>,
    pub value_provider: Option<ValueProvider>,
}

impl AssertionItem {
    pub fn from_function(function: Function) -> Self {
        AssertionItem {
            function: Some(function),
            value_provider: None,
        }
    }

    pub fn from_expression(expression: Expression) -> Self {
        AssertionItem {
            function: None,
            value_provider: Some(ValueProvider {
                expression: Some(expression),
                value: None,
            }),
        }
    }

    pub fn from_value(value: Value) -> Self {
        AssertionItem {
            function: None,
            value_provider: Some(ValueProvider {
                expression: None,
                value: Some(value),
            }),
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Assertion {
    pub customer_id: String,
    pub test_case_id: String,
    pub id: String,
    pub left: AssertionItem,
    pub right: AssertionItem,
    pub comparison_type: ComparisonType,
    pub negate: bool,
}

#[derive(Serialize, Deserialize, Clone, Eq, PartialEq, Debug)]
pub struct AssertionResult {
    pub assertion_id: String,
    pub success: bool,
    pub message: Option<String>,
}

impl AssertionResult {
    pub fn from_error(id: String, message: String) -> Self {
        AssertionResult {
            assertion_id: id.clone(),
            success: false,
            message: Some(message),
        }
    }

    pub fn of_success(id: String) -> Self {
        AssertionResult {
            assertion_id: id.clone(),
            success: true,
            message: None,
        }
    }
}