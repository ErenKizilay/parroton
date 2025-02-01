use bon::Builder;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;

#[derive(Serialize, Deserialize, Clone, Eq, PartialEq, Debug, Builder)]
pub struct Action {
    pub customer_id: String,
    pub test_case_id: String,
    #[builder(default = uuid::Uuid::new_v4().to_string())]
    pub id: String,
    pub order: usize,
    pub url: String,
    pub name: String,
    pub mime_type: Option<String>,
    pub method: String,
    pub created_at: Option<u64>,
    pub updated_at: Option<u64>,
}

impl PartialOrd for Action {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.order.cmp(&other.order))
    }
}

impl Ord for Action {
    fn cmp(&self, other: &Self) -> Ordering {
        self.order.cmp(&other.order)
    }
}