use crate::error::{FigError, Result};
use serde_json::Value as JsonValue;
use std::collections::HashMap;

/// Build a tree structure from flat nodeChanges array
pub fn build_tree(node_changes: Vec<JsonValue>) -> Result<JsonValue> {
    // 1. Create map: GUID -> Node and map of parent -> children (position, GUID) tuples
    let mut nodes: HashMap<String, JsonValue> = HashMap::new();
    let mut parent_to_children: HashMap<String, Vec<(String, String)>> = HashMap::new();

    for node in &node_changes {
        let guid = format_guid(node)?;
        nodes.insert(guid, node.clone());
    }

    // 2. Build parent-child relationships
    for node in &node_changes {
        if let Some(parent_index) = node.get("parentIndex") {
            let parent_guid = format_parent_guid(parent_index)?;
            let child_guid = format_guid(node)?;
            let position = parent_index
                .get("position")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            parent_to_children
                .entry(parent_guid)
                .or_default()
                .push((position, child_guid));
        }
    }

    // 3. Sort children by position
    for children in parent_to_children.values_mut() {
        children.sort_by(|a, b| a.0.cmp(&b.0));
    }

    // 4. Build tree recursively from root, calculating absolute coordinates
    let identity = Matrix2D::identity();
    build_node_tree("0:0", &nodes, &parent_to_children, identity, None)
}

#[derive(Clone, Copy, Debug)]
struct Matrix2D {
    a: f64, c: f64, tx: f64,
    b: f64, d: f64, ty: f64,
}

impl Matrix2D {
    fn identity() -> Self {
        Self {
            a: 1.0, c: 0.0, tx: 0.0,
            b: 0.0, d: 1.0, ty: 0.0,
        }
    }

    fn multiply(&self, other: &Self) -> Self {
        Self {
            a: self.a * other.a + self.c * other.b,
            c: self.a * other.c + self.c * other.d,
            tx: self.a * other.tx + self.c * other.ty + self.tx,
            b: self.b * other.a + self.d * other.b,
            d: self.b * other.c + self.d * other.d,
            ty: self.b * other.tx + self.d * other.ty + self.ty,
        }
    }

    fn from_json(transform: &JsonValue) -> Self {
        if let Some(obj) = transform.as_object() {
            let a = obj.get("m00").and_then(|v| v.as_f64()).unwrap_or(1.0);
            let c = obj.get("m01").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let tx = obj.get("m02").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let b = obj.get("m10").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let d = obj.get("m11").and_then(|v| v.as_f64()).unwrap_or(1.0);
            let ty = obj.get("m12").and_then(|v| v.as_f64()).unwrap_or(0.0);
            Self { a, c, tx, b, d, ty }
        } else {
            Self::identity()
        }
    }
}

/// Recursively build a node with its children
fn build_node_tree(
    guid: &str,
    nodes: &HashMap<String, JsonValue>,
    parent_to_children: &HashMap<String, Vec<(String, String)>>,
    parent_matrix: Matrix2D,
    id_prefix: Option<String>,
) -> Result<JsonValue> {
    // Get the node
    let mut node = nodes
        .get(guid)
        .ok_or_else(|| FigError::ZipError(format!("Node {} not found", guid)))?
        .clone();

    // Calculate absolute bounding box
    let local_matrix = if let Some(t) = node.get("transform") {
        Matrix2D::from_json(t)
    } else {
        Matrix2D::identity()
    };

    // DOCUMENT node (0:0) should not accumulate matrix, but CANVAS (Pages) start from 0:0
    let current_matrix = if guid == "0:0" {
        Matrix2D::identity()
    } else {
        parent_matrix.multiply(&local_matrix)
    };

    if let Some(obj) = node.as_object_mut() {
        obj.remove("parentIndex");

        // Set ID with prefix
        let current_id = if let Some(ref prefix) = id_prefix {
            format!("{};{}", prefix, guid)
        } else {
            guid.to_string()
        };
        obj.insert("id".to_string(), serde_json::json!(current_id));

        // Add absoluteBoundingBox
        let size = obj.get("size").and_then(|v| v.as_object());
        let width = size.and_then(|s| s.get("x")).and_then(|v| v.as_f64()).unwrap_or(0.0);
        let height = size.and_then(|s| s.get("y")).and_then(|v| v.as_f64()).unwrap_or(0.0);

        obj.insert("absoluteBoundingBox".to_string(), serde_json::json!({
            "x": current_matrix.tx,
            "y": current_matrix.ty,
            "width": width,
            "height": height
        }));

        // Instance Expansion: If this is an INSTANCE, clone children from its SYMBOL
        let mut children = Vec::new();

        let node_type = obj.get("type").and_then(|v| {
            if let Some(s) = v.as_str() { Some(s) }
            else { v.get("value").and_then(|iv| iv.as_str()) }
        }).unwrap_or("");

        if node_type == "INSTANCE" {
            if let Some(symbol_id_obj) = obj.get("symbolData").and_then(|sd| sd.get("symbolID")) {
                let s_session = symbol_id_obj.get("sessionID").and_then(|v| v.as_u64()).unwrap_or(0);
                let s_local = symbol_id_obj.get("localID").and_then(|v| v.as_u64()).unwrap_or(0);
                let symbol_guid = format!("{}:{}", s_session, s_local);

                // Find children of this symbol
                if let Some(symbol_children_entries) = parent_to_children.get(&symbol_guid) {
                    // When expanding, children get a prefix starting with "I" and the current instance's ID
                    let next_prefix = if let Some(ref p) = id_prefix {
                        format!("{};{}", p, guid)
                    } else {
                        format!("I{}", guid)
                    };

                    for (_pos, child_guid) in symbol_children_entries {
                        let child_node = build_node_tree(child_guid, nodes, parent_to_children, current_matrix, Some(next_prefix.clone()))?;
                        children.push(child_node);
                    }
                }
            }
        }

        // Add regular children recursively
        if let Some(child_entries) = parent_to_children.get(guid) {
            for (_position, child_guid) in child_entries {
                let child_node = build_node_tree(child_guid, nodes, parent_to_children, current_matrix, id_prefix.clone())?;
                children.push(child_node);
            }
        }

        if !children.is_empty() {
            obj.insert("children".to_string(), JsonValue::Array(children));
        }
    }

    Ok(node)
}

/// Format a GUID from a node object
fn format_guid(node: &JsonValue) -> Result<String> {
    let guid_obj = node.get("guid").ok_or_else(|| FigError::ZipError("Node missing guid field".to_string()))?;

    let session = guid_obj.get("sessionID")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| FigError::ZipError("Invalid sessionID in guid".to_string()))?;

    let local = guid_obj.get("localID")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| FigError::ZipError("Invalid localID in guid".to_string()))?;

    Ok(format!("{}:{}", session, local))
}

/// Format a GUID from a parentIndex's guid field
fn format_parent_guid(parent_index: &JsonValue) -> Result<String> {
    let guid_obj = parent_index.get("guid")
        .ok_or_else(|| FigError::ZipError("parentIndex missing guid field".to_string()))?;

    let session = guid_obj.get("sessionID")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| FigError::ZipError("Invalid sessionID in parentIndex".to_string()))?;

    let local = guid_obj.get("localID")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| FigError::ZipError("Invalid localID in parentIndex".to_string()))?;

    Ok(format!("{}:{}", session, local))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_build_tree_simple() {
        let node_changes = vec![
            json!({
                "guid": {"sessionID": 0, "localID": 0},
                "name": "Document"
            }),
            json!({
                "guid": {"sessionID": 1, "localID": 1},
                "name": "Page",
                "parentIndex": {
                    "guid": {"sessionID": 0, "localID": 0},
                    "position": "!"
                }
            })
        ];

        let root = build_tree(node_changes).unwrap();
        assert_eq!(root["name"], "Document");
        assert_eq!(root["children"][0]["name"], "Page");
    }

    #[test]
    fn test_build_tree_nested() {
        let node_changes = vec![
            json!({
                "guid": {"sessionID": 0, "localID": 0},
                "name": "Document"
            }),
            json!({
                "guid": {"sessionID": 1, "localID": 1},
                "name": "Page",
                "parentIndex": {
                    "guid": {"sessionID": 0, "localID": 0},
                    "position": "!"
                }
            }),
            json!({
                "guid": {"sessionID": 1, "localID": 2},
                "name": "Frame",
                "parentIndex": {
                    "guid": {"sessionID": 1, "localID": 1},
                    "position": "!"
                }
            })
        ];

        let root = build_tree(node_changes).unwrap();
        assert_eq!(root["children"][0]["children"][0]["name"], "Frame");
    }

    #[test]
    fn test_build_tree_ordering() {
        let node_changes = vec![
            json!({
                "guid": {"sessionID": 0, "localID": 0},
                "name": "Document"
            }),
            json!({
                "guid": {"sessionID": 1, "localID": 2},
                "name": "Child B",
                "parentIndex": {
                    "guid": {"sessionID": 0, "localID": 0},
                    "position": "B"
                }
            }),
            json!({
                "guid": {"sessionID": 1, "localID": 1},
                "name": "Child A",
                "parentIndex": {
                    "guid": {"sessionID": 0, "localID": 0},
                    "position": "A"
                }
            })
        ];

        let root = build_tree(node_changes).unwrap();
        let children = root["children"].as_array().unwrap();
        assert_eq!(children[0]["name"], "Child A");
        assert_eq!(children[1]["name"], "Child B");
    }
}
