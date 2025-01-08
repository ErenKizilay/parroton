use crate::api::AppError;
use crate::persistence::repo::{build_composite_key, QueryResult, Table};
use aws_sdk_dynamodb::types::AttributeValue;
use aws_sdk_dynamodb::Client;
use serde_dynamo::to_attribute_value;
use std::sync::Arc;
use crate::assertion::model::{Assertion, ComparisonType};

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
        "id".to_string()
    }

    fn partition_key_from_entity(entity: &Assertion) -> (String, AttributeValue) {
        Self::partition_key(build_composite_key(vec![entity.customer_id.clone(), entity.test_case_id.clone()]))
    }

    fn sort_key_from_entity(entity: &Assertion) -> (String, AttributeValue) {
        Self::sort_key(entity.id.clone())
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

    pub async fn delete(&self, customer_id: String, test_case_id: String, id: String) -> Result<Option<Assertion>, AppError> {
        AssertionsTable::delete_item(self.client.clone(), build_composite_key(vec![customer_id.clone(),
                                                                                   test_case_id.clone()]), id)
            .await
    }
    pub async fn put(&self, assertion: Assertion) -> Result<Assertion, AppError> {
        AssertionsTable::put_item(self.client.clone(), assertion).await
    }

    pub async fn update_comparison_type(&self, customer_id: String, test_case_id: String, id: String, comparison_type: ComparisonType) -> Result<Assertion, AppError> {
        AssertionsTable::update_partial(build_composite_key(vec![customer_id.clone(), test_case_id.clone()]), id,
                                        self.client.clone().update_item()
                                            .expression_attribute_names("#comparison_type", "comparison_type")
                                            .expression_attribute_values(":value", to_attribute_value(comparison_type).unwrap())
                                            .update_expression("SET #comparison_type = :value")).await
    }

    pub async fn update_comparison_negation(&self, customer_id: String, test_case_id: String, id: String, negate: bool) -> Result<Assertion, AppError> {
        AssertionsTable::update_partial(build_composite_key(vec![customer_id.clone(), test_case_id.clone()]), id,
                                        self.client.clone().update_item()
                                            .expression_attribute_names("#negate", "negate")
                                            .expression_attribute_values(":value", to_attribute_value(negate).unwrap())
                                            .update_expression("SET #negate = :value")).await
    }

    pub async fn get(&self, customer_id: String, test_case_id: String, id: String) -> Result<Option<Assertion>, AppError> {
        AssertionsTable::get_item(self.client.clone(), build_composite_key(vec![customer_id.clone(), test_case_id.clone()]), id)
            .await
    }
}