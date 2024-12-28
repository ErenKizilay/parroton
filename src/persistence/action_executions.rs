use crate::api::AppError;
use crate::models::ActionExecution;
use crate::persistence::repo::{build_composite_key, Table};
use aws_sdk_dynamodb::types::AttributeValue;
use aws_sdk_dynamodb::Client;
use std::collections::HashMap;
use std::sync::Arc;

pub struct ActionExecutionsOperations {
    pub(crate) client: Arc<Client>,
}
struct ActionExecutionTable();

impl Table<ActionExecution> for ActionExecutionTable {
    fn table_name() -> String {
        "action_executions".to_string()
    }

    fn partition_key_name() -> String {
        "customer_id#test_case_id#run_id".to_string()
    }

    fn sort_key_name() -> String {
        "id".to_string()
    }

    fn partition_key_from_entity(entity: &ActionExecution) -> (String, AttributeValue) {
        Self::partition_key(build_composite_key(vec![
            entity.customer_id.clone(),
            entity.test_case_id.clone(),
            entity.run_id.clone(),
        ]))
    }

    fn sort_key_from_entity(entity: &ActionExecution) -> (String, AttributeValue) {
        Self::sort_key(entity.id.clone())
    }

    fn add_index_key_attributes(
        entity: &ActionExecution,
        item: &mut HashMap<String, AttributeValue>,
    ) {
        item.insert(
            "started_at".to_string(),
            AttributeValue::S(entity.started_at.to_string()),
        );
    }
}

impl ActionExecutionsOperations {
    pub async fn list(
        &self,
        customer_id: &String,
        test_case_id: &String,
        run_id: &String,
    ) -> Result<Vec<ActionExecution>, AppError> {
        ActionExecutionTable::list_all_items(
            self.client.clone(),
            build_composite_key(vec![
                customer_id.clone(),
                test_case_id.clone(),
                run_id.clone(),
            ]),
        ).await
    }

    pub async fn create(
        &self,
        action_execution: ActionExecution,
    ) -> ActionExecution {
        ActionExecutionTable::put_item(self.client.clone(), action_execution)
            .await
            .unwrap()
    }
}