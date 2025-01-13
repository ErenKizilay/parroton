use std::cmp::Ordering;
use crate::persistence::repo::{build_composite_key, OnDeleteMessage, QueryResult, Table};
use aws_sdk_dynamodb::primitives::{DateTime, DateTimeFormat};
use aws_sdk_dynamodb::primitives::DateTimeFormat::DateTimeWithOffset;
use aws_sdk_dynamodb::types::AttributeValue;
use aws_sdk_dynamodb::Client;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::SystemTime;
use serde_dynamo::aws_sdk_dynamodb_1::to_attribute_value;
use crate::api::{AppError};
use crate::assertion::model::AssertionResult;
use crate::persistence::repo::OnDeleteMessage::RunDeleted;
use crate::run::model::{Run, RunStatus};

pub struct RunOperations {
    pub(crate) client: Arc<Client>,
}

pub struct RunTable();

impl Table<Run> for RunTable {
    fn table_name() -> String {
        "runs".to_string()
    }

    fn partition_key_name() -> String {
        "customer_id#test_case_id".to_string()
    }

    fn sort_key_name() -> String {
        "id".to_string()
    }

    fn partition_key_from_entity(entity: &Run) -> (String, AttributeValue) {
        Self::partition_key(build_composite_key(vec![
            entity.customer_id.clone(),
            entity.test_case_id.clone(),
        ]))
    }

    fn sort_key_from_entity(entity: &Run) -> (String, AttributeValue) {
        Self::sort_key(entity.id.clone())
    }

    fn add_index_key_attributes(entity: &Run, item: &mut HashMap<String, AttributeValue>) {
        item.insert(
            "started_at".to_string(),
            AttributeValue::S(entity.started_at.to_string()),
        );
    }

    fn build_deleted_event(entity: Run) -> Option<OnDeleteMessage> {
        Some(RunDeleted(entity))
    }

    fn ordering(e1: &Run, e2: &Run) -> Ordering {
        let started_at1 = DateTime::from_str(e1.started_at.as_str(), DateTimeWithOffset).unwrap();
        let started_at2 = DateTime::from_str(e2.started_at.as_str(), DateTimeWithOffset).unwrap();
        started_at2.cmp(&started_at1)
    }
}

impl RunOperations {
    pub async fn create(&self, run: Run) -> Run {
        RunTable::put_item(self.client.clone(), run).await.unwrap()
    }

    pub async fn get(
        &self,
        customer_id: &String,
        test_case_id: &String,
        id: &String,
    ) -> Result<Option<Run>, AppError> {
        RunTable::get_item(
            self.client.clone(),
            build_composite_key(vec![customer_id.clone(), test_case_id.clone()]),
            id.clone(),
        ).await
    }
    pub async fn list(&self, customer_id: &String, test_case_id: &String) -> Result<QueryResult<Run>, AppError> {
        let result = RunTable::query_builder(self.client.clone())
            .scan_index_forward(false)
            .expression_attribute_names("#pk", RunTable::partition_key_name())
            .expression_attribute_values(":pk", AttributeValue::S(build_composite_key(vec![customer_id.clone(), test_case_id.clone()])))
            .key_condition_expression("#pk = :pk")
            .send().await;
        RunTable::from_query_result(result)
    }

    pub async fn update(
        &self,
        customer_id: &String,
        test_case_id: &String,
        id: &String,
        status: &RunStatus,
        assertion_results: Vec<AssertionResult>,
    ) {
        RunTable::update_builder(self.client.clone())
            .set_key(Some(RunTable::unique_key(
                build_composite_key(vec![customer_id.clone(), test_case_id.clone()]),
                id.clone(),
            )))
            .expression_attribute_names("#fa", "finished_at")
            .expression_attribute_names("#s", "status")
            .expression_attribute_names("#ar", "assertion_results")
            .expression_attribute_values(
                ":s",
                AttributeValue::S(
                    serde_json::to_string(status)
                        .unwrap()
                        .trim_matches('"')
                        .to_string(),
                ),
            )
            .expression_attribute_values(
                ":fa",
                AttributeValue::S(DateTime::from(SystemTime::now()).fmt(DateTimeWithOffset).unwrap()),
            )
            .expression_attribute_values(":ar", to_attribute_value(assertion_results).unwrap())
            .update_expression("SET #fa = :fa, #s = :s, #ar = :ar")
            .send()
            .await
            .unwrap();
    }
}