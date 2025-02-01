use crate::action::service::ActionsTable;
use crate::action_execution::service::ActionExecutionTable;
use crate::api::AppError;
use crate::assertion::service::AssertionsTable;
use crate::auth::service::AuthProviderOperations;
use crate::case::model::TestCase;
use crate::parameter::service::ParametersTable;
use crate::persistence::model::{ListItemsRequest, QueryResult};
use crate::persistence::repo::{build_composite_key, OnDeleteMessage, Table};
use crate::run::service::RunTable;
use aws_sdk_dynamodb::types::AttributeValue;
use aws_sdk_dynamodb::Client;
use std::alloc::System;
use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::mpsc;
use tokio::sync::mpsc::Sender;
use tokio::task::id;
use tracing::info;

struct TestCaseTable();

pub struct TestCaseOperations {
    pub(crate) client: Arc<Client>,
}

impl Table<TestCase> for TestCaseTable {
    fn table_name() -> String {
        "test_cases".to_string()
    }

    fn partition_key_name() -> String {
        "customer_id".to_string()
    }

    fn sort_key_name() -> String {
        "id".to_string()
    }

    fn partition_key_from_entity(entity: &TestCase) -> (String, AttributeValue) {
        Self::sort_key(build_composite_key(vec![entity.customer_id.clone()]))
    }

    fn sort_key_from_entity(entity: &TestCase) -> (String, AttributeValue) {
        Self::sort_key(build_composite_key(vec![entity.id.clone()]))
    }
}

impl TestCaseOperations {
    pub async fn create(&self, test_case: TestCase) -> TestCase {
        TestCaseTable::put_item(self.client.clone(), test_case)
            .await
            .unwrap()
    }

    pub async fn list(
        &self,
        customer_id: String,
        next_page_key: Option<String>,
        keyword: Option<String>,
    ) -> Result<QueryResult<TestCase>, AppError> {
        TestCaseTable::list_items(self.client.clone(), ListItemsRequest::builder()
            .partition_key(customer_id)
            .maybe_next_page_key(next_page_key)
            .maybe_filter_expression(keyword.clone().map(|keyword|{"contains(#name, :keyword)".to_string()}))
            .maybe_expression_attribute_names(keyword.clone().map(|keyword|{HashMap::from([("#name".to_string(), "name".to_string())])}))
            .maybe_expression_attribute_values(keyword.clone().map(|keyword|{HashMap::from([(":keyword".to_string(), AttributeValue::S(keyword))])}))
            .build()).await
    }

    pub async fn get(
        &self,
        customer_id: String,
        test_case_id: String,
    ) -> Result<Option<TestCase>, AppError> {
        TestCaseTable::get_item(self.client.clone(), customer_id, test_case_id).await
    }

    pub async fn update(&self, customer_id: String, test_case_id: String, name: String, desc: String) -> Result<TestCase, AppError> {
        TestCaseTable::update_partial(customer_id, test_case_id, self.client.clone()
            .update_item()
            .expression_attribute_names("#name", "name")
            .expression_attribute_names("#desc", "description")
            .expression_attribute_values(":name", AttributeValue::S(name))
            .expression_attribute_values(":desc", AttributeValue::S(desc))
            .update_expression("SET #name = :name, #desc = :desc"),
        ).await
    }

    pub async fn update_name(&self, customer_id: String, test_case_id: String, name: String) -> Result<TestCase, AppError> {
        TestCaseTable::update_partial(customer_id, test_case_id, self.client.clone()
            .update_item()
            .expression_attribute_names("#name", "name")
            .expression_attribute_values(":val", AttributeValue::S(name))
            .update_expression("SET #name = :val"),
        ).await
    }

    pub async fn update_description(&self, customer_id: String, test_case_id: String, description: String) -> Result<TestCase, AppError> {
        TestCaseTable::update_partial(customer_id, test_case_id, self.client.clone()
            .update_item()
            .expression_attribute_names("#desc", "description")
            .expression_attribute_values(":val", AttributeValue::S(description))
            .update_expression("SET #desc = :val"),
        ).await
    }

    pub async fn delete(&self, customer_id: &String, test_case_id: &String) {
        let (tx, mut rx) = mpsc::channel(32);
        let deleted_test_case = TestCaseTable::delete_item(
            self.client.clone(),
            customer_id.clone(),
            test_case_id.clone(),
        ).await;
        if let Ok(Some(deleted_case)) = deleted_test_case {
            tx.send(OnDeleteMessage::TestCaseDeleted(deleted_case))
                .await
                .unwrap();
        }
        let cloned_client = self.client.clone();
        tokio::task::spawn(async move {
            while let Some(message) = rx.recv().await {
                info!("received deleted message: {:?}", message);
                match message {
                    OnDeleteMessage::TestCaseDeleted(test_case) => {
                        Self::delete_all_actions(&test_case.customer_id, &test_case.id, &tx, cloned_client.clone()).await;
                        Self::delete_all_runs(&test_case.customer_id, &test_case.id, &tx, cloned_client.clone()).await;
                        Self::delete_all_assertions(&test_case.customer_id, &test_case.id, &tx, cloned_client.clone()).await;
                        AuthProviderOperations {
                            client: cloned_client.clone(),
                        }.unlink_test_case(&test_case.customer_id, &test_case.id).await;
                    }
                    OnDeleteMessage::ActionDeleted(action) => {
                        Self::delete_all_parameters(&action.customer_id, &action.test_case_id, &tx, cloned_client.clone()).await;
                    }
                    OnDeleteMessage::RunDeleted(run) => {
                        Self::delete_all_action_executions(&run.customer_id, &run.test_case_id, &run.id, &tx, cloned_client.clone()).await;
                    }
                }
            }
        });
    }

    async fn delete_all_actions(customer_id: &String, id: &String, tx: &Sender<OnDeleteMessage>, client: Arc<Client>) {
        let sender = tx.clone();
        let client_cloned = client.clone();
        let customer_id_cloned = customer_id.clone();
        let id_cloned = id.clone();
        tokio::task::spawn(async move {
            ActionsTable::delete_all_items(
                client_cloned,
                build_composite_key(vec![
                    customer_id_cloned,
                    id_cloned,
                ]),
                &sender,
            )
                .await;
        });
    }

    async fn delete_all_parameters(customer_id: &String, id: &String, tx: &Sender<OnDeleteMessage>, client: Arc<Client>) {
        let sender = tx.clone();
        let client_cloned = client.clone();
        let customer_id_cloned = customer_id.clone();
        let id_cloned = id.clone();
        tokio::task::spawn(async move {
            ParametersTable::delete_all_items(
                client_cloned,
                build_composite_key(vec![
                    customer_id_cloned,
                    id_cloned,
                ]),
                &sender,
            ).await;
        });
    }

    async fn delete_all_runs(customer_id: &String, id: &String, tx: &Sender<OnDeleteMessage>, client: Arc<Client>) {
        let sender = tx.clone();
        let client_cloned = client.clone();
        let customer_id_cloned = customer_id.clone();
        let id_cloned = id.clone();
        tokio::task::spawn(async move {
            RunTable::delete_all_items(
                client_cloned.clone(),
                build_composite_key(vec![
                    customer_id_cloned,
                    id_cloned,
                ]),
                &sender,
            ).await;
        });
    }

    async fn delete_all_assertions(customer_id: &String, id: &String, tx: &Sender<OnDeleteMessage>, client: Arc<Client>) {
        let sender = tx.clone();
        let client_cloned = client.clone();
        let customer_id_cloned = customer_id.clone();
        let id_cloned = id.clone();
        tokio::task::spawn(async move {
            AssertionsTable::delete_all_items(client_cloned, build_composite_key(vec![customer_id_cloned, id_cloned]), &sender)
                .await;
        });
    }

    async fn delete_all_action_executions(customer_id: &String, test_case_id: &String, run_id: &String, tx: &Sender<OnDeleteMessage>, client: Arc<Client>) {
        let sender = tx.clone();
        let client_cloned = client.clone();
        let customer_id_cloned = customer_id.clone();
        let test_case_id_cloned = test_case_id.clone();
        let run_id_cloned = run_id.clone();
        tokio::task::spawn(async move {
            ActionExecutionTable::delete_all_items(client_cloned, build_composite_key(vec![customer_id_cloned, test_case_id_cloned, run_id_cloned]), &sender)
                .await;
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::repo::{init_logger, Repository};


    #[tokio::test]
    async fn delete() {
        init_logger();
        let repository = Repository::new().await;
        let create_case = repository.test_cases()
            .create(TestCase::builder()
                .customer_id("cust1".to_owned())
                .name("Test Case".to_owned())
                .description("desc".to_owned())
                .build())
            .await;


        let get_result = repository.test_cases().get("cust1".to_string(), create_case.id.clone()).await;
        println!("{:?}", get_result);

        repository.test_cases()
            .delete(&create_case.customer_id, &create_case.id.to_string()).await;

        let result = repository.test_cases()
            .get(create_case.customer_id.clone(), create_case.customer_id).await;

        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }
}


