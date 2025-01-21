use serde::{Deserialize, Serialize};
use std::any::Any;

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BlackboardValue {
    String(String),
    Int(i32),
    Float(f32),
    Double(f64),
    Bool(bool),
}

impl BlackboardValue {
    pub fn from_any(value: &dyn Any) -> Option<Self> {
        if let Some(&v) = value.downcast_ref::<i32>() {
            Some(BlackboardValue::Int(v))
        } else if let Some(&v) = value.downcast_ref::<f32>() {
            Some(BlackboardValue::Float(v))
        } else if let Some(&v) = value.downcast_ref::<f64>() {
            Some(BlackboardValue::Double(v))
        } else if let Some(v) = value.downcast_ref::<String>() {
            Some(BlackboardValue::String(v.clone()))
        } else if let Some(&v) = value.downcast_ref::<bool>() {
            Some(BlackboardValue::Bool(v))
        } else {
            None // Unsupported type
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BlackboardEntry {
    pub key: String,
    pub value: BlackboardValue,
}