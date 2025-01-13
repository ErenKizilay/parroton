use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone)]
pub struct TestCase {
    pub customer_id: String,
    pub id: String,
    pub name: String,
    pub description: String,
}