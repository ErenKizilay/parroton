use crate::assertion::model::AssertionResult;
use bon::Builder;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Eq, PartialEq, Debug, Builder)]
pub struct Run {
    pub customer_id: String,
    pub test_case_id: String,
    #[builder(default = uuid::Uuid::new_v4().to_string())]
    pub id: String,
    pub status: RunStatus,
    pub started_at: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<u64>,
    pub assertion_results: Option<Vec<AssertionResult>>,
    pub created_at: Option<u64>,
    pub updated_at: Option<u64>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub enum RunStatus {
    InProgress,
    Finished,
}