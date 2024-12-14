use futures::StreamExt;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use std::fmt::Debug;
use std::str::FromStr;

#[derive(Serialize, Deserialize, Clone)]
pub struct TestCase {
    pub customer_id: String,
    pub id: String,
    pub name: String,
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


#[derive(Serialize, Deserialize, Clone)]
pub struct Expression {
    pub value: String,
}

#[derive(Serialize, Deserialize)]
pub enum Assertion {
    EqualTo(Expression, Expression),
}

pub enum FlattenKeyPrefixType {
    Output,
    Input,
    AssertionExpression,
}