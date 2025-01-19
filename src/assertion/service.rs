use crate::api::AppError;
use crate::assertion::model::{Assertion, ComparisonType, ValueProvider};
use crate::json_path::model::Expression;
use crate::persistence::model::{ListItemsRequest, QueryResult};
use crate::persistence::repo::{build_composite_key, Table};
use aws_sdk_dynamodb::types::AttributeValue;
use aws_sdk_dynamodb::Client;
use bon::Builder;
use serde_dynamo::to_attribute_value;
use std::sync::Arc;

pub struct AssertionOperations {
    pub(crate) client: Arc<Client>,
}

pub(crate) struct AssertionsTable();

impl Table<Assertion> for AssertionsTable {
    fn table_name() -> String {
        "assertions".to_string()
    }

    fn partition_key_name() -> String {
        "customer_id#test_case_id".to_string()
    }

    fn sort_key_name() -> String {
        "id".to_string()
    }

    fn partition_key_from_entity(entity: &Assertion) -> (String, AttributeValue) {
        Self::partition_key(build_composite_key(vec![entity.customer_id.clone(), entity.test_case_id.clone()]))
    }

    fn sort_key_from_entity(entity: &Assertion) -> (String, AttributeValue) {
        Self::sort_key(entity.id.clone())
    }
}

impl AssertionOperations {
    pub async fn list(&self, customer_id: &String, test_case_id: &String) -> Result<QueryResult<Assertion>, AppError> {
        AssertionsTable::list_items(self.client.clone(), ListItemsRequest::builder()
            .partition_key(build_composite_key(vec![customer_id.clone(), test_case_id.clone()]))
            .build())
            .await
    }

    pub async fn batch_create(&self, assertions: Vec<Assertion>) {
        AssertionsTable::batch_put_item(self.client.clone(), assertions).await
    }

    pub async fn delete(&self, customer_id: String, test_case_id: String, id: String) -> Result<Option<Assertion>, AppError> {
        AssertionsTable::delete_item(self.client.clone(), build_composite_key(vec![customer_id.clone(),
                                                                                   test_case_id.clone()]), id)
            .await
    }
    pub async fn put(&self, assertion: Assertion) -> Result<Assertion, AppError> {
        AssertionsTable::put_item(self.client.clone(), assertion).await
    }

    pub async fn update_comparison_type(&self, customer_id: String, test_case_id: String, id: String, comparison_type: ComparisonType) -> Result<Assertion, AppError> {
        AssertionsTable::update_partial(build_composite_key(vec![customer_id.clone(), test_case_id.clone()]), id,
                                        self.client.clone().update_item()
                                            .expression_attribute_names("#comparison_type", "comparison_type")
                                            .expression_attribute_values(":value", to_attribute_value(comparison_type).unwrap())
                                            .update_expression("SET #comparison_type = :value")).await
    }

    pub async fn update_comparison_negation(&self, customer_id: String, test_case_id: String, id: String, negate: bool) -> Result<Assertion, AppError> {
        AssertionsTable::update_partial(build_composite_key(vec![customer_id.clone(), test_case_id.clone()]), id,
                                        self.client.clone().update_item()
                                            .expression_attribute_names("#negate", "negate")
                                            .expression_attribute_values(":value", to_attribute_value(negate).unwrap())
                                            .update_expression("SET #negate = :value")).await
    }

    pub async fn update_expression(&self, customer_id: String, test_case_id: String, id: String, left: bool, expression: Option<String>) -> Result<Assertion, AppError> {
        let left_or_right = if left { "left" } else { "right" };
        AssertionsTable::update_partial(build_composite_key(vec![customer_id.clone(), test_case_id.clone()]), id,
                                        self.client.clone().update_item()
                                            .update_expression(format!("SET {} = :newValue, #f = :func", format!("#{}.#value_provider.#expression.#value", left_or_right)))
                                            .expression_attribute_names(format!("#{}", left_or_right), left_or_right)
                                            .expression_attribute_names("#value_provider", "value_provider")
                                            .expression_attribute_names("#expression", "expression")
                                            .expression_attribute_names("#value", "value")
                                            .expression_attribute_names("#f", "function")
                                            .expression_attribute_values(":func", AttributeValue::Null(true))
                                            .expression_attribute_values(":newValue", to_attribute_value(expression).unwrap())).await
    }

    pub async fn get(&self, customer_id: String, test_case_id: String, id: String) -> Result<Option<Assertion>, AppError> {
        AssertionsTable::get_item(self.client.clone(), build_composite_key(vec![customer_id.clone(), test_case_id.clone()]), id)
            .await
    }

    pub async fn batch_get(&self, customer_id: String, test_case_id: String, ids: Vec<String>) -> Result<Vec<Assertion>, AppError> {
        let key_pairs: Vec<(String, String)> = ids.iter()
            .map(|id| {
                (build_composite_key(vec![customer_id.clone(), test_case_id.clone()]), id.clone())
            }).collect();
        AssertionsTable::batch_get_items(self.client.clone(), key_pairs)
            .await
    }

    pub async fn update_function_parameter(&self, request: UpdateFunctionParameterRequest) -> Result<Assertion, AppError> {
        let left_or_right = if request.left { "left" } else { "right" };
        let update_path = format!("#location.#f.#p[{}]", request.parameter_index);
        AssertionsTable::update_partial(build_composite_key(vec![request.customer_id, request.test_case_id]), request.assertion_id,
                                        self.client.clone().update_item()
                                            .update_expression(format!("SET {} = :newValue, #vp = :vp", update_path))
                                            .expression_attribute_names("#location", left_or_right)
                                            .expression_attribute_names("#f", "function")
                                            .expression_attribute_names("#p", "parameters")
                                            .expression_attribute_names("#vp", "value_provider")
                                            .expression_attribute_values(":vp", AttributeValue::Null(true))
                                            .expression_attribute_values(":newValue", to_attribute_value(request.value_provider).unwrap())).await
    }

    pub async fn delete_function_parameter(&self, request: DeleteFunctionParameterRequest) -> Result<Assertion, AppError> {
        let left_or_right = if request.left { "left" } else { "right" };
        let update_path = format!("#location.#f.#p[{}]", request.parameter_index);
        AssertionsTable::update_partial(build_composite_key(vec![request.customer_id, request.test_case_id]), request.assertion_id,
                                        self.client.clone().update_item()
                                            .update_expression(format!("REMOVE {}", update_path))
                                            .expression_attribute_names("#location", left_or_right)
                                            .expression_attribute_names("#f", "function")
                                            .expression_attribute_names("#p", "parameters")).await
    }

}

#[derive(Builder)]
pub struct UpdateFunctionParameterRequest {
    pub customer_id: String,
    pub test_case_id: String,
    pub assertion_id: String,
    pub value_provider: ValueProvider,
    pub parameter_index: u8,
    pub left: bool,
}

#[derive(Builder)]
pub struct DeleteFunctionParameterRequest {
    pub customer_id: String,
    pub test_case_id: String,
    pub assertion_id: String,
    pub parameter_index: u8,
    pub left: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assertion::model::{AssertionItem, Function, Operation};
    use crate::json_path::model::Expression;
    use crate::persistence::repo::{init_logger, Repository, INIT};
    use std::sync::Once;
    use std::time::Duration;
    use tokio::time::sleep;
    use tracing::{debug, info};

    #[tokio::test]
    async fn update_assertion_expression() {
        init_logger();
        let repository = Repository::new().await;
        repository.assertions()
            .batch_create(vec![Assertion::builder()
                .customer_id("cust1".to_string())
                .test_case_id("tc1".to_string())
                .id("a1".to_string())
                .left(AssertionItem::from_expression(Expression{ value: "$x.y.z".to_string() }))
                .right(AssertionItem::from_expression(Expression{ value: "$a.b.c".to_string() }))
                .comparison_type(ComparisonType::EqualTo)
                .negate(false)
                .build()]).await;

        sleep(Duration::from_millis(100)).await;

        let get_result = repository.assertions()
            .get("cust1".to_string(), "tc1".to_string(), "a1".to_string()).await;
        assert!(get_result.is_ok());
        info!("{:?}", get_result);

        let update_result = repository.assertions()
            .update_expression("cust1".to_string(), "tc1".to_string(), "a1".to_string(), true, Some(String::from("$m.n"))).await;

        assert!(update_result.is_ok());
        assert_eq!(update_result.unwrap().left.value_provider.unwrap().expression.unwrap().value, String::from("$m.n"));
    }

    #[tokio::test]
    async fn update_function_parameter() {
        init_logger();
        let repository = Repository::new().await;
        repository.assertions()
            .batch_create(vec![Assertion::builder()
                .customer_id("cust1".to_string())
                .test_case_id("tc1".to_string())
                .id("a2".to_string())
                .left(AssertionItem::from_function(Function{ operation: Operation::Sum, parameters: vec![] }))
                .right(AssertionItem::from_expression(Expression{ value: "$a.b.c".to_string() }))
                .comparison_type(ComparisonType::EqualTo)
                .build()]).await;

        sleep(Duration::from_millis(100)).await;

        let update_result = repository.assertions()
            .update_function_parameter(UpdateFunctionParameterRequest {
                customer_id: "cust1".to_string(),
                test_case_id: "tc1".to_string(),
                assertion_id: "a2".to_string(),
                value_provider: ValueProvider {
                    expression: Some(Expression { value: "$.x.y".to_string() }),
                    value: None,
                },
                parameter_index: 0,
                left: true,
            }).await;

        assert!(update_result.is_ok());
        assert_eq!(update_result.unwrap().left.function.unwrap().parameters.get(0).unwrap().clone(), ValueProvider {
            expression: Some(Expression { value: "$.x.y".to_string() }),
            value: None,
        });

    }

    #[tokio::test]
    async fn delete_function_parameter() {
        init_logger();
        let repository = Repository::new().await;
        repository.assertions()
            .batch_create(vec![Assertion::builder()
                .customer_id("cust1".to_string())
                .test_case_id("tc1".to_string())
                .id("a3".to_string())
                .left(AssertionItem::from_function(Function{ operation: Operation::Sum, parameters: vec![ValueProvider { expression: Some(Expression { value: "$.x.y".to_string() }), value: None }, ValueProvider { expression: Some(Expression { value: "$.1.2".to_string() }), value: None }] }))
                .right(AssertionItem::from_expression(Expression{ value: "$a.b.c".to_string() }))
                .comparison_type(ComparisonType::EqualTo)
                .build()]).await;

        sleep(Duration::from_millis(100)).await;

        let update_result = repository.assertions()
            .delete_function_parameter(DeleteFunctionParameterRequest {
                customer_id: "cust1".to_string(),
                test_case_id: "tc1".to_string(),
                assertion_id: "a3".to_string(),
                parameter_index: 0,
                left: true,
            }).await;

        assert!(update_result.is_ok());
        assert_eq!(update_result.unwrap().left.function.unwrap().parameters.get(0).unwrap().clone(), ValueProvider { expression: Some(Expression { value: "$.1.2".to_string() }), value: None });

    }
}