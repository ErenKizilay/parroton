use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use futures::StreamExt;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use std::fmt::Debug;
use std::str::FromStr;
use std::time::{Instant, SystemTime};
use aws_sdk_dynamodb::types::Put;
use futures::future::Either;
use uuid::Timestamp;
use crate::http::{HttpError, HttpResult};

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
    StatusCode(),
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
    pub disabled: bool
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
    Finished
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

#[derive(Serialize, Deserialize)]
pub enum Assertion {
    EqualTo(Expression, Expression),
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

#[derive(Serialize, Deserialize, Clone)]
pub struct AssertionEntity {
    pub left: AssertionItem,
    pub right: AssertionItem,
    pub comparison_type: ComparisonType,
    pub negate: bool,
}

pub enum FlattenKeyPrefixType {
    Output,
    Input,
    AssertionExpression,
}

impl Parameter {

    pub fn get_path(&self) -> String {
        match &self.location {
            ParameterLocation::Header(name) => {name.clone()}
            ParameterLocation::Cookie(name) => {name.clone()}
            ParameterLocation::Query(name) => {name.clone()}
            ParameterLocation::Body(name) => {name.clone()}
            ParameterLocation::StatusCode() => {String::new()}
        }
    }
}