use std::cmp::Ordering;
use crate::api::AppError;
use crate::persistence::repo::{build_composite_key, Table};
use aws_sdk_dynamodb::types::AttributeValue;
use aws_sdk_dynamodb::Client;
use std::collections::HashMap;
use std::sync::Arc;
use aws_sdk_dynamodb::primitives::{DateTime, DateTimeFormat};
use crate::action::service::ActionsTable;
use crate::action_execution::model::{ActionExecution, ActionExecutionPair};

pub struct ActionExecutionsOperations {
    pub(crate) client: Arc<Client>,
}
pub(crate) struct ActionExecutionTable();

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

    fn ordering(e1: &ActionExecution, e2: &ActionExecution) -> Ordering {
        let started_at1 = DateTime::from_str(e1.started_at.as_str(), DateTimeFormat::DateTimeWithOffset).unwrap();
        let started_at2 = DateTime::from_str(e2.started_at.as_str(), DateTimeFormat::DateTimeWithOffset).unwrap();
        started_at1.cmp(&started_at2)
    }
}

impl ActionExecutionsOperations {

    pub async fn list_with_actions(
        &self,
        customer_id: &String,
        test_case_id: &String,
        run_id: &String,
    ) -> Result<Vec<ActionExecutionPair>, AppError> {
        let result = ActionExecutionTable::list_all_items(
            self.client.clone(),
            build_composite_key(vec![
                customer_id.clone(),
                test_case_id.clone(),
                run_id.clone(),
            ]),
        )
            .await;
        match result {
            Ok(execs) => {
                let key_pairs = execs
                    .iter()
                    .map(|exec| {
                        (
                            build_composite_key(vec![
                                exec.customer_id.clone(),
                                exec.test_case_id.clone(),
                            ]),
                            exec.action_id.clone(),
                        )
                    })
                    .collect();
                ActionsTable::batch_get_items(self.client.clone(), key_pairs)
                    .await
                    .map(|actions| {
                        let mut pairs: Vec<ActionExecutionPair> = execs
                            .into_iter()
                            .map(|exec| ActionExecutionPair {
                                action: (actions.iter().find(|a| a.id.eq(&exec.action_id))).cloned(),
                                execution: exec,
                            })
                            .collect();
                        pairs
                    })
            }
            Err(err) => Err(err),
        }
    }

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

    pub async fn create(&self, action_execution: ActionExecution) -> ActionExecution {
        ActionExecutionTable::put_item(self.client.clone(), action_execution)
            .await
            .unwrap()
    }
}
