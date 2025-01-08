use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

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