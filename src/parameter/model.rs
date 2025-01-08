use serde::{Deserialize, Serialize};
use serde_json::Value;
use crate::json_path::model::Expression;

#[derive(Serialize, Deserialize, Clone)]
pub enum ParameterType {
    Input,
    Output,
}

#[derive(Deserialize, Clone, PartialEq)]
pub enum ParameterIn {
    Header,
    Cookie,
    Query,
    Body,
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
            ParameterLocation::Header(_) => { ParameterIn::Header }
            ParameterLocation::Cookie(_) => { ParameterIn::Cookie }
            ParameterLocation::Query(_) => { ParameterIn::Query }
            ParameterLocation::Body(_) => { ParameterIn::Body }
        }
    }
}