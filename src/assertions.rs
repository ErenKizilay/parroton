use serde_json::Value;
use crate::models::{Expression, Function, Operation, ValueProvider};

trait ValueSupplier {

    fn supply(&self) -> Value;
}

impl ValueSupplier for ValueProvider {
    fn supply(&self) -> Value {
        match &self.value {
            None => {
                match &self.expression {
                    None => {
                        Value::Null
                    }
                    Some(exp) => {
                        Value::Null
                    }
                }
            }
            Some(val) => {
                val.clone()
            }
        }
    }
}

impl ValueSupplier for Function {
    fn supply(&self) -> Value {
        match &self.operation {
            Operation::Sum => {
                Value::from(i64::min_value())
            }
            Operation::Avg => {
                Value::from(i64::min_value())
            }
            Operation::Count => {
                Value::from(i64::min_value())
            }
        }
    }
}