use crate::error::Result;
use serde_json::Value as JsonValue;

/// Transform internal Figma variable structures to official Figma REST API format
pub fn transform_variables(tree: &mut JsonValue) -> Result<()> {
    transform_recursive(tree)
}

fn transform_recursive(value: &mut JsonValue) -> Result<()> {
    match value {
        JsonValue::Object(map) => {
            // 1. Handle colorVar -> boundVariables
            if let Some(color_var) = map.remove("colorVar") {
                if let Some(official_var) = convert_to_official_alias(&color_var) {
                    let mut bound_vars = serde_json::Map::new();
                    bound_vars.insert("color".to_string(), official_var);
                    map.insert("boundVariables".to_string(), JsonValue::Object(bound_vars));
                }
            }

            // 3. Recurse
            let keys: Vec<String> = map.keys().cloned().collect();
            for key in keys {
                if let Some(val) = map.get_mut(&key) {
                    transform_recursive(val)?;
                }
            }
        }
        JsonValue::Array(arr) => {
            for val in arr.iter_mut() {
                transform_recursive(val)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn convert_to_official_alias(internal_var: &JsonValue) -> Option<JsonValue> {
    if let Some(obj) = internal_var.as_object() {
        // Check if it's an ALIAS
        let is_alias = obj.get("dataType").and_then(|v| {
            if let Some(s) = v.as_str() { Some(s == "ALIAS") }
            else { v.get("value").and_then(|v2| v2.as_str()).map(|s| s == "ALIAS") }
        }).unwrap_or(false);

        if is_alias {
            let val_obj = obj.get("value")?;
            let alias_obj = val_obj.get("alias")?;

            let var_id = if let Some(id) = alias_obj.get("id").and_then(|v| v.as_str()) {
                if id.starts_with("VariableID:") { id.to_string() }
                else { format!("VariableID:{}", id) }
            } else if let Some(guid) = alias_obj.get("guid") {
                let s = guid.get("sessionID").and_then(|v| v.as_u64()).unwrap_or(0);
                let l = guid.get("localID").and_then(|v| v.as_u64()).unwrap_or(0);
                format!("VariableID:{}:{}", s, l)
            } else {
                return None;
            };

            return Some(serde_json::json!({
                "type": "VARIABLE_ALIAS",
                "id": var_id
            }));
        }
    }
    None
}
