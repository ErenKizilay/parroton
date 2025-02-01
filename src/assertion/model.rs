use crate::json_path::model::Expression;
use bon::Builder;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum ComparisonType {
    EqualTo,
    Contains,
    GreaterThan,
    GreaterThanOrEqualTo,
    LessThan,
    LessThanOrEqualTo,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum Operation {
    Sum,
    Avg,
    Count,
}

#[derive(Serialize, Deserialize, Clone, Debug, Builder)]
pub struct Function {
    pub operation: Operation,
    pub parameters: Vec<ValueProvider>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq, Builder)]
pub struct ValueProvider {
    pub expression: Option<Expression>,
    pub value: Option<Value>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Builder)]
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

#[derive(Serialize, Deserialize, Clone, Debug, Builder)]
pub struct Assertion {
    pub customer_id: String,
    pub test_case_id: String,
    #[builder(default = uuid::Uuid::new_v4().to_string())]
    pub id: String,
    pub left: AssertionItem,
    pub right: AssertionItem,
    pub comparison_type: ComparisonType,
    #[builder(default = false)]
    pub negate: bool,
    pub created_at: Option<u64>,
    pub updated_at: Option<u64>,
}

#[derive(Serialize, Deserialize, Clone, Eq, PartialEq, Debug, Builder)]
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