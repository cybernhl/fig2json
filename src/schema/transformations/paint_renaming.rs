use crate::error::Result;
use serde_json::Value as JsonValue;

/// Rename fillPaints to fills and strokePaints to strokes for Figma REST API compatibility
pub fn rename_paints(tree: &mut JsonValue) -> Result<()> {
    transform_recursive(tree)
}

fn transform_recursive(value: &mut JsonValue) -> Result<()> {
    match value {
        JsonValue::Object(map) => {
            if let Some(fills) = map.remove("fillPaints") {
                map.insert("fills".to_string(), fills);
            }
            if let Some(strokes) = map.remove("strokePaints") {
                map.insert("strokes".to_string(), strokes);
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
