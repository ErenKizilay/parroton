use std::cmp::Ordering;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Eq, PartialEq, Debug)]
pub struct Action {
    pub customer_id: String,
    pub test_case_id: String,
    pub id: String,
    pub order: usize,
    pub url: String,
    pub name: String,
    pub mime_type: Option<String>,
    pub method: String,
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