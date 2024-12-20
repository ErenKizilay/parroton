use crate::models::{
    Action, ActionExecution, AuthenticationProvider, Parameter, ParameterLocation, ParameterType,
    Run, RunStatus, TestCase,
};
use aws_config::BehaviorVersion;
use aws_sdk_dynamodb::config::http::HttpResponse;
use aws_sdk_dynamodb::error::{ProvideErrorMetadata, SdkError};
use aws_sdk_dynamodb::operation::put_item::PutItemError;
use aws_sdk_dynamodb::operation::query::builders::QueryFluentBuilder;
use aws_sdk_dynamodb::operation::query::{QueryError, QueryOutput};
use aws_sdk_dynamodb::operation::update_item::builders::UpdateItemFluentBuilder;
use aws_sdk_dynamodb::types::{
    AttributeValue, ComparisonOperator, Condition, PutRequest, WriteRequest,
};
use aws_sdk_dynamodb::Client;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_dynamo::aws_sdk_dynamodb_1::to_item;
use serde_dynamo::{from_attribute_value, from_item};
use std::collections::HashMap;
use std::future::Future;
use std::time::SystemTime;
use aws_sdk_dynamodb::primitives::DateTime;
use aws_sdk_dynamodb::primitives::DateTimeFormat::DateTimeWithOffset;

struct PageKey {
    keys: HashMap<String, String>,
}

#[derive(Deserialize, Clone)]
pub enum ParameterIn {
    Header,
    Cookie,
    Query,
    Body,
}

impl PageKey {
    fn from_attribute_values(values: HashMap<String, AttributeValue>) -> Self {
        let mut keys: HashMap<String, String> = HashMap::new();
        values.iter().for_each(|(k, v)| {
            keys.insert(
                k.to_string(),
                v.as_s().map_or(String::new(), |v| v.to_string()),
            );
        });
        Self { keys }
    }

    fn to_attribute_values(&self) -> HashMap<String, AttributeValue> {
        let mut keys: HashMap<String, AttributeValue> = HashMap::new();
        self.keys.iter().for_each(|(k, v)| {
            keys.insert(k.to_string(), AttributeValue::S(v.to_string()));
        });
        keys
    }

    fn to_next_page_key(&self) -> String {
        serde_json::to_string(&self.keys).unwrap()
    }

    fn from_next_page_key(keys: &String) -> Self {
        Self {
            keys: serde_json::from_str(&keys).unwrap(),
        }
    }
}
pub struct QueryResult<T>
where
    T: DeserializeOwned + Serialize,
{
    pub items: Vec<T>,
    pub next_page_key: Option<String>,
}

trait Table<T>
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

    async fn get_item(client: &Client, partition_key: String, sort_key: String) -> Option<T> {
        let result = client
            .get_item()
            .table_name(Self::table_name())
            .set_key(Some(Self::unique_key(partition_key, sort_key)))
            .consistent_read(true)
            .send()
            .await;
        match result {
            Ok(output) => match output.item {
                Some(item_map) => Some(from_item(item_map).unwrap()),
                None => None,
            },
            Err(_) => None,
        }
    }

    async fn put_item(client: &Client, entity: T) -> Result<T, PutItemError> {
        let mut item = to_item(entity.clone()).unwrap();
        Self::add_main_key_attributes(&entity, &mut item);
        let result = client
            .put_item()
            .table_name(Self::table_name())
            .set_item(Some(item))
            .send()
            .await;
        match result {
            Ok(output) => Ok(entity.clone()),
            Err(err) => {
                println!("put_item error: {}", err);
                Err(err.into_service_error())
            }
        }
    }

    async fn delete_item(client: &Client, partition_key: String, sort_key: String) -> Option<T> {
        let result = client
            .delete_item()
            .table_name(Self::table_name())
            .set_key(Some(Self::unique_key(partition_key, sort_key)))
            .send()
            .await;
        match result {
            Ok(output) => output.attributes.map_or(None, |item_map| {
                from_attribute_value(AttributeValue::M(item_map)).unwrap()
            }),
            Err(err) => None,
        }
    }

    fn query_builder(client: &Client) -> QueryFluentBuilder {
        client.query().table_name(Self::table_name())
    }

    fn update_builder(client: &Client) -> UpdateItemFluentBuilder {
        client.update_item().table_name(Self::table_name())
    }

    fn from_query_result(
        result: Result<QueryOutput, SdkError<QueryError, HttpResponse>>,
    ) -> QueryResult<T> {
        match result {
            Ok(output) => {
                let items = output.items.map_or(vec![], |items| {
                    items
                        .iter()
                        .map(|item| from_attribute_value(AttributeValue::M(item.clone())).unwrap())
                        .collect()
                });
                QueryResult {
                    items,
                    next_page_key: output.last_evaluated_key.map(|last_key| {
                        PageKey::from_attribute_values(last_key).to_next_page_key()
                    }),
                }
            }
            Err(_) => QueryResult {
                items: vec![],
                next_page_key: None,
            },
        }
    }

    async fn list_items(
        client: &Client,
        partition_key: String,
        next_page_key: Option<String>,
    ) -> QueryResult<T> {
        let result = Self::query_builder(&client)
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

    async fn batch_put_item(client: &Client, entities: Vec<T>) {
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
        for write_chunk in write_requests.chunks(25) {
            let result = client
                .batch_write_item()
                .set_request_items(Some(HashMap::from([(
                    Self::table_name(),
                    write_chunk.to_vec(),
                )])))
                .send()
                .await;
            match result {
                Ok(_) => {}
                Err(err) => {
                    println!(
                        "batch_put_item error: {}",
                        err.into_service_error().message().unwrap_or_default()
                    );
                }
            }
        }
    }

    fn add_main_key_attributes(entity: &T, mut item: &mut HashMap<String, AttributeValue>) {
        let partition_key = Self::partition_key_from_entity(&entity);
        let sort_key = Self::sort_key_from_entity(&entity);
        item.insert(partition_key.0, partition_key.1);
        item.insert(sort_key.0, sort_key.1);
        Self::add_index_key_attributes(&entity, item);
    }

    fn add_index_key_attributes(entity: &T, mut item: &mut HashMap<String, AttributeValue>) {}
}

struct TestCaseTable();
struct ActionsTable();
struct ParametersTable();
struct AuthenticationProviderTable();

struct RunTable();

struct ActionExecutionTable();

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

impl Table<Parameter> for ParametersTable {
    fn table_name() -> String {
        "parameters".to_string()
    }

    fn partition_key_name() -> String {
        "customer_id#test_case_id".to_string()
    }

    fn sort_key_name() -> String {
        "action_id#id".to_string()
    }

    fn partition_key_from_entity(entity: &Parameter) -> (String, AttributeValue) {
        Self::partition_key(build_composite_key(vec![
            entity.customer_id.clone(),
            entity.test_case_id.clone(),
        ]))
    }

    fn sort_key_from_entity(entity: &Parameter) -> (String, AttributeValue) {
        Self::sort_key(build_composite_key(vec![
            entity.id.clone(),
            entity.action_id.clone(),
        ]))
    }

    fn add_index_key_attributes(entity: &Parameter, item: &mut HashMap<String, AttributeValue>) {
        let parameter_type = parameter_type_to_str(&entity.parameter_type);
        let (location, path) = extract_location_tuple(&entity);

        //location_index
        item.insert(
            "action_id#parameter_type#location".to_string(),
            AttributeValue::S(build_composite_key(vec![
                entity.action_id.clone(),
                parameter_type.to_string(),
                location.to_string(),
            ])),
        );

        //path_index
        item.insert(
            "action_id#parameter_type#path".to_string(),
            AttributeValue::S(build_composite_key(vec![
                entity.action_id.clone(),
                parameter_type.to_string(),
                path.to_string(),
            ])),
        );
    }
}

impl Table<AuthenticationProvider> for AuthenticationProviderTable {
    fn table_name() -> String {
        "authentication_providers".to_string()
    }

    fn partition_key_name() -> String {
        "customer_id".to_string()
    }

    fn sort_key_name() -> String {
        "id".to_string()
    }

    fn partition_key_from_entity(entity: &AuthenticationProvider) -> (String, AttributeValue) {
        Self::partition_key(entity.customer_id.clone())
    }

    fn sort_key_from_entity(entity: &AuthenticationProvider) -> (String, AttributeValue) {
        Self::sort_key(entity.id.clone())
    }

    fn add_index_key_attributes(
        entity: &AuthenticationProvider,
        item: &mut HashMap<String, AttributeValue>,
    ) {
        item.insert(
            "base_url".to_string(),
            AttributeValue::S(entity.base_url.clone()),
        );
    }
}

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

#[derive(Clone)]
pub struct Repository {
    client: Client,
}

type TestCasePort = TestCaseTable;
impl Repository {
    pub async fn new() -> Self {
        let config = aws_config::load_defaults(BehaviorVersion::latest()).await;
        let client = Client::new(&config);
        Repository { client }
    }

    pub async fn create_test_case(&self, test_case: TestCase) -> TestCase {
        TestCaseTable::put_item(&self.client, test_case)
            .await
            .unwrap()
    }

    pub async fn list_test_cases(
        &self,
        customer_id: String,
        next_page_key: Option<String>,
    ) -> QueryResult<TestCase> {
        TestCaseTable::list_items(&self.client, customer_id, next_page_key).await
    }

    pub async fn get_test_case(
        &self,
        customer_id: String,
        test_case_id: String,
    ) -> Option<TestCase> {
        TestCaseTable::get_item(&self.client, customer_id, test_case_id).await
    }

    pub async fn list_actions(
        &self,
        customer_id: String,
        test_case_id: String,
        next_page_key: Option<String>,
    ) -> QueryResult<Action> {
        ActionsTable::list_items(
            &self.client,
            build_composite_key(vec![customer_id, test_case_id]),
            next_page_key,
        )
        .await
    }

    pub async fn list_previous_actions(
        &self,
        customer_id: String,
        test_case_id: String,
        before_order: usize,
        next_page_key: Option<String>,
    ) -> QueryResult<Action> {
        let partition_key =
            ActionsTable::partition_key(build_composite_key(vec![customer_id, test_case_id]));
        let result = ActionsTable::query_builder(&self.client)
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

    pub async fn get_action_by_name(
        &self,
        customer_id: String,
        test_case_id: String,
        name: String,
    ) -> Option<Action> {
        let partition_key =
            ActionsTable::partition_key(build_composite_key(vec![customer_id, test_case_id]));
        let result = ActionsTable::query_builder(&self.client)
            .index_name("name_index".to_string())
            .expression_attribute_names("#pk", partition_key.0)
            .expression_attribute_names("#sk", "name")
            .expression_attribute_values(":pk", partition_key.1)
            .expression_attribute_values(":sk", AttributeValue::S(name.to_string()))
            .key_condition_expression("#pk = :pk AND #sk < :sk")
            .send()
            .await;

        ActionsTable::from_query_result(result).items.pop()
    }

    pub async fn list_parameters_of_action(
        &self,
        customer_id: String,
        test_case_id: String,
        action_id: String,
        parameter_type: ParameterType,
        parameter_in: Option<ParameterIn>,
        next_page_key: Option<String>,
    ) -> QueryResult<Parameter> {
        let partition_key =
            ParametersTable::partition_key(build_composite_key(vec![customer_id, test_case_id]));
        let param_in = parameter_in.map_or(String::new(), |parameter_in: ParameterIn| {
            parameter_in_to_str(&parameter_in)
        });
        let sort_key_value = format!(
            "{}#{}#{}",
            action_id,
            parameter_type_to_str(&parameter_type),
            param_in
        );
        let result = ParametersTable::query_builder(&self.client)
            .index_name("location_index")
            .expression_attribute_names("#pk", partition_key.0)
            .expression_attribute_names("#sk", "action_id#parameter_type#location")
            .expression_attribute_values(":pk", partition_key.1)
            .expression_attribute_values(":sk", AttributeValue::S(sort_key_value))
            .key_condition_expression("#pk = :pk AND begins_with(#sk, :sk)")
            .set_exclusive_start_key(
                next_page_key.map(|next| PageKey::from_next_page_key(&next).to_attribute_values()),
            )
            .send()
            .await;

        ParametersTable::from_query_result(result)
    }

    pub async fn query_parameters_of_action_by_path(
        &self,
        customer_id: String,
        test_case_id: String,
        action_id: String,
        parameter_type: ParameterType,
        path: String,
        next_page_key: Option<String>,
    ) -> QueryResult<Parameter> {
        let partition_key =
            ParametersTable::partition_key(build_composite_key(vec![customer_id, test_case_id]));
        let sort_key_value = format!(
            "{}#{}#{}",
            action_id,
            parameter_type_to_str(&parameter_type),
            path
        );
        println!("path query sort key: {}", sort_key_value);
        let result = ParametersTable::query_builder(&self.client)
            .index_name("path_index")
            .expression_attribute_names("#pk", partition_key.0)
            .expression_attribute_names("#sk", "action_id#parameter_type#path")
            .expression_attribute_values(":pk", partition_key.1)
            .expression_attribute_values(":sk", AttributeValue::S(sort_key_value))
            .key_condition_expression("#pk = :pk AND begins_with(#sk, :sk)")
            .set_exclusive_start_key(
                next_page_key.map(|next| PageKey::from_next_page_key(&next).to_attribute_values()),
            )
            .send()
            .await;

        ParametersTable::from_query_result(result)
    }

    pub async fn batch_create_actions(&self, actions: Vec<Action>) {
        ActionsTable::batch_put_item(&self.client, actions).await
    }

    pub async fn batch_create_parameters(&self, parameters: Vec<Parameter>) {
        ParametersTable::batch_put_item(&self.client, parameters).await
    }

    pub async fn batch_create_auth_providers(
        &self,
        authentication_providers: Vec<AuthenticationProvider>,
    ) {
        AuthenticationProviderTable::batch_put_item(&self.client, authentication_providers).await
    }

    pub async fn list_auth_providers(
        &self,
        customer_id: &String,
        test_case_id: Option<String>,
        base_url: Option<String>,
    ) -> Vec<AuthenticationProvider> {
        let mut builder = AuthenticationProviderTable::query_builder(&self.client)
            .expression_attribute_names("#pk", AuthenticationProviderTable::partition_key_name())
            .expression_attribute_values(":pk", AttributeValue::S(customer_id.clone()))
            .key_condition_expression("#pk = :pk");
        if let Some(tc_id) = test_case_id {
            builder = builder
                .expression_attribute_values(":ltc", AttributeValue::S(tc_id))
                .expression_attribute_names("#ltc", "linked_test_case_ids")
                .filter_expression("contains(#ltc, :ltc)");
        }
        if let Some(url) = base_url {
            builder = builder
                .expression_attribute_names("#sk", "base_url")
                .index_name("base_url_index")
                .key_condition_expression("#pk = :pk AND #sk = :sk")
                .expression_attribute_values(":sk", AttributeValue::S(url))
        }
        let result = builder.send().await;
        AuthenticationProviderTable::from_query_result(result).items
    }

    pub async fn create_run(&self, run: Run) -> Run {
        RunTable::put_item(&self.client, run).await.unwrap()
    }

    pub async fn get_run(
        &self,
        customer_id: &String,
        test_case_id: &String,
        id: &String,
    ) -> Option<Run> {
        RunTable::get_item(
            &self.client,
            build_composite_key(vec![customer_id.clone(), test_case_id.clone()]),
            id.clone(),
        )
        .await
    }

    pub async fn get_action_executions(
        &self,
        customer_id: &String,
        test_case_id: &String,
        run_id: &String,
    ) -> Vec<ActionExecution> {
        ActionExecutionTable::list_items(
            &self.client,
            build_composite_key(vec![
                customer_id.clone(),
                test_case_id.clone(),
                run_id.clone(),
            ]),
            None,
        )
        .await
        .items
    }

    pub async fn update_run_status(
        &self,
        customer_id: &String,
        test_case_id: &String,
        id: &String,
        status: &RunStatus,
    ) {
        RunTable::update_builder(&self.client)
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

    pub async fn create_action_execution(
        &self,
        action_execution: ActionExecution,
    ) -> ActionExecution {
        ActionExecutionTable::put_item(&self.client, action_execution)
            .await
            .unwrap()
    }

    pub async fn list_runs(&self, customer_id: &String, test_case_id: &String) -> QueryResult<Run> {
        let result = RunTable::query_builder(&self.client)
            .scan_index_forward(false)
            .expression_attribute_names("#pk", RunTable::partition_key_name())
            .expression_attribute_values(":pk", AttributeValue::S(build_composite_key(vec![customer_id.clone(), test_case_id.clone()])))
            .key_condition_expression("#pk = :pk")
            .send().await;
        RunTable::from_query_result(result)
    }
}

fn build_composite_key(keys: Vec<String>) -> String {
    keys.join("#")
}

fn parameter_type_to_str(parameter_type: &ParameterType) -> &str {
    let parameter_type = match parameter_type {
        ParameterType::Input => "input",
        ParameterType::Output => "output",
    };
    parameter_type
}

fn parameter_in_to_str(parameter_in: &ParameterIn) -> String {
    let parameter_type = match parameter_in {
        ParameterIn::Header => "header".to_string(),
        ParameterIn::Cookie => "cookie".to_string(),
        ParameterIn::Body => "body".to_string(),
        ParameterIn::Query => "query".to_string(),
    };
    parameter_type
}

fn extract_location_tuple(entity: &Parameter) -> (String, String) {
    let (location, path) = match &entity.location {
        ParameterLocation::Header(name) => ("header".to_string(), name),
        ParameterLocation::Cookie(name) => ("cookie".to_string(), name),
        ParameterLocation::Query(name) => ("query".to_string(), name),
        ParameterLocation::Body(name) => ("body".to_string(), name),
        ParameterLocation::StatusCode() => ("status_code".to_string(), &String::new()),
    };
    (location.clone(), path.clone())
}
