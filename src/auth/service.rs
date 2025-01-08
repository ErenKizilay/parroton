use crate::api::AppError;
use crate::auth::model::AuthenticationProvider;
use crate::persistence::repo::{QueryResult, Table};
use aws_sdk_dynamodb::types::AttributeValue;
use aws_sdk_dynamodb::Client;
use std::collections::HashMap;
use std::sync::Arc;

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
    }
}

pub struct SetHeaderRequest {
    pub customer_id: String,
    pub id: String,
    pub name: String,
    pub value: String,
}

impl AuthProviderOperations {
    pub async fn batch_create_auth_providers(
        &self,
        authentication_providers: Vec<AuthenticationProvider>,
    ) {
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
            .list(customer_id, Some(test_case_id.clone()), None)
            .await;

        if let Ok(query_result) = list_result {
            for item in query_result.items {
                self.unlink(&item.customer_id, test_case_id, &item.id).await;
            }
        }
    }

    pub async fn list(
        &self,
        customer_id: &String,
        test_case_id: Option<String>,
        base_url: Option<String>,
    ) -> Result<QueryResult<AuthenticationProvider>, AppError> {
        let mut builder = AuthenticationProviderTable::query_builder(self.client.clone())
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

    async fn unlink(&self, customer_id: &String, test_case_id: &String, auth_provider_id: &String) {
        let client = self.client.clone();
        let customer_id_cloned = customer_id.clone();
        let test_case_id_cloned = test_case_id.clone();
        let auth_id_cloned = auth_provider_id.clone();
        tokio::task::spawn(async move {
            client
                .update_item()
                .table_name(AuthenticationProviderTable::table_name())
                .set_key(Some(AuthenticationProviderTable::unique_key(
                    customer_id_cloned,
                    auth_id_cloned,
                )))
                .update_expression("delete linked_test_case_ids :idToDelete")
                .expression_attribute_values("idToDelete", AttributeValue::S(test_case_id_cloned))
                .send()
                .await
        });
    }

    pub async fn link(&self, customer_id: &String, id: &String, test_case_id: &String) {
        let client = self.client.clone();
        let customer_id_cloned = customer_id.clone();
        let auth_id_cloned = id.clone();
        let test_case_id_cloned = test_case_id.clone();
        tokio::task::spawn(async move {
            client
                .update_item()
                .table_name(AuthenticationProviderTable::table_name())
                .set_key(Some(AuthenticationProviderTable::unique_key(
                    customer_id_cloned,
                    auth_id_cloned,
                )))
                .update_expression("ADD #mySet :newValue")
                .expression_attribute_names("#mySet", "linked_test_case_ids")
                .expression_attribute_values("newValue", AttributeValue::S(test_case_id_cloned))
                .send()
                .await
        });
    }

    pub async fn delete(
        &self,
        customer_id: &String,
        id: String,
    ) -> Result<Option<AuthenticationProvider>, AppError> {
        AuthenticationProviderTable::delete_item(self.client.clone(), customer_id.clone(), id.clone())
            .await
    }
}
