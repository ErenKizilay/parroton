use std::sync::Arc;
use aws_sdk_dynamodb::Client;
use aws_sdk_dynamodb::types::AttributeValue;
use crate::api::AppError;
use crate::models::Assertion;
use crate::persistence::repo::{build_composite_key, QueryResult, Table};


pub struct AssertionOperations {
    pub(crate) client: Arc<Client>,
}

pub(crate) struct AssertionsTable();

impl Table<Assertion> for AssertionsTable {
    fn table_name() -> String {
        "assertions".to_string()
    }

    fn partition_key_name() -> String {
        "customer_id#test_case_id".to_string()
    }

    fn sort_key_name() -> String {
        "action_id#id".to_string()
    }

    fn partition_key_from_entity(entity: &Assertion) -> (String, AttributeValue) {
        Self::partition_key(build_composite_key(vec![entity.customer_id.clone(), entity.test_case_id.clone()]))
    }

    fn sort_key_from_entity(entity: &Assertion) -> (String, AttributeValue) {
        Self::sort_key(build_composite_key(vec![entity.action_id.clone(), entity.id.clone()]))
    }
}

impl AssertionOperations {
    pub async fn list(&self, customer_id: &String, test_case_id: &String) -> Result<QueryResult<Assertion>, AppError> {
        AssertionsTable::list_items(self.client.clone(), build_composite_key(vec![customer_id.clone(), test_case_id.clone()]), None)
            .await
    }

    pub async fn batch_create(&self, assertions: Vec<Assertion>) {
        AssertionsTable::batch_put_item(self.client.clone(), assertions).await
    }
}