use crate::error::Result;
use serde_json::Value as JsonValue;

/// Rename transform to relativeTransform for Figma REST API compatibility
pub fn rename_transform(tree: &mut JsonValue) -> Result<()> {
    transform_recursive(tree)
}

fn transform_recursive(value: &mut JsonValue) -> Result<()> {
    match value {
        JsonValue::Object(map) => {
            if let Some(transform) = map.remove("transform") {
                // If it has m00...m12, convert it to official 2x3 matrix [[m00, m01, m02], [m10, m11, m12]]
                if let Some(obj) = transform.as_object() {
                    if obj.contains_key("m00") {
                        let m00 = obj.get("m00").and_then(|v| v.as_f64()).unwrap_or(1.0);
                        let m01 = obj.get("m01").and_then(|v| v.as_f64()).unwrap_or(0.0);
                        let m02 = obj.get("m02").and_then(|v| v.as_f64()).unwrap_or(0.0);
                        let m10 = obj.get("m10").and_then(|v| v.as_f64()).unwrap_or(0.0);
                        let m11 = obj.get("m11").and_then(|v| v.as_f64()).unwrap_or(1.0);
                        let m12 = obj.get("m12").and_then(|v| v.as_f64()).unwrap_or(0.0);

                        let matrix = serde_json::json!([
                            [m00, m01, m02],
                            [m10, m11, m12]
                        ]);
                        map.insert("relativeTransform".to_string(), matrix);
                    } else {
                        // If it's already decomposed or something else, just rename it
                        map.insert("relativeTransform".to_string(), transform);
                    }
                } else {
                    map.insert("relativeTransform".to_string(), transform);
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
