use crate::api::AppError;
use crate::models::AuthenticationProvider;
use crate::persistence::repo::{QueryResult, Table};
use aws_sdk_dynamodb::types::AttributeValue;
use aws_sdk_dynamodb::Client;
use std::collections::HashMap;
use std::sync::Arc;

pub struct AuthProviderOperations {
    pub(crate) client: Arc<Client>,
}
struct AuthenticationProviderTable();

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

impl AuthProviderOperations {
    pub async fn batch_create_auth_providers(
        &self,
        authentication_providers: Vec<AuthenticationProvider>,
    ) {
        AuthenticationProviderTable::batch_put_item(self.client.clone(), authentication_providers).await
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
}
