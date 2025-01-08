use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::fmt::Debug;
use crate::persistence::repo::ParameterIn;

#[derive(Serialize, Deserialize, Clone)]
pub struct TestCase {
    pub customer_id: String,
    pub id: String,
    pub name: String,
    pub description: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub enum ParameterType {
    Input,
    Output,
}

#[derive(Serialize, Deserialize, Clone)]
pub enum ParameterLocation {
    Header(String),
    Cookie(String),
    Query(String),
    Body(String),
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Parameter {
    pub customer_id: String,
    pub test_case_id: String,
    pub action_id: String,
    pub id: String,
    pub parameter_type: ParameterType,
    pub location: ParameterLocation,
    pub value: Value,
    pub value_expression: Option<Expression>,

}

#[derive(Serialize, Deserialize, Clone)]
pub struct AuthenticationProvider {
    pub customer_id: String,
    pub id: String,
    pub base_url: String,
    pub headers_by_name: HashMap<String, AuthHeaderValue>,
    pub linked_test_case_ids: HashSet<String>,
}

#[derive(Serialize, Deserialize, Clone, Eq, PartialEq, Debug)]
pub struct AuthHeaderValue {
    pub value: String,
    pub disabled: bool,
}


#[derive(Serialize, Deserialize, Clone, Eq, PartialEq, Debug)]
pub struct Run {
    pub customer_id: String,
    pub test_case_id: String,
    pub id: String,
    pub status: RunStatus,
    pub started_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub enum RunStatus {
    InProgress,
    Finished,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ActionExecution {
    pub run_id: String,
    pub customer_id: String,
    pub test_case_id: String,
    pub action_id: String,
    pub id: String,
    pub status_code: u16,
    pub error: Option<String>,
    pub response_body: Option<Value>,
    pub request_body: Option<Value>,
    pub query_params: Vec<(String, String)>,
    pub started_at: String,
    pub finished_at: String,
    pub assertion_results: Option<Vec<AssertionResult>>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ActionExecutionPair {
    pub action: Option<Action>,
    pub execution: ActionExecution,
}


#[derive(Serialize, Deserialize, Clone)]
pub struct ProxyRecord {
    pub customer_id: String,
    pub test_case_id: String,
    pub run_id: String,
    pub id: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct AuthenticationProviderAssociation {
    pub customer_id: String,
    pub provider_id: String,
    pub test_case_id: String,
}

#[derive(Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct Action {
    pub customer_id: String,
    pub test_case_id: String,
    pub id: String,
    pub order: usize,
    pub url: String,
    pub name: String,
    pub mime_type: Option<String>,
    pub method: String,
}

impl PartialOrd for Action {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.order.cmp(&other.order))
    }
}

impl Ord for Action {
    fn cmp(&self, other: &Self) -> Ordering {
        self.order.cmp(&other.order)
    }
}


#[derive(Serialize, Deserialize, Clone)]
pub struct Expression {
    pub value: String,
}


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
    pub action_id: String,
    pub id: String,
    pub left: AssertionItem,
    pub right: AssertionItem,
    pub comparison_type: ComparisonType,
    pub negate: bool,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct AssertionResult {
    pub success: bool,
    pub message: Option<String>,
}

impl AssertionResult {
    pub fn from_error(message: String) -> Self {
        AssertionResult {
            success: false,
            message: Some(message),
        }
    }

    pub fn of_success() -> Self {
        AssertionResult {
            success: true,
            message: None,
        }
    }
}

pub enum FlattenKeyPrefixType {
    Output,
    Input,
    AssertionExpression,
}

impl Parameter {
    pub fn get_path(&self) -> String {
        match &self.location {
            ParameterLocation::Header(name) => { name.clone() }
            ParameterLocation::Cookie(name) => { name.clone() }
            ParameterLocation::Query(name) => { name.clone() }
            ParameterLocation::Body(name) => { name.clone() }
        }
    }

    pub fn get_parameter_in(&self) -> ParameterIn {
        match &self.location {
            ParameterLocation::Header(_) => {ParameterIn::Header}
            ParameterLocation::Cookie(_) => {ParameterIn::Cookie}
            ParameterLocation::Query(_) => {ParameterIn::Query}
            ParameterLocation::Body(_) => {ParameterIn::Body}
        }
    }
}