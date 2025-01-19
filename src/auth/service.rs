use crate::api::AppError;
use crate::auth::model::{AuthHeaderValue, AuthenticationProvider, ListAuthProvidersRequest};
use crate::persistence::model::QueryResult;
use crate::persistence::repo::Table;
use aws_sdk_dynamodb::types::AttributeValue;
use aws_sdk_dynamodb::Client;
use serde_dynamo::to_attribute_value;
use std::collections::{HashMap, HashSet};
use std::hash::Hash;
use std::sync::Arc;
use tokio::task::JoinHandle;

pub struct AuthProviderOperations {
    pub(crate) client: Arc<Client>,
}
pub struct AuthenticationProviderTable();

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
        if entity.linked_test_case_ids.len() > 0 {
            let set: Vec<String> = entity.linked_test_case_ids.clone().into_iter().collect();
            item.insert(
                "linked_test_case_ids".to_string(),
                AttributeValue::Ss(set),
            );
        }
    }
}

pub struct SetHeaderRequest {
    pub customer_id: String,
    pub id: String,
    pub name: String,
    pub value: String,
}

impl AuthProviderOperations {
    pub async fn batch_create(&self, authentication_providers: Vec<AuthenticationProvider>) {
        AuthenticationProviderTable::batch_put_item(self.client.clone(), authentication_providers)
            .await
    }

    pub async fn set_header(
        &self,
        request: SetHeaderRequest,
    ) -> Result<AuthenticationProvider, AppError> {
        AuthenticationProviderTable::update_partial(request.customer_id, request.id,
                                                    self.client.clone()
                                                        .update_item()
                                                        .update_expression("SET headers_by_name.#key.#value = :newValue")
                                                        .expression_attribute_names("#key", request.name)
                                                        .expression_attribute_names("#value", "value")
                                                        .expression_attribute_values(":newValue", AttributeValue::S(request.value))).await
    }

    pub async fn add_header(
        &self,
        request: SetHeaderRequest,
    ) -> Result<AuthenticationProvider, AppError> {
        AuthenticationProviderTable::update_partial(request.customer_id, request.id,
                                                    self.client.clone()
                                                        .update_item()
                                                        .update_expression("SET headers_by_name.#key = :newValue")
                                                        .expression_attribute_names("#key", request.name)
                                                        .expression_attribute_values(":newValue", to_attribute_value(AuthHeaderValue::builder()
                                                            .value(request.value)
                                                            .build()).unwrap())).await
    }

    pub async fn set_header_enablement(
        &self,
        customer_id: String,
        id: String,
        name: String,
        disabled: bool,
    ) -> Result<AuthenticationProvider, AppError> {
        AuthenticationProviderTable::update_partial(customer_id, id,
                                                    self.client.clone()
                                                        .update_item()
                                                        .update_expression("SET headers_by_name.#key.#disabled = :newValue")
                                                        .expression_attribute_names("#key", name)
                                                        .expression_attribute_names("#disabled", "disabled")
                                                        .expression_attribute_values(":newValue", AttributeValue::Bool(disabled))).await
    }

    pub async fn unlink_test_case(&self, customer_id: &String, test_case_id: &String) {
        let list_result = self
            .list(ListAuthProvidersRequest::builder()
                .customer_id(customer_id.clone())
                .test_case_id(test_case_id.clone())
                .build())
            .await;

        if let Ok(query_result) = list_result {
            for item in query_result.items {
                self.unlink(&item.customer_id, test_case_id, &item.id).await;
            }
        }
    }

    pub async fn list(
        &self,
        request: ListAuthProvidersRequest
    ) -> Result<QueryResult<AuthenticationProvider>, AppError> {
        let mut builder = AuthenticationProviderTable::query_builder(self.client.clone())
            .expression_attribute_names("#pk", AuthenticationProviderTable::partition_key_name())
            .expression_attribute_values(":pk", AttributeValue::S(request.customer_id))
            .key_condition_expression("#pk = :pk")
            .set_exclusive_start_key(AuthenticationProviderTable::build_exclusion_key(request.next_page_key));
        let mut filter_expr: String = String::new();
        if let Some(text)= request.keyword {
            filter_expr.push_str("contains(#keyword, :keyword)");
            builder = builder.expression_attribute_values(":keyword", AttributeValue::S(text.to_lowercase()))
                .expression_attribute_names("#keyword", "name");
        }
        if let Some(tc_id) = request.test_case_id {
            builder = builder
                .expression_attribute_values(":ltc", AttributeValue::S(tc_id))
                .expression_attribute_names("#ltc", "linked_test_case_ids");
            filter_expr.push_str("contains(#ltc, :ltc)");
        }
        if let Some(url) = request.base_url {
            builder = builder
                .expression_attribute_names("#sk", "base_url")
                .index_name("base_url_index")
                .key_condition_expression("#pk = :pk AND #sk = :sk")
                .expression_attribute_values(":sk", AttributeValue::S(url))
        }
        if filter_expr.len() > 0  {
            builder = builder.filter_expression(filter_expr.as_str());
        }
        let result = builder.send().await;
        AuthenticationProviderTable::from_query_result(result)
    }

    pub async fn batch_get(
        &self,
        customer_id: &String,
        ids: Vec<String>,
    ) -> Result<Vec<AuthenticationProvider>, AppError> {
        let key_pairs = ids.iter()
            .map(|id| { (customer_id.clone(), id.clone()) })
            .collect();
        AuthenticationProviderTable::batch_get_items(self.client.clone(), key_pairs).await
    }

    async fn unlink(&self, customer_id: &String, test_case_id: &String, auth_provider_id: &String) -> Result<AuthenticationProvider, AppError> {
        let client = self.client.clone();
        let customer_id_cloned = customer_id.clone();
        let test_case_id_cloned = test_case_id.clone();
        let auth_id_cloned = auth_provider_id.clone();
        AuthenticationProviderTable::update_partial(customer_id_cloned, auth_id_cloned, client.update_item()
            .update_expression("delete linked_test_case_ids :idToDelete")
            .expression_attribute_values(":idToDelete", AttributeValue::Ss(vec![test_case_id_cloned])))
            .await
    }

    pub async fn link(&self, customer_id: &String, id: &String, test_case_id: &String) -> Result<AuthenticationProvider, AppError> {
        AuthenticationProviderTable::update_partial(customer_id.clone(), id.clone(), self.client.clone()
            .update_item()
            .update_expression("ADD #mySet :newValue")
            .expression_attribute_names("#mySet", "linked_test_case_ids")
            .expression_attribute_values(":newValue", AttributeValue::Ss(vec![test_case_id.clone()]))).await
    }

    pub async fn delete(
        &self,
        customer_id: &String,
        id: String,
    ) -> Result<Option<AuthenticationProvider>, AppError> {
        AuthenticationProviderTable::delete_item(self.client.clone(), customer_id.clone(), id.clone())
            .await
    }

    pub async fn get(
        &self,
        customer_id: &String,
        id: String,
    ) -> Result<Option<AuthenticationProvider>, AppError> {
        AuthenticationProviderTable::get_item(self.client.clone(), customer_id.clone(), id.clone())
            .await
    }

    pub async fn create(
        &self,
        auth_provider: AuthenticationProvider,
    ) -> Result<AuthenticationProvider, AppError> {
        AuthenticationProviderTable::put_item(self.client.clone(), auth_provider).await
    }

    pub async fn list_by_multi_base_url(&self, customer_id: &String, base_urls: Vec<String>) -> Result<Vec<AuthenticationProvider>, AppError> {
        let mut providers: Vec<AuthenticationProvider> = vec![];
        let mut tasks: Vec<JoinHandle<Result<QueryResult<AuthenticationProvider>, AppError>>> = vec![];
        let url_set: HashSet<String> = HashSet::from_iter(base_urls);
        for base_url in url_set {
            let customer_id_cloned = customer_id.clone();
            let cloned_url = base_url.clone();
            let client = self.client.clone();
            let handle = tokio::task::spawn(async move {
                list_by_url(client, customer_id_cloned, cloned_url).await
            });
            tasks.push(handle);
        }
        for task in tasks {
            let result = task.await.unwrap();
            if let Ok(mut query_result) = result {
                providers.append(&mut query_result.items)
            }
        }

        Ok(providers)
    }
}

async fn list_by_url(client: Arc<Client>, customer_id: String, url: String) -> Result<QueryResult<AuthenticationProvider>, AppError> {
    let result = AuthenticationProviderTable::query_builder(client)
        .expression_attribute_names("#pk", AuthenticationProviderTable::partition_key_name())
        .expression_attribute_values(":pk", AttributeValue::S(customer_id))
        .key_condition_expression("#pk = :pk")
        .expression_attribute_names("#sk", "base_url")
        .index_name("base_url_index")
        .key_condition_expression("#pk = :pk AND #sk = :sk")
        .expression_attribute_values(":sk", AttributeValue::S(url))
        .send().await;
    AuthenticationProviderTable::from_query_result(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::repo::{init_logger, Repository};
    use std::time::Duration;
    use tokio::time::sleep;


    #[tokio::test]
    async fn crud_auth_providers() {
        init_logger();
        let repository = Repository::new().await;
        repository.auth_providers()
            .batch_create(vec![AuthenticationProvider::builder()
                .customer_id("cust1".to_string())
                .id("auth1".to_string())
                .name("".to_string())
                .base_url("https://xyz.abc".to_string())
                .headers_by_name(HashMap::new())
                .linked_test_case_ids(HashSet::new())
                .build()]).await;
        sleep(Duration::from_millis(100)).await;
        repository.auth_providers()
            .add_header(SetHeaderRequest {
                customer_id: "cust1".to_string(),
                id: "auth1".to_string(),
                name: "newHeader".to_string(),
                value: "newVal".to_string(),
            }).await.unwrap();

        let link_result = repository.auth_providers()
            .link(&"cust1".to_string(), &"auth1".to_string(), &"tc1".to_string())
            .await;

        assert!(link_result.is_ok());
        assert!(link_result.unwrap().linked_test_case_ids.contains(&"tc1".to_string()));

        let unlink_result = repository.auth_providers()
            .unlink(&"cust1".to_string(), &"tc1".to_string(), &"auth1".to_string()).await;
        assert!(unlink_result.is_ok());
        assert!(unlink_result.unwrap().linked_test_case_ids.is_empty());

        repository.auth_providers()
            .delete(&"cust1".to_string(), "auth1".to_string()).await.unwrap();
    }
}
