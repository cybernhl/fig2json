use crate::error::Result;
use serde_json::Value as JsonValue;

/// Remove vectorData fields from all objects in the JSON tree
pub fn remove_vector_data(tree: &mut JsonValue) -> Result<()> {
    transform_recursive(tree)
}

fn transform_recursive(value: &mut JsonValue) -> Result<()> {
    match value {
        JsonValue::Object(map) => {
            map.remove("vectorData");

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
