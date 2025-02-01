use crate::api::AppError;
use crate::assertion::model::AssertionResult;
use crate::persistence::model::QueryResult;
use crate::persistence::repo::OnDeleteMessage::RunDeleted;
use crate::persistence::repo::{build_composite_key, current_timestamp, OnDeleteMessage, Table};
use crate::run::model::{Run, RunStatus};
use aws_sdk_dynamodb::types::AttributeValue;
use aws_sdk_dynamodb::Client;
use serde_dynamo::aws_sdk_dynamodb_1::to_attribute_value;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::sync::Arc;

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
            AttributeValue::N(entity.started_at.to_string()),
        );
    }

    fn build_deleted_event(entity: Run) -> Option<OnDeleteMessage> {
        Some(RunDeleted(entity))
    }

    fn ordering(e1: &Run, e2: &Run) -> Ordering {
        e2.started_at.cmp(&e1.started_at)
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
        RunTable::update_partial(build_composite_key(vec![customer_id.clone(), test_case_id.clone()]), id.clone(),
                                 self.client.clone().update_item()
                                     .expression_attribute_names("#fa", "finished_at")
                                     .expression_attribute_names("#s", "status")
                                     .expression_attribute_names("#ar", "assertion_results")
                                     .expression_attribute_values(":s", to_attribute_value(status).unwrap())
                                     .expression_attribute_values(":fa", AttributeValue::N(current_timestamp().to_string()))
                                     .expression_attribute_values(":ar", to_attribute_value(assertion_results).unwrap())
                                     .update_expression("SET #fa = :fa, #s = :s, #ar = :ar"))
            .await
            .unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::repo::{init_logger, Repository};


    #[tokio::test]
    async fn crud() {
        init_logger();
        let repository = Repository::new().await;
        let run = Run::builder()
            .customer_id("cust1".to_string())
            .test_case_id("tc1".to_string())
            .id("r1".to_string())
            .status(RunStatus::InProgress)
            .started_at(current_timestamp())
            .build();
        repository.runs()
            .create(run).await;
        let update_result = repository.runs()
            .update(&"cust1".to_string(), &"tc1".to_string(), &"r1".to_string(), &RunStatus::Finished, vec![])
            .await;
        let get_result = repository.runs()
            .get(&"cust1".to_string(), &"tc1".to_string(), &"r1".to_string())
            .await;
        assert_eq!(get_result.is_ok(), true);
        assert_eq!(get_result.unwrap().unwrap().status, RunStatus::Finished);
    }
}