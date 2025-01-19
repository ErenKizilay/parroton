use crate::action::model::Action;
use bon::Builder;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Serialize, Deserialize, Clone, Builder)]
pub struct ActionExecution {
    pub run_id: String,
    pub customer_id: String,
    pub test_case_id: String,
    pub action_id: String,
    #[builder(default = uuid::Uuid::new_v4().to_string())]
    pub id: String,
    pub status_code: u16,
    pub error: Option<String>,
    pub response_body: Option<Value>,
    pub request_body: Option<Value>,
    pub query_params: Vec<(String, String)>,
    pub started_at: Option<u64>,
    pub finished_at: Option<u64>,
    pub created_at: Option<u64>,
    pub updated_at: Option<u64>,
}

#[derive(Serialize, Deserialize, Clone, Builder)]
pub struct ActionExecutionPair {
    pub action: Option<Action>,
    pub execution: ActionExecution,
}