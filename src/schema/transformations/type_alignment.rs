use crate::error::Result;
use serde_json::Value as JsonValue;

/// Align node types with Figma REST API naming conventions
pub fn align_node_types(tree: &mut JsonValue) -> Result<()> {
    transform_recursive(tree)
}

fn transform_recursive(value: &mut JsonValue) -> Result<()> {
    match value {
        JsonValue::Object(map) => {
            // Check for isStateGroup first to avoid borrow conflict
            let is_state_group = map.get("isStateGroup").and_then(|v| v.as_bool()).unwrap_or(false);

            // Fix type names
            if let Some(t) = map.get_mut("type") {
                if let Some(type_str) = t.as_str() {
                    if type_str == "FRAME" && is_state_group {
                        *t = JsonValue::String("COMPONENT_SET".to_string());
                    } else {
                        let new_type = match type_str {
                            "SYMBOL" => "COMPONENT",
                            "ROUNDED_RECTANGLE" => "RECTANGLE",
                            _ => type_str,
                        };
                        if new_type != type_str {
                            *t = JsonValue::String(new_type.to_string());
                        }
                    }
                }
            }

            // Recurse into children
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
