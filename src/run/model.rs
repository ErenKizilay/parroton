use serde::{Deserialize, Serialize};
use crate::assertion::model::AssertionResult;

#[derive(Serialize, Deserialize, Clone, Eq, PartialEq, Debug)]
pub struct Run {
    pub customer_id: String,
    pub test_case_id: String,
    pub id: String,
    pub status: RunStatus,
    pub started_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<String>,
    pub assertion_results: Option<Vec<AssertionResult>>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub enum RunStatus {
    InProgress,
    Finished,
}