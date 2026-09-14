//! Locale-neutral messages crossing the processing/product boundary.
//!
//! Mathematical code emits stable keys and structured arguments. The product
//! chooses a locale and renders the message. `fallback` preserves compatibility
//! while legacy domains are migrated away from prose fields.

use std::collections::BTreeMap;

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MessageRef {
    pub key: String,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub args: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fallback: Option<String>,
}

impl MessageRef {
    pub fn new(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            args: BTreeMap::new(),
            fallback: None,
        }
    }

    pub fn with_fallback(key: impl Into<String>, fallback: impl Into<String>) -> Self {
        Self {
            fallback: Some(fallback.into()),
            ..Self::new(key)
        }
    }

    pub fn arg(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.args.insert(name.into(), value.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_stable_key_arguments_and_fallback() {
        let message = MessageRef::with_fallback("steps.equation-establish", "建立方程。")
            .arg("variable", "x");
        let value = serde_json::to_value(message).unwrap();
        assert_eq!(value["key"], "steps.equation-establish");
        assert_eq!(value["args"]["variable"], "x");
        assert_eq!(value["fallback"], "建立方程。");
    }
}
