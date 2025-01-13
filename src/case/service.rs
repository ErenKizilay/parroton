use crate::api::AppError;
use crate::persistence::repo::{build_composite_key, OnDeleteMessage, QueryResult, Table};
use aws_sdk_dynamodb::types::AttributeValue;
use aws_sdk_dynamodb::Client;
use std::sync::Arc;
use tokio::sync::mpsc;
use crate::action::service::ActionsTable;
use crate::action_execution::service::ActionExecutionTable;
use crate::assertion::service::AssertionsTable;
use crate::auth::service::AuthProviderOperations;
use crate::case::model::TestCase;
use crate::parameter::service::ParametersTable;
use crate::run::service::RunTable;

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
    pub async fn create_test_case(&self, test_case: TestCase) -> TestCase {
        TestCaseTable::put_item(self.client.clone(), test_case)
            .await
            .unwrap()
    }

    pub async fn list(
        &self,
        customer_id: String,
        next_page_key: Option<String>,
    ) -> Result<QueryResult<TestCase>, AppError> {
        TestCaseTable::list_items(self.client.clone(), customer_id, next_page_key).await
    }

    pub async fn get(
        &self,
        customer_id: String,
        test_case_id: String,
    ) -> Result<Option<TestCase>, AppError> {
        TestCaseTable::get_item(self.client.clone(), customer_id, test_case_id).await
    }

    pub async fn delete(&self, customer_id: &String, test_case_id: &String) {
        let (tx, mut rx) = mpsc::channel(32);
        let deleted_test_case = TestCaseTable::delete_item(
            self.client.clone(),
            customer_id.clone(),
            test_case_id.clone(),
        )
            .await;
        if let Ok(Some(deleted_case)) = deleted_test_case {
            tx.send(OnDeleteMessage::TestCaseDeleted(deleted_case))
                .await
                .unwrap();
        }
        let tx3 = tx.clone();
        let cloned_client = self.client.clone();
        tokio::task::spawn(async move {
            while let Some(message) = rx.recv().await {
                match message {
                    OnDeleteMessage::TestCaseDeleted(test_case) => {
                        ActionsTable::delete_all_items(
                            cloned_client.clone(),
                            build_composite_key(vec![
                                test_case.customer_id.clone(),
                                test_case.id.clone(),
                            ]),
                            None,
                            &tx3,
                        )
                            .await;
                        RunTable::delete_all_items(
                            cloned_client.clone(),
                            build_composite_key(vec![
                                test_case.customer_id.clone(),
                                test_case.id.clone(),
                            ]),
                            None,
                            &tx3,
                        )
                            .await;
                        AssertionsTable::delete_all_items(cloned_client.clone(), test_case.id.clone(), None, &tx3)
                            .await;
                        AuthProviderOperations {
                            client: cloned_client.clone(),
                        }.unlink_test_case(&test_case.customer_id, &test_case.id).await;
                    }
                    OnDeleteMessage::ActionDeleted(action) => {
                        ParametersTable::delete_all_items(
                            cloned_client.clone(),
                            build_composite_key(vec![
                                action.customer_id.clone(),
                                action.test_case_id.clone(),
                            ]),
                            None,
                            &tx3,
                        )
                            .await;
                        AssertionsTable::delete_all_items(
                            cloned_client.clone(),
                            build_composite_key(vec![
                                action.customer_id.clone(),
                                action.test_case_id.clone(),
                            ]),
                            None,
                            &tx3,
                        )
                            .await;
                    }
                    OnDeleteMessage::RunDeleted(run) => {
                        ActionExecutionTable::delete_all_items(
                            cloned_client.clone(),
                            build_composite_key(vec![run.customer_id, run.test_case_id, run.id]),
                            None,
                            &tx3,
                        )
                            .await;
                    }
                }
            }
        });
    }
}
