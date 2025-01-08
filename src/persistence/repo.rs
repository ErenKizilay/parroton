use crate::api::AppError;
use crate::models::{Action, TestCase};
use crate::persistence::action_executions::ActionExecutionsOperations;
use crate::persistence::actions::ActionOperations;
use crate::persistence::assertions::AssertionOperations;
use crate::persistence::auth_providers::AuthProviderOperations;
use crate::persistence::parameters::ParameterOperations;
use crate::persistence::runs::RunOperations;
use crate::persistence::test_cases::TestCaseOperations;
use aws_config::BehaviorVersion;
use aws_sdk_dynamodb::config::http::HttpResponse;
use aws_sdk_dynamodb::error::{ProvideErrorMetadata, SdkError};
use aws_sdk_dynamodb::operation::query::builders::QueryFluentBuilder;
use aws_sdk_dynamodb::operation::query::{QueryError, QueryOutput};
use aws_sdk_dynamodb::operation::update_item::builders::UpdateItemFluentBuilder;
use aws_sdk_dynamodb::types::{AttributeValue, ComparisonOperator, Condition, DeleteRequest, KeysAndAttributes, PutRequest, WriteRequest};
use aws_sdk_dynamodb::Client;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_dynamo::aws_sdk_dynamodb_1::to_item;
use serde_dynamo::{from_attribute_value, from_item};
use std::collections::HashMap;
use std::sync::Arc;
use aws_sdk_dynamodb::operation::batch_get_item::{BatchGetItemError, BatchGetItemOutput};
use tokio::sync::mpsc::Sender;

pub struct PageKey {
    keys: HashMap<String, String>,
}

#[derive(Deserialize, Clone, PartialEq)]
pub enum ParameterIn {
    Header,
    Cookie,
    Query,
    Body,
}

impl PageKey {
    pub fn from_attribute_values(values: HashMap<String, AttributeValue>) -> Self {
        let mut keys: HashMap<String, String> = HashMap::new();
        values.iter().for_each(|(k, v)| {
            keys.insert(
                k.to_string(),
                v.as_s().map_or(String::new(), |v| v.to_string()),
            );
        });
        Self { keys }
    }

    pub fn to_attribute_values(&self) -> HashMap<String, AttributeValue> {
        let mut keys: HashMap<String, AttributeValue> = HashMap::new();
        self.keys.iter().for_each(|(k, v)| {
            keys.insert(k.to_string(), AttributeValue::S(v.to_string()));
        });
        keys
    }

    pub fn to_next_page_key(&self) -> String {
        serde_json::to_string(&self.keys).unwrap()
    }

    pub fn from_next_page_key(keys: &String) -> Self {
        Self {
            keys: serde_json::from_str(&keys).unwrap(),
        }
    }
}

#[derive(Clone, Serialize, Debug)]
pub struct QueryResult<T>
where
    T: DeserializeOwned + Serialize + Clone,
{
    pub items: Vec<T>,
    pub next_page_key: Option<String>,
}

pub(crate) trait Table<T>
where
    T: DeserializeOwned + Serialize + Clone,
{
    fn table_name() -> String;
    fn partition_key_name() -> String;
    fn sort_key_name() -> String;

    fn partition_key(value: String) -> (String, AttributeValue) {
        (Self::partition_key_name(), AttributeValue::S(value))
    }

    fn partition_key_from_entity(entity: &T) -> (String, AttributeValue);
    fn sort_key_from_entity(entity: &T) -> (String, AttributeValue);

    fn sort_key(value: String) -> (String, AttributeValue) {
        (Self::sort_key_name(), AttributeValue::S(value))
    }

    fn unique_key(partition_key: String, sort_key: String) -> HashMap<String, AttributeValue> {
        HashMap::from([Self::partition_key(partition_key), Self::sort_key(sort_key)])
    }

    fn partition_key_condition(value: String) -> (String, Condition) {
        (
            Self::partition_key_name(),
            Self::key_condition(value, ComparisonOperator::Eq),
        )
    }
    fn sort_key_condition(value: String) -> (String, Condition) {
        (
            Self::sort_key_name(),
            Self::key_condition(value, ComparisonOperator::Eq),
        )
    }

    fn unique_key_condition(partition_key: String, sort_key: String) -> HashMap<String, Condition> {
        HashMap::from([
            Self::partition_key_condition(partition_key),
            Self::sort_key_condition(sort_key),
        ])
    }

    fn key_condition(value: String, operator: ComparisonOperator) -> Condition {
        Condition::builder()
            .comparison_operator(operator)
            .attribute_value_list(AttributeValue::S(value.to_string()))
            .build()
            .unwrap()
    }

    async fn get_item(client: Arc<Client>, partition_key: String, sort_key: String) -> Result<Option<T>, AppError> {
        let result = client
            .get_item()
            .table_name(Self::table_name())
            .set_key(Some(Self::unique_key(partition_key, sort_key)))
            .consistent_read(true)
            .send()
            .await;
        match result {
            Ok(output) => match output.item {
                Some(item_map) => {
                    Ok(Some(from_item(item_map).unwrap()))
                }
                None => Ok(None),
            },
            Err(e) => Err(AppError::Internal(e.to_string())),
        }
    }

    async fn put_item(client: Arc<Client>, entity: T) -> Result<T, AppError> {
        let mut item = to_item(entity.clone()).unwrap();
        Self::add_main_key_attributes(&entity, &mut item);
        let result = client
            .put_item()
            .table_name(Self::table_name())
            .set_item(Some(item))
            .send()
            .await;
        match result {
            Ok(_) => Ok(entity.clone()),
            Err(err) => {
                Err(AppError::Internal(err.to_string()))
            }
        }
    }

    async fn delete_item(client: Arc<Client>, partition_key: String, sort_key: String) -> Result<Option<T>, AppError> {
        let result = client
            .delete_item()
            .table_name(Self::table_name())
            .set_key(Some(Self::unique_key(partition_key, sort_key)))
            .send()
            .await;
        match result {
            Ok(output) => output.attributes.map_or(Ok(None), |item_map| {
                Ok(Some(from_attribute_value(AttributeValue::M(item_map)).unwrap()))
            }),
            Err(err) => Err(AppError::Internal(err.to_string())),
        }
    }

    fn query_builder(client: Arc<Client>) -> QueryFluentBuilder {
        client.query().table_name(Self::table_name())
    }

    fn update_builder(client: Arc<Client>) -> UpdateItemFluentBuilder {
        client.update_item().table_name(Self::table_name())
    }

    async fn batch_get_items(client: Arc<Client>, key_pairs: Vec<(String, String)>) -> Result<Vec<T>, AppError> {
        let keys = key_pairs.iter()
            .map(|key_pair| { Self::unique_key(key_pair.0.clone(), key_pair.1.clone()) })
            .collect();
        let table_name = Self::table_name();
        let result = client.batch_get_item()
            .request_items(&table_name, KeysAndAttributes::builder()
                .consistent_read(true)
                .set_keys(Some(keys))
                .build().unwrap())
            .send().await;
        match result {
            Ok(batch_get_item_output) => {
                batch_get_item_output.responses
                    .map_or(Ok(vec![]), |items_by_table|{
                        Ok(items_by_table.get(&table_name).unwrap()
                            .iter()
                            .map(|item|{from_item(item.clone()).unwrap()})
                            .collect())
                        })
            }
            Err(err) => {
                Err(AppError::Internal(err.to_string()))
            }
        }
    }

    fn from_query_result(
        result: Result<QueryOutput, SdkError<QueryError, HttpResponse>>,
    ) -> Result<QueryResult<T>, AppError> {
        match result {
            Ok(output) => {
                let items = output.items.map_or(vec![], |items| {
                    items
                        .iter()
                        .map(|item| from_attribute_value(AttributeValue::M(item.clone())).unwrap())
                        .collect()
                });
                Ok(QueryResult {
                    items,
                    next_page_key: output.last_evaluated_key.map(|last_key| {
                        PageKey::from_attribute_values(last_key).to_next_page_key()
                    }),
                })
            }
            Err(err) => Err(AppError::Internal(err.to_string())),
        }
    }

    async fn list_items(
        client: Arc<Client>,
        partition_key: String,
        next_page_key: Option<String>,
    ) -> Result<QueryResult<T>, AppError> {
        let result = Self::query_builder(client)
            .expression_attribute_names("#pk", Self::partition_key_name())
            .expression_attribute_values(":pk", AttributeValue::S(partition_key))
            .key_condition_expression("#pk = :pk")
            .set_exclusive_start_key(
                next_page_key.map(|next| PageKey::from_next_page_key(&next).to_attribute_values()),
            )
            .send()
            .await;
        Self::from_query_result(result)
    }

    async fn list_all_items(
        client: Arc<Client>,
        partition_key: String,
    ) -> Result<Vec<T>, AppError> {
        let mut app_error = None;
        let mut next_page_key = None;
        let mut items: Vec<T> = vec![];
        loop {
            let result = Self::list_items(client.clone(), partition_key.clone(), next_page_key.clone()).await;
            match result {
                Ok(query_result) => {
                    items.extend(query_result.items);
                    next_page_key = query_result.next_page_key;
                }
                Err(err) => {
                    app_error = Some(err);
                }
            }
            if app_error.is_some() {
                break;
            }
            if next_page_key.is_none() {
                break;
            }
        }

        match app_error {
            None => {
                Ok(items)
            }
            Some(app_err) => {
                Err(app_err)
            }
        }
    }

    async fn delete_all_items(client: Arc<Client>, partition_key: String, mut next_page_key: Option<String>, sender: &Sender<OnDeleteMessage>) {
        loop {
            let result = Self::list_items(client.clone(), partition_key.clone(), next_page_key.clone()).await;
            if let Ok(query_result) = result {
                let mut keys: Vec<(String, String)> = vec![];
                for item in query_result.items {
                    let attribute_value = Self::partition_key_from_entity(&item).1;
                    let partition_key = attribute_value.as_s().unwrap();
                    let sort_key = attribute_value.as_s().unwrap();
                    keys.push((partition_key.clone(), sort_key.clone()));
                    if let Some(event) = Self::build_deleted_event(item) {
                        let cloned_sender = sender.clone();
                        tokio::task::spawn(async move {
                            cloned_sender.send(event).await.unwrap();
                        });
                    }
                }

                Self::batch_delete_items(client.clone(), keys).await;
                if let Some(page_key) = query_result.next_page_key {
                    next_page_key = Some(page_key.clone());
                } else {
                    break;
                }
            }
        }
    }

    async fn batch_put_item(client: Arc<Client>, entities: Vec<T>) {
        let write_requests: Vec<WriteRequest> = entities
            .iter()
            .map(|entity| {
                let mut item = to_item(entity).unwrap();
                Self::add_main_key_attributes(&entity, &mut item);
                WriteRequest::builder()
                    .put_request(PutRequest::builder().set_item(Some(item)).build().unwrap())
                    .build()
            })
            .collect();
        batch_write(client, write_requests, &Self::table_name()).await;
    }

    async fn batch_delete_items(client: Arc<Client>, keys: Vec<(String, String)>) {
        let cloned_client = client.clone();
        tokio::task::spawn(async move {
            let write_requests: Vec<WriteRequest> = keys
                .iter()
                .map(|key| {
                    WriteRequest::builder()
                        .delete_request(DeleteRequest::builder()
                            .set_key(Some(Self::unique_key(key.0.clone(), key.1.clone())))
                            .build().unwrap())
                        .build()
                })
                .collect();
            batch_write(cloned_client, write_requests, &Self::table_name()).await;
        });
    }

    fn add_main_key_attributes(entity: &T, mut item: &mut HashMap<String, AttributeValue>) {
        let partition_key = Self::partition_key_from_entity(&entity);
        let sort_key = Self::sort_key_from_entity(&entity);
        item.insert(partition_key.0, partition_key.1);
        item.insert(sort_key.0, sort_key.1);
        Self::add_index_key_attributes(&entity, item);
    }

    fn add_index_key_attributes(entity: &T, mut item: &mut HashMap<String, AttributeValue>) {}

    fn build_deleted_event(entity: T) -> Option<OnDeleteMessage> {
        None
    }
}

#[derive(Clone)]
pub struct Repository {
    client: Arc<Client>,
}

impl Repository {
    pub async fn new() -> Self {
        let config = aws_config::load_defaults(BehaviorVersion::latest()).await;
        let client = Client::new(&config);
        Repository {
            client: Arc::new(client),
        }
    }

    pub fn runs(&self) -> RunOperations {
        RunOperations {
            client: Arc::clone(&self.client),
        }
    }

    pub fn parameters(&self) -> ParameterOperations {
        ParameterOperations {
            client: Arc::clone(&self.client),
        }
    }
    pub fn test_cases(&self) -> TestCaseOperations {
        TestCaseOperations {
            client: Arc::clone(&self.client),
        }
    }

    pub fn action_executions(&self) -> ActionExecutionsOperations {
        ActionExecutionsOperations {
            client: Arc::clone(&self.client),
        }
    }

    pub fn assertions(&self) -> AssertionOperations {
        AssertionOperations {
            client: Arc::clone(&self.client),
        }
    }

    pub fn actions(&self) -> ActionOperations {
        ActionOperations {
            client: Arc::clone(&self.client),
        }
    }

    pub fn auth_providers(&self) -> AuthProviderOperations {
        AuthProviderOperations {
            client: Arc::clone(&self.client),
        }
    }
}

pub(crate) fn build_composite_key(keys: Vec<String>) -> String {
    keys.join("#")
}

async fn batch_write(client: Arc<Client>, write_requests: Vec<WriteRequest>, table_name: &String) {
    let chunks: Vec<Vec<WriteRequest>> = write_requests.chunks(25)
        .map(|chunk| { chunk.to_vec() })
        .collect();
    for write_chunk in chunks {
        let cloned_client = client.clone();
        let table_name_cloned = table_name.clone();
        tokio::task::spawn(async move {
            let result = cloned_client
                .batch_write_item()
                .set_request_items(Some(HashMap::from([(
                    table_name_cloned,
                    write_chunk,
                )])))
                .send()
                .await;
            match result {
                Ok(_) => {}
                Err(err) => {
                    println!(
                        "batch write error: {}",
                        err.into_service_error().message().unwrap_or_default()
                    );
                }
            }
        });
    }
}

pub(crate) enum OnDeleteMessage {
    TestCaseDeleted(TestCase),
    ActionDeleted(Action),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Run, RunStatus};
    #[test]
    fn test() {
        let result = QueryResult {
            items: vec![Run {
                customer_id: "".to_string(),
                test_case_id: "".to_string(),
                id: "".to_string(),
                status: RunStatus::InProgress,
                started_at: "".to_string(),
                finished_at: None,
            }],
            next_page_key: Some("asdasd".to_string()),
        };
        println!("{:?}", serde_json::to_string(&result));
    }
}
