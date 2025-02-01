use aws_sdk_dynamodb::types::AttributeValue;
use bon::Builder;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::collections::HashMap;

pub struct PageKey {
    keys: HashMap<String, String>,
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

#[derive(Builder)]
pub struct ListItemsRequest {
    pub partition_key: String,
    pub next_page_key: Option<String>,
    pub expression_attribute_names: Option<HashMap<String, String>>,
    pub expression_attribute_values: Option<HashMap<String, AttributeValue>>,
    pub filter_expression: Option<String>,
    pub limit: Option<i32>,
}