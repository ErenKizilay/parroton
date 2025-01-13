use crate::api::AppError;
use crate::persistence::repo::{build_composite_key, PageKey, QueryResult, Table};
use aws_sdk_dynamodb::types::AttributeValue;
use aws_sdk_dynamodb::Client;
use serde_dynamo::to_attribute_value;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::info;
use crate::json_path::model::Expression;
use crate::parameter::model::{Parameter, ParameterIn, ParameterLocation, ParameterType};

pub(crate) struct ParametersTable();

pub(crate) struct ParameterOperations {
    pub(crate) client: Arc<Client>,
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
            entity.action_id.clone(),
            entity.id.clone(),
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

impl ParameterOperations {
    pub async fn batch_create(&self, parameters: Vec<Parameter>) {
        ParametersTable::batch_put_item(self.client.clone(), parameters).await
    }

    pub async fn query_by_path(
        &self,
        customer_id: String,
        test_case_id: String,
        action_id: String,
        parameter_type: ParameterType,
        path: String,
        next_page_key: Option<String>,
    ) -> Result<QueryResult<Parameter>, AppError> {
        let partition_key =
            ParametersTable::partition_key(build_composite_key(vec![customer_id, test_case_id]));
        let sort_key_value = format!(
            "{}#{}#{}",
            action_id,
            parameter_type_to_str(&parameter_type),
            path
        );
        println!("path query sort key: {}", sort_key_value);
        let result = ParametersTable::query_builder(self.client.clone())
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

    pub async fn list_all_inputs_of_action(
        &self,
        customer_id: String,
        test_case_id: String,
        action_id: String,
    ) -> Result<Vec<Parameter>, AppError> {
        let mut parameters: Vec<Parameter> = vec![];
        let mut next_page_key: Option<String> = None;
        let mut app_error: Option<AppError> = None;
        loop {
            let list_result = self.list_by_action(customer_id.clone(), test_case_id.clone(), action_id.clone(), ParameterType::Input, None, next_page_key.clone())
                .await;
            match list_result {
                Ok(query_result) => {
                    parameters.extend(query_result.items);
                    next_page_key = query_result.next_page_key;
                }
                Err(err) => {
                    app_error = Some(err);
                }
            }
            if app_error.is_some() || next_page_key.is_none() {
                break;
            }
        }
        if app_error.is_some() {
            Err(app_error.unwrap())
        } else {
            Ok(parameters)
        }
    }

    pub async fn list_by_action(
        &self,
        customer_id: String,
        test_case_id: String,
        action_id: String,
        parameter_type: ParameterType,
        parameter_in: Option<ParameterIn>,
        next_page_key: Option<String>,
    ) -> Result<QueryResult<Parameter>, AppError> {
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
        let result = ParametersTable::query_builder(self.client.clone())
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

    pub async fn update_expression(&self, customer_id: String, test_case_id: String, action_id: String, id: String,
                                   expression: Option<Expression>) -> Result<Parameter, AppError> {
        info!("{:?}", expression);
        info!("cid: {}, tid: {}, aid: {}, id: {}", customer_id, test_case_id, action_id, id);
        let attribute_value = expression.map_or(AttributeValue::Null(true), |new_expr| to_attribute_value(new_expr).unwrap());
        info!("attribute_value: {:?}", attribute_value);
        ParametersTable::update_partial(build_composite_key(vec![customer_id, test_case_id]),
                                        build_composite_key(vec![action_id, id]),
                                        self.client.clone()
                                            .update_item()
                                            .update_expression("SET #expr = :expr")
                                            .expression_attribute_names("#expr", "value_expression")
                                            .expression_attribute_values(":expr", attribute_value)).await
    }
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
    };
    (location.clone(), path.clone())
}