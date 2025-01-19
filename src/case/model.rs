use bon::Builder;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, Builder)]
pub struct TestCase {
    pub customer_id: String,
    #[builder(default = uuid::Uuid::new_v4().to_string())]
    pub id: String,
    pub name: String,
    pub description: String,
    pub created_at: Option<u64>,
    pub updated_at: Option<u64>,
}