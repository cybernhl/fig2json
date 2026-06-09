use crate::error::Result;
use serde_json::Value as JsonValue;

/// Transform guid objects to id strings
pub fn transform_guids_to_ids(tree: &mut JsonValue) -> Result<()> {
    transform_recursive(tree)
}

fn transform_recursive(value: &mut JsonValue) -> Result<()> {
    match value {
        JsonValue::Object(map) => {
            // 1. First, handle "guid" field specifically (rename to "id")
            // ONLY if "id" field does not exist (preserve expanded IDs)
            if let Some(guid_val) = map.remove("guid") {
                if !map.contains_key("id") {
                    if let Some(id_str) = format_id_string(&guid_val) {
                        map.insert("id".to_string(), JsonValue::String(id_str));
                    } else {
                        map.insert("guid".to_string(), guid_val);
                    }
                }
            }

            // 2. Handle other fields (like "symbolID", "detachedSymbolID")
            // that might contain the {sessionID, localID} structure
            for (key, val) in map.iter_mut() {
                if key == "id" { continue; } // Skip existing ID
                if let Some(id_str) = format_id_string(val) {
                    *val = JsonValue::String(id_str);
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

fn format_id_string(value: &JsonValue) -> Option<String> {
    if let JsonValue::Object(obj) = value {
        let session = obj.get("sessionID")?.as_u64()?;
        let local = obj.get("localID")?.as_u64()?;
        Some(format!("{}:{}", session, local))
    } else {
        None
    }
}
