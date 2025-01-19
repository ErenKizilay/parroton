use crate::action::model::Action;
use crate::action::service::ActionOperations;
use crate::action_execution::service::ActionExecutionsOperations;
use crate::api::AppError;
use crate::assertion::service::AssertionOperations;
use crate::auth::service::AuthProviderOperations;
use crate::case::model::TestCase;
use crate::case::service::TestCaseOperations;
use crate::parameter::service::ParameterOperations;
use crate::persistence::model::{ListItemsRequest, PageKey, QueryResult};
use crate::run::model::Run;
use crate::run::service::RunOperations;
use aws_config::meta::region::RegionProviderChain;
use aws_config::{BehaviorVersion, SdkConfig};
use aws_sdk_dynamodb::config::http::HttpResponse;
use aws_sdk_dynamodb::config::{Credentials, ProvideCredentials, SharedCredentialsProvider};
use aws_sdk_dynamodb::error::{ProvideErrorMetadata, SdkError};
use aws_sdk_dynamodb::operation::query::builders::QueryFluentBuilder;
use aws_sdk_dynamodb::operation::query::{QueryError, QueryOutput};
use aws_sdk_dynamodb::operation::update_item::builders::UpdateItemFluentBuilder;
use aws_sdk_dynamodb::operation::update_item::{UpdateItemError, UpdateItemOutput};
use aws_sdk_dynamodb::types::builders::UpdateBuilder;
use aws_sdk_dynamodb::types::{AttributeValue, ComparisonOperator, Condition, DeleteRequest, KeysAndAttributes, PutRequest, ReturnValue, WriteRequest};
use aws_sdk_dynamodb::Client;
use futures::future::err;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_dynamo::aws_sdk_dynamodb_1::to_item;
use serde_dynamo::{from_attribute_value, from_item, to_attribute_value};
use std::cmp::Ordering;
use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::{Arc, Once};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc::Sender;
use tracing::{info, Instrument};

pub static INIT: Once = Once::new();

pub fn init_logger() {
    INIT.call_once(|| {
        tracing_subscriber::fmt::init();
    });
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

    fn build_exclusion_key(key: Option<String>) -> Option<HashMap<String, AttributeValue>> {
        key.map(|k| { PageKey::from_next_page_key(&k).to_attribute_values() })
    }

    async fn get_item(
        client: Arc<Client>,
        partition_key: String,
        sort_key: String,
    ) -> Result<Option<T>, AppError> {
        let result = client
            .get_item()
            .table_name(Self::table_name())
            .set_key(Some(Self::unique_key(partition_key, sort_key)))
            .consistent_read(true)
            .send()
            .await;
        match result {
            Ok(output) => match output.item {
                Some(item_map) => Ok(Some(from_item(item_map).unwrap())),
                None => Ok(None),
            },
            Err(e) => Err(from_sdk_error(e)),
        }
    }

    async fn update_partial(
        partition_key: String,
        sort_key: String,
        update_builder: UpdateItemFluentBuilder,
    ) -> Result<T, AppError> {
        let mut update_expression = update_builder.get_update_expression().clone()
            .unwrap();
        update_expression.push_str(format!("{} #updated_at = :updated_at", if update_expression.contains("SET") { "," } else { " SET" }).as_str());
        info!("will update partially {}|{} with expr: {:?}, attribute names: {:?}, attributes values: {:?}", partition_key, sort_key, update_expression, update_builder.get_expression_attribute_names(), update_builder.get_expression_attribute_values());
        let result = update_builder
            .table_name(Self::table_name())
            .set_key(Some(Self::unique_key(
                partition_key,
                sort_key,
            )))
            .return_values(ReturnValue::AllNew)
            .expression_attribute_names("#pk", Self::partition_key_name())
            .expression_attribute_names("#sk", Self::sort_key_name())
            .expression_attribute_names("#updated_at", "updated_at")
            .condition_expression("attribute_exists(#pk) AND attribute_exists(#sk)")
            .expression_attribute_values(":updated_at", to_attribute_value(current_timestamp()).unwrap())
            .update_expression(update_expression)
            .send().await;
        Self::from_update_result(result)
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
            Err(err) => Err(from_sdk_error(err)),
        }
    }

    async fn delete_item(
        client: Arc<Client>,
        partition_key: String,
        sort_key: String,
    ) -> Result<Option<T>, AppError> {
        info!("{}:will delete: {}|{}", Self::table_name(),  partition_key, sort_key);
        let result = client
            .delete_item()
            .table_name(Self::table_name())
            .set_key(Some(Self::unique_key(partition_key, sort_key)))
            .return_values(ReturnValue::AllOld)
            .send()
            .await;
        match result {
            Ok(output) => output.attributes.map_or(Ok(None), |item_map| {
                Ok(Some(
                    from_attribute_value(AttributeValue::M(item_map)).unwrap(),
                ))
            }),
            Err(err) => Err(from_sdk_error(err)),
        }
    }

    fn query_builder(client: Arc<Client>) -> QueryFluentBuilder {
        client.query().table_name(Self::table_name())
            .limit(50)
    }

    async fn batch_get_items(
        client: Arc<Client>,
        key_pairs: Vec<(String, String)>,
    ) -> Result<Vec<T>, AppError> {
        if key_pairs.is_empty() {
            return Ok(vec![]);
        }
        let keys = key_pairs
            .iter()
            .map(|key_pair| Self::unique_key(key_pair.0.clone(), key_pair.1.clone()))
            .collect();
        let table_name = Self::table_name();
        let result = client
            .batch_get_item()
            .request_items(
                &table_name,
                KeysAndAttributes::builder()
                    .consistent_read(true)
                    .set_keys(Some(keys))
                    .build()
                    .unwrap(),
            )
            .send()
            .await;
        match result {
            Ok(batch_get_item_output) => {
                batch_get_item_output
                    .responses
                    .map_or(Ok(vec![]), |items_by_table| {
                        let mut items: Vec<T> = items_by_table
                            .get(&table_name)
                            .unwrap()
                            .iter()
                            .map(|item| from_item(item.clone()).unwrap())
                            .collect();
                        items.sort_by(Self::ordering);
                        Ok(items)
                    })
            }
            Err(err) => Err(from_sdk_error(err)),
        }
    }

    fn from_query_result(
        result: Result<QueryOutput, SdkError<QueryError, HttpResponse>>,
    ) -> Result<QueryResult<T>, AppError> {
        match result {
            Ok(output) => {
                let mut items = output.items.map_or(vec![], |items| {
                    items
                        .iter()
                        .map(|item| from_attribute_value(AttributeValue::M(item.clone())).unwrap())
                        .collect()
                });
                items.sort_by(Self::ordering);
                Ok(QueryResult {
                    items,
                    next_page_key: output.last_evaluated_key.map(|last_key| {
                        PageKey::from_attribute_values(last_key).to_next_page_key()
                    }),
                })
            }
            Err(err) => {
                Err(from_sdk_error(err))
            }
        }
    }

    fn from_update_result(
        result: Result<UpdateItemOutput, SdkError<UpdateItemError, HttpResponse>>,
    ) -> Result<T, AppError> {
        match result {
            Ok(output) => {
                Ok(serde_dynamo::aws_sdk_dynamodb_1::from_attribute_value(
                    output
                        .attributes
                        .map_or(AttributeValue::M(HashMap::new()), |v| AttributeValue::M(v)),
                )
                    .unwrap())
            }
            Err(err) => {
                Err(from_sdk_error(err))
            }
        }
    }

    async fn list_items(
        client: Arc<Client>,
        request: ListItemsRequest,
    ) -> Result<QueryResult<T>, AppError> {
        let mut expr_attribute_names: HashMap<String, String> = HashMap::from([("#pk".to_string(), Self::partition_key_name())]);
        request.expression_attribute_names.inspect(|names| {
            expr_attribute_names.extend(names.clone());
        });
        let mut expr_attribute_values: HashMap<String, AttributeValue> = HashMap::from([(":pk".to_string(), AttributeValue::S(request.partition_key))]);
        request.expression_attribute_values.inspect(|values| {
            expr_attribute_values.extend(values.clone());
        });
        let result = Self::query_builder(client)
            .set_expression_attribute_names(Some(expr_attribute_names))
            .set_expression_attribute_values(Some(expr_attribute_values))
            .key_condition_expression("#pk = :pk")
            .set_filter_expression(request.filter_expression)
            .limit(request.limit.map_or(25, |limit| { limit }))
            .set_exclusive_start_key(
                request.next_page_key.map(|next| PageKey::from_next_page_key(&next).to_attribute_values()),
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
            let result =
                Self::list_items(client.clone(), ListItemsRequest::builder()
                    .partition_key(partition_key.clone())
                    .maybe_next_page_key(next_page_key.clone())
                    .build())
                    .await;
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
            None => Ok(items),
            Some(app_err) => Err(app_err),
        }
    }

    async fn delete_all_items(
        client: Arc<Client>,
        partition_key: String,
        sender: &Sender<OnDeleteMessage>,
    ) {
        info!("{}: will delete all items for partition {}", Self::table_name(), partition_key);
        let mut next_page_key: Option<String> = None;
        loop {
            let result =
                Self::list_items(client.clone(), ListItemsRequest::builder()
                    .partition_key(partition_key.clone())
                    .maybe_next_page_key(next_page_key.clone()).build())
                    .await;
            if let Ok(query_result) = result {
                let mut keys: Vec<(String, String)> = vec![];
                for item in query_result.items {
                    let partition_key = Self::partition_key_from_entity(&item).1;
                    let sort_key = Self::sort_key_from_entity(&item).1;
                    keys.push((partition_key.as_s().unwrap().clone(), sort_key.as_s().unwrap().clone()));
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
        info!("{}:will batch delete {} items!", Self::table_name(), keys.len());
        let cloned_client = client.clone();
        tokio::task::spawn(async move {
            let write_requests: Vec<WriteRequest> = keys
                .iter()
                .map(|key| {
                    WriteRequest::builder()
                        .delete_request(
                            DeleteRequest::builder()
                                .set_key(Some(Self::unique_key(key.0.clone(), key.1.clone())))
                                .build()
                                .unwrap(),
                        )
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
        item.insert("created_at".to_string(), AttributeValue::N(current_timestamp().to_string()));
        Self::add_index_key_attributes(&entity, item);
    }

    fn add_index_key_attributes(entity: &T, mut item: &mut HashMap<String, AttributeValue>) {}

    fn build_deleted_event(entity: T) -> Option<OnDeleteMessage> {
        None
    }

    fn ordering(e1: &T, e2: &T) -> Ordering {
        Ordering::Equal
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
    let chunks: Vec<Vec<WriteRequest>> = write_requests
        .chunks(25)
        .map(|chunk| chunk.to_vec())
        .collect();
    for write_chunk in chunks {
        let cloned_client = client.clone();
        let table_name_cloned = table_name.clone();
        let table_name_cloned2 = table_name.clone();
        tokio::task::spawn(async move {
            let result = cloned_client
                .batch_write_item()
                .set_request_items(Some(HashMap::from([(table_name_cloned, write_chunk)])))
                .send()
                .await;
            match result {
                Ok(_) => {
                    info!("{}:batch_write_item ok", table_name_cloned2);
                }
                Err(err) => {
                    println!("{}: batch write error: {}", table_name_cloned2, err.into_service_error().message().unwrap_or_default());
                }
            }
        });
    }
}

#[derive(Debug)]
pub(crate) enum OnDeleteMessage {
    TestCaseDeleted(TestCase),
    ActionDeleted(Action),
    RunDeleted(Run),
}

pub fn current_timestamp() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64
}

fn from_sdk_error<T>(sdk_err: SdkError<T>) -> AppError
where
    T: Debug,
    T: ProvideErrorMetadata,
{
    tracing::error!("aws dynamodb sdk error: {:?}", sdk_err);
    let message = sdk_err.message()
        .map_or_else(|| sdk_err.to_string(), |err| { err.to_string() });
    AppError::Internal(message)
}
