use bon::Builder;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Serialize, Deserialize, Clone, Builder)]
pub struct AuthenticationProvider {
    pub customer_id: String,
    #[builder(default = uuid::Uuid::new_v4().to_string())]
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub headers_by_name: HashMap<String, AuthHeaderValue>,
    #[serde(skip_serializing_if = "HashSet::is_empty", default = "HashSet::new")]
    pub linked_test_case_ids: HashSet<String>,
    pub created_at: Option<u64>,
    pub updated_at: Option<u64>,
}

#[derive(Serialize, Deserialize, Clone, Eq, PartialEq, Debug, Builder)]
pub struct AuthHeaderValue {
    pub value: String,
    #[builder(default = false)]
    pub disabled: bool,
}

#[derive(Builder)]
pub struct ListAuthProvidersRequest {
    pub customer_id: String,
    pub test_case_id: Option<String>,
    pub base_url: Option<String>,
    pub next_page_key: Option<String>,
    pub keyword: Option<String>,
}