use serde::{Deserialize, Serialize};
use serde_json::Value;
use crate::action::model::Action;

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
pub struct ActionExecutionPair {
    pub action: Option<Action>,
    pub execution: ActionExecution,
}