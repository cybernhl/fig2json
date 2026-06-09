use crate::error::Result;
use serde_json::Value as JsonValue;
use serde_json::json;

/// Align specific fields with Figma REST API naming and structure
pub fn align_fields(tree: &mut JsonValue) -> Result<()> {
    transform_recursive(tree)
}

fn transform_recursive(value: &mut JsonValue) -> Result<()> {
    match value {
        JsonValue::Object(map) => {
            let is_node = map.contains_key("type");

            // 1. Auto Layout Mappings
            if let Some(mode) = map.remove("stackMode") {
                let mode_val = if let Some(s) = mode.as_str() { s.to_string() }
                              else { mode.get("value").and_then(|v| v.as_str()).unwrap_or("").to_string() };
                map.insert("layoutMode".to_string(), json!(mode_val));
            }
            if let Some(spacing) = map.remove("stackSpacing") {
                map.insert("itemSpacing".to_string(), spacing);
            }
            if let Some(h_pad) = map.remove("stackHorizontalPadding") {
                map.insert("paddingLeft".to_string(), h_pad.clone());
                map.insert("paddingRight".to_string(), h_pad);
            }
            if let Some(v_pad) = map.remove("stackVerticalPadding") {
                map.insert("paddingTop".to_string(), v_pad.clone());
                map.insert("paddingBottom".to_string(), v_pad);
            }

            // 2. Extract Text Characters and Style
            let mut extracted_chars = None;
            let mut extracted_style = None;
            if let Some(text_data) = map.get("textData") {
                extracted_chars = text_data.get("characters").cloned();
                if let Some(style) = text_data.get("style") {
                    let mut s_obj = style.as_object().cloned().unwrap_or_default();
                    if let Some(f) = s_obj.remove("family") { s_obj.insert("fontFamily".to_string(), f); }
                    if let Some(p) = s_obj.remove("postscript") { s_obj.insert("fontPostScriptName".to_string(), p); }
                    extracted_style = Some(JsonValue::Object(s_obj));
                }
            }
            if let Some(c) = extracted_chars { map.insert("characters".to_string(), c); }
            if let Some(s) = extracted_style { map.insert("style".to_string(), s); }

            // 3. Instance componentId & overrides
            let mut symbol_info = None;
            if let Some(symbol_data) = map.get("symbolData") {
                let mut cid = None;
                if let Some(sid) = symbol_data.get("symbolID") {
                    cid = if let Some(guid) = sid.get("guid") {
                        let s = guid.get("sessionID").and_then(|v| v.as_u64()).unwrap_or(0);
                        let l = guid.get("localID").and_then(|v| v.as_u64()).unwrap_or(0);
                        Some(format!("{}:{}", s, l))
                    } else { sid.as_str().map(|s| s.to_string()) };
                }
                let ovr = symbol_data.get("symbolOverrides").cloned();
                symbol_info = Some((cid, ovr));
            }
            if let Some((cid, ovr)) = symbol_info {
                if let Some(c) = cid { map.insert("componentId".to_string(), json!(c)); }
                if let Some(o) = ovr { map.insert("overrides".to_string(), o); }
            }

            // 4. Constraints Assembly
            let h_const = map.remove("horizontalConstraint");
            let v_const = map.remove("verticalConstraint");
            if h_const.is_some() || v_const.is_some() {
                let h_val = h_const.and_then(|v| if v.is_string() { Some(v) } else { v.get("value").cloned() }).unwrap_or(json!("MIN"));
                let v_val = v_const.and_then(|v| if v.is_string() { Some(v) } else { v.get("value").cloned() }).unwrap_or(json!("MIN"));
                map.insert("constraints".to_string(), json!({ "horizontal": h_val, "vertical": v_val }));
            }

            // 5. Rectangle Corner Radii
            let tl = map.get("rectangleTopLeftCornerRadius").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let tr = map.get("rectangleTopRightCornerRadius").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let br = map.get("rectangleBottomRightCornerRadius").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let bl = map.get("rectangleBottomLeftCornerRadius").and_then(|v| v.as_f64()).unwrap_or(0.0);
            if tl != 0.0 || tr != 0.0 || br != 0.0 || bl != 0.0 {
                map.insert("rectangleCornerRadii".to_string(), json!([tl, tr, br, bl]));
            }

            // 6. Clips Content
            if let Some(mask_disabled) = map.remove("frameMaskDisabled") {
                let disabled = if let Some(b) = mask_disabled.as_bool() { b }
                              else { mask_disabled.get("value").and_then(|v| v.as_bool()).unwrap_or(false) };
                map.insert("clipsContent".to_string(), json!(!disabled));
            }

            // 7. Bound Variables
            if let Some(var_map) = map.remove("variableConsumptionMap") {
                let mut bound_vars = serde_json::Map::new();
                let mut radii_vars = serde_json::Map::new();
                if let Some(entries) = var_map.get("entries").and_then(|v| v.as_array()) {
                    for entry in entries {
                        if let (Some(field), Some(data)) = (entry.get("variableField"), entry.get("variableData")) {
                            let field_name = if let Some(s) = field.as_str() { s.to_string() }
                                            else { field.get("value").and_then(|v| v.as_str()).unwrap_or("").to_string() };
                            if let Some(alias_obj) = convert_to_official_alias(data) {
                                match field_name.as_str() {
                                    "FILLPaints" | "FILLS" | "fills" => { bound_vars.insert("fills".to_string(), json!([alias_obj])); },
                                    "STROKEPaints" | "STROKES" | "strokes" => { bound_vars.insert("strokes".to_string(), json!([alias_obj])); },
                                    "CORNER_RADIUS" | "cornerRadius" => { bound_vars.insert("cornerRadius".to_string(), alias_obj); },
                                    "RECTANGLE_TOP_LEFT_CORNER_RADIUS" => { radii_vars.insert("RECTANGLE_TOP_LEFT_CORNER_RADIUS".to_string(), alias_obj); },
                                    "RECTANGLE_TOP_RIGHT_CORNER_RADIUS" => { radii_vars.insert("RECTANGLE_TOP_RIGHT_CORNER_RADIUS".to_string(), alias_obj); },
                                    "RECTANGLE_BOTTOM_LEFT_CORNER_RADIUS" => { radii_vars.insert("RECTANGLE_BOTTOM_LEFT_CORNER_RADIUS".to_string(), alias_obj); },
                                    "RECTANGLE_BOTTOM_RIGHT_CORNER_RADIUS" => { radii_vars.insert("RECTANGLE_BOTTOM_RIGHT_CORNER_RADIUS".to_string(), alias_obj); },
                                    _ => { bound_vars.insert(field_name.to_lowercase(), alias_obj); }
                                }
                            }
                        }
                    }
                }
                if !radii_vars.is_empty() { bound_vars.insert("rectangleCornerRadii".to_string(), JsonValue::Object(radii_vars)); }
                if !bound_vars.is_empty() { map.insert("boundVariables".to_string(), JsonValue::Object(bound_vars)); }
            }

            // 7a. Bound Variables (Paint level)
            if let Some(color_var) = map.remove("colorVar") {
                if let Some(official_var) = convert_to_official_alias(&color_var) {
                    let mut bound_vars = serde_json::Map::new();
                    bound_vars.insert("color".to_string(), official_var);
                    map.insert("boundVariables".to_string(), JsonValue::Object(bound_vars));
                }
            }

            // 8. backgroundColor (Synthesis from fills)
            if map.get("type").and_then(|v| v.as_str()) == Some("FRAME") {
                if let Some(fills) = map.get("fills").and_then(|v| v.as_array()) {
                    if let Some(first_fill) = fills.first() {
                        if first_fill.get("type").and_then(|v| v.as_str()) == Some("SOLID") {
                            if let Some(color) = first_fill.get("color") {
                                map.insert("backgroundColor".to_string(), color.clone());
                            }
                        }
                    }
                }
                if !map.contains_key("backgroundColor") {
                    map.insert("backgroundColor".to_string(), json!({"r": 0.0, "g": 0.0, "b": 0.0, "a": 0.0}));
                }
            }

            // 9. Styles Mapping
            let mut styles_obj = serde_json::Map::new();
            for (old, new) in [("styleIdForFill", "fill"), ("styleIdForText", "text"), ("styleIdForStrokeFill", "stroke"), ("styleIdForEffect", "effect")] {
                if let Some(sid) = map.remove(old) {
                    let id_str = if let Some(guid) = sid.get("guid") {
                        let s = guid.get("sessionID").and_then(|v| v.as_u64()).unwrap_or(0);
                        let l = guid.get("localID").and_then(|v| v.as_u64()).unwrap_or(0);
                        Some(format!("{}:{}", s, l))
                    } else { sid.as_str().map(|s| s.to_string()) };
                    if let Some(id) = id_str { styles_obj.insert(new.to_string(), json!(id)); }
                }
            }
            if !styles_obj.is_empty() { map.insert("styles".to_string(), JsonValue::Object(styles_obj)); }

            // 10. Interactions & Prototypes
            if let Some(proto) = map.remove("prototypeInteractions") { map.insert("interactions".to_string(), proto); }

            // 11. Layout Alignment / Grow
            if let Some(align) = map.remove("stackChildAlignSelf") { map.insert("layoutAlign".to_string(), align); }
            if let Some(grow) = map.remove("stackChildPrimaryGrow") { map.insert("layoutGrow".to_string(), grow); }

            // 12. Property arrays (Node level only)
            if is_node {
                for arr_field in ["fills", "strokes", "effects", "interactions"] {
                    if !map.contains_key(arr_field) { map.insert(arr_field.to_string(), json!([])); }
                }
            }

            // 13. Cleanup
            map.remove("parameterConsumptionMap");
            map.remove("derivedSymbolData");
            map.remove("sharedSymbolVersion");

            // Recurse
            let keys: Vec<String> = map.keys().cloned().collect();
            for key in keys {
                if let Some(val) = map.get_mut(&key) { transform_recursive(val)?; }
            }
        }
        JsonValue::Array(arr) => {
            for val in arr.iter_mut() { transform_recursive(val)?; }
        }
        _ => {}
    }
    Ok(())
}

fn convert_to_official_alias(internal_var: &JsonValue) -> Option<JsonValue> {
    if let Some(obj) = internal_var.as_object() {
        let is_alias = obj.get("dataType").and_then(|v| {
            if let Some(s) = v.as_str() { Some(s == "ALIAS") }
            else { v.get("value").and_then(|v2| v2.as_str()).map(|s| s == "ALIAS") }
        }).unwrap_or(false);

        if is_alias {
            let val_obj = obj.get("value")?;
            let alias_obj = val_obj.get("alias")?;
            let var_id = if let Some(id) = alias_obj.get("id").and_then(|v| v.as_str()) {
                if id.starts_with("VariableID:") { id.to_string() } else { format!("VariableID:{}", id) }
            } else if let Some(guid) = alias_obj.get("guid") {
                let s = guid.get("sessionID").and_then(|v| v.as_u64()).unwrap_or(0);
                let l = guid.get("localID").and_then(|v| v.as_u64()).unwrap_or(0);
                format!("VariableID:{}:{}", s, l)
            } else { return None; };
            return Some(json!({ "type": "VARIABLE_ALIAS", "id": var_id }));
        }
    }
    None
}
