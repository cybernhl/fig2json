use crate::error::Result;
use serde_json::Value as JsonValue;

/// Remove small redundant or internal properties
pub fn remove_redundant_properties(tree: &mut JsonValue) -> Result<()> {
    transform_recursive(tree)
}

fn transform_recursive(value: &mut JsonValue) -> Result<()> {
    match value {
        JsonValue::Object(map) => {
            // Remove isPageDivider
            map.remove("isPageDivider");

            // Remove dashPattern if [0.0, 0.0]
            if let Some(dash) = map.get("dashPattern") {
                if let Some(arr) = dash.as_array() {
                    if arr.len() == 2 && arr.iter().all(|v| v.as_f64() == Some(0.0)) {
                        map.remove("dashPattern");
                    }
                }
            }

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
