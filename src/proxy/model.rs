use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone)]
pub struct ProxyRecord {
    pub customer_id: String,
    pub test_case_id: String,
    pub run_id: String,
    pub id: String,
}