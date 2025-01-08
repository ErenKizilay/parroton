use std::cmp::Ordering;
use std::collections::HashMap;
use std::sync::Arc;
use aws_sdk_dynamodb::Client;
use aws_sdk_dynamodb::types::AttributeValue;
use crate::action::model::Action;
use crate::api::AppError;
use crate::persistence::repo::{build_composite_key, OnDeleteMessage, PageKey, QueryResult, Table};

pub struct ActionOperations {
    pub(crate) client: Arc<Client>,
}
pub(crate) struct ActionsTable();

impl Table<Action> for ActionsTable {
    fn table_name() -> String {
        "actions".to_string()
    }

    fn partition_key_name() -> String {
        "customer_id#test_case_id".to_string()
    }

    fn sort_key_name() -> String {
        "id".to_string()
    }

    fn partition_key_from_entity(entity: &Action) -> (String, AttributeValue) {
        Self::partition_key(build_composite_key(vec![
            entity.customer_id.clone(),
            entity.test_case_id.clone(),
        ]))
    }

    fn sort_key_from_entity(entity: &Action) -> (String, AttributeValue) {
        Self::sort_key(build_composite_key(vec![entity.id.clone()]))
    }



    fn add_index_key_attributes(entity: &Action, item: &mut HashMap<String, AttributeValue>) {
        item.insert(
            "name".to_string(),
            AttributeValue::S(entity.name.to_ascii_lowercase()),
        );
    }

    fn build_deleted_event(entity: Action) -> Option<OnDeleteMessage> {
        Some(OnDeleteMessage::ActionDeleted(entity))
    }

    fn ordering(e1: &Action, e2: &Action) -> Ordering {
        e1.order.cmp(&e2.order)
    }
}

impl ActionOperations {

    pub async fn list(
        &self,
        customer_id: String,
        test_case_id: String,
        next_page_key: Option<String>,
    ) -> Result<QueryResult<Action>, AppError> {
        ActionsTable::list_items(
            self.client.clone(),
            build_composite_key(vec![customer_id, test_case_id]),
            next_page_key,
        )
            .await
    }

    pub async fn list_previous(
        &self,
        customer_id: String,
        test_case_id: String,
        before_order: usize,
        next_page_key: Option<String>,
    ) -> Result<QueryResult<Action>, AppError> {
        let partition_key =
            ActionsTable::partition_key(build_composite_key(vec![customer_id, test_case_id]));
        let result = ActionsTable::query_builder(self.client.clone())
            .expression_attribute_names("#pk", partition_key.0)
            .expression_attribute_names("#order", "order")
            .expression_attribute_values(":pk", partition_key.1)
            .expression_attribute_values(":order", AttributeValue::N(before_order.to_string()))
            .key_condition_expression("#pk = :pk")
            .filter_expression("#order < :order")
            .set_exclusive_start_key(
                next_page_key.map(|next| PageKey::from_next_page_key(&next).to_attribute_values()),
            )
            .send()
            .await;

        ActionsTable::from_query_result(result)
    }

    pub async fn get(
        &self,
        customer_id: String,
        test_case_id: String,
        id: String,
    ) -> Result<Option<Action>, AppError> {
        ActionsTable::get_item(self.client.clone(), build_composite_key(vec![customer_id, test_case_id]), id)
            .await

    }

    pub async fn get_action_by_name(
        &self,
        customer_id: String,
        test_case_id: String,
        name: String,
    ) -> Option<Action> {
        let partition_key =
            ActionsTable::partition_key(build_composite_key(vec![customer_id, test_case_id]));
        let result = ActionsTable::query_builder(self.client.clone())
            .index_name("name_index".to_string())
            .expression_attribute_names("#pk", partition_key.0)
            .expression_attribute_names("#sk", "name")
            .expression_attribute_values(":pk", partition_key.1)
            .expression_attribute_values(":sk", AttributeValue::S(name.to_string()))
            .key_condition_expression("#pk = :pk AND #sk = :sk")
            .send()
            .await;

        ActionsTable::from_query_result(result)
            .map_or(None, |mut query_result: QueryResult<Action>|{query_result.items.pop()})

    }

    pub async fn batch_create(&self, actions: Vec<Action>) {
        ActionsTable::batch_put_item(self.client.clone(), actions).await
    }
}