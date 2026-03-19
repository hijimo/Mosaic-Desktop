use crate::protocol::types::ResponseItem;
use serde_json::Value;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Compression {
    #[default]
    None,
    Zstd,
}

pub(crate) fn attach_item_ids(payload_json: &mut Value, original_items: &[ResponseItem]) {
    let input_value = match payload_json.get_mut("input") {
        Some(v) => v,
        None => return,
    };
    let items = match input_value {
        Value::Array(items) => items,
        _ => return,
    };

    for (value, item) in items.iter_mut().zip(original_items.iter()) {
        let id_opt = match item {
            ResponseItem::Reasoning { id, .. } => Some(id.clone()),
            ResponseItem::Message { id: Some(id), .. } => Some(id.clone()),
            ResponseItem::FunctionCall { id: Some(id), .. } => Some(id.clone()),
            _ => None,
        };

        if let Some(id) = id_opt {
            if !id.is_empty() {
                if let Some(obj) = value.as_object_mut() {
                    obj.insert("id".to_string(), Value::String(id));
                }
            }
        }
    }
}
