use crate::models::{Run, RunStatus};
use crate::persistence::repo::{build_composite_key, QueryResult, Table};
use aws_sdk_dynamodb::primitives::DateTime;
use aws_sdk_dynamodb::primitives::DateTimeFormat::DateTimeWithOffset;
use aws_sdk_dynamodb::types::AttributeValue;
use aws_sdk_dynamodb::Client;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::SystemTime;
use crate::api::{AppError};

pub struct RunOperations {
    pub(crate) client: Arc<Client>,
}

struct RunTable();

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
    ) {
        RunTable::update_builder(self.client.clone())
            .set_key(Some(RunTable::unique_key(
                build_composite_key(vec![customer_id.clone(), test_case_id.clone()]),
                id.clone(),
            )))
            .expression_attribute_names("#fa", "finished_at")
            .expression_attribute_names("#s", "status")
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
            .update_expression("SET #fa = :fa, #s = :s")
            .send()
            .await
            .unwrap();
    }
}