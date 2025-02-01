use crate::json_path::model::Expression;
use bon::Builder;
use serde::{Deserialize, Serialize};
use serde_json::Value;

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

#[derive(Serialize, Deserialize, Clone, Builder)]
pub struct Parameter {
    pub customer_id: String,
    pub test_case_id: String,
    pub action_id: String,
    #[builder(default = uuid::Uuid::new_v4().to_string())]
    pub id: String,
    pub parameter_type: ParameterType,
    pub location: ParameterLocation,
    pub value: Value,
    pub value_expression: Option<Expression>,
    pub created_at: Option<u64>,
    pub updated_at: Option<u64>,

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