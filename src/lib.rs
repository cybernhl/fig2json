pub mod blobs;
pub mod error;
pub mod parser;
pub mod schema;
pub mod types;

pub use error::{FigError, Result};
pub use types::{FileType, ParsedFile};

pub fn convert(bytes: &[u8], base_dir: Option<&std::path::Path>) -> Result<serde_json::Value> {
    let bytes = if parser::is_zip_container(bytes) {
        parser::extract_from_zip(bytes)?
    } else {
        bytes.to_vec()
    };

    let file_type = parser::detect_file_type(&bytes)?;
    let parsed = parser::extract_chunks(&bytes)?;

    let schema_bytes = parser::decompress_chunk(parsed.schema_chunk().ok_or(FigError::NotEnoughChunks { expected: 1, actual: 0 })?)?;
    let data_bytes = parser::decompress_chunk(parsed.data_chunk().ok_or(FigError::NotEnoughChunks { expected: 2, actual: parsed.chunks.len() })?)?;

    let json = schema::decode_fig_to_json(&schema_bytes, &data_bytes)?;

    let node_changes = json.get("nodeChanges").and_then(|v| v.as_array()).ok_or_else(|| FigError::ZipError("No nodeChanges".to_string()))?.clone();
    let (components, styles, component_sets) = extract_components_and_styles(&node_changes);

    let mut document = schema::build_tree(node_changes)?;

    let blobs = json.get("blobs").and_then(|v| v.as_array()).ok_or_else(|| FigError::ZipError("No blobs".to_string()))?.clone();
    let processed_blobs = blobs::process_blobs(blobs)?;
    blobs::substitute_blobs(&mut document, processed_blobs.as_array().unwrap())?;

    if let Some(dir) = base_dir {
        schema::transform_image_hashes(&mut document, dir)?;
    } else {
        schema::transform_image_hashes(&mut document, std::path::Path::new("."))?;
    }

    // --- PHASE 1: Pre-Alignment Cleanup ---
    schema::remove_text_glyphs(&mut document)?;

    // Build initial output
    let mut output = serde_json::json!({
        "version": parsed.version,
        "fileType": match file_type {
            FileType::Figma => "figma",
            FileType::FigJam => "figjam",
        },
        "document": document,
        "components": components,
        "styles": styles,
        "componentSets": component_sets,
    });

    // --- PHASE 2: Alignment & Normalization ---
    // IMPORTANT: DO NOT run transform_colors_to_css or transform_matrix_to_css if we want 99% alignment
    schema::align_node_types(&mut output)?;
    schema::simplify_enums(&mut output)?;
    schema::rename_paints(&mut output)?;
    schema::align_fields(&mut output)?;
    schema::transform_guids_to_ids(&mut output)?;
    schema::rename_transform(&mut output)?;

    // --- PHASE 3: Final Cleanup (Minimal) ---
    schema::remove_guid_fields(&mut output)?;
    schema::remove_internal_only_nodes(&mut output)?;
    schema::remove_vector_data(&mut output)?;
    // We keep default opacity/visible/rotation for better alignment if official API has them
    // schema::remove_default_opacity(&mut output)?;
    // schema::remove_default_visible(&mut output)?;
    // schema::remove_default_rotation(&mut output)?;
    schema::remove_detached_symbol_id(&mut output)?;
    schema::remove_overridden_symbol_id(&mut output)?;
    schema::remove_root_blobs(&mut output)?;

    Ok(output)
}

pub fn convert_raw(bytes: &[u8]) -> Result<serde_json::Value> {
    let bytes = if parser::is_zip_container(bytes) { parser::extract_from_zip(bytes)? } else { bytes.to_vec() };
    let file_type = parser::detect_file_type(&bytes)?;
    let parsed = parser::extract_chunks(&bytes)?;
    let schema_bytes = parser::decompress_chunk(parsed.schema_chunk().ok_or(FigError::NotEnoughChunks { expected: 1, actual: 0 })?)?;
    let data_bytes = parser::decompress_chunk(parsed.data_chunk().ok_or(FigError::NotEnoughChunks { expected: 2, actual: parsed.chunks.len() })?)?;
    let json = schema::decode_fig_to_json(&schema_bytes, &data_bytes)?;
    let node_changes = json.get("nodeChanges").and_then(|v| v.as_array()).ok_or_else(|| FigError::ZipError("No nodeChanges".to_string()))?.clone();
    let (components, styles, component_sets) = extract_components_and_styles(&node_changes);
    let mut document = schema::build_tree(node_changes)?;
    let blobs = json.get("blobs").and_then(|v| v.as_array()).ok_or_else(|| FigError::ZipError("No blobs".to_string()))?.clone();
    let processed_blobs = blobs::process_blobs(blobs)?;
    blobs::substitute_blobs(&mut document, processed_blobs.as_array().unwrap())?;

    Ok(serde_json::json!({
        "version": parsed.version,
        "fileType": match file_type { FileType::Figma => "figma", FileType::FigJam => "figjam" },
        "document": document,
        "components": components,
        "styles": styles,
        "componentSets": component_sets,
        "blobs": processed_blobs,
    }))
}

fn extract_components_and_styles(node_changes: &[serde_json::Value]) -> (serde_json::Value, serde_json::Value, serde_json::Value) {
    let mut components = serde_json::Map::new();
    let mut styles = serde_json::Map::new();
    let mut component_sets = serde_json::Map::new();
    for node in node_changes {
        if let Some(obj) = node.as_object() {
            let guid = if let Some(guid_obj) = obj.get("guid") {
                let session = guid_obj.get("sessionID").and_then(|v| v.as_u64()).unwrap_or(0);
                let local = guid_obj.get("localID").and_then(|v| v.as_u64()).unwrap_or(0);
                format!("{}:{}", session, local)
            } else { continue };
            if let Some(node_type_obj) = obj.get("type") {
                let node_type = node_type_obj.get("value").and_then(|v| v.as_str()).unwrap_or("");
                let is_state_group = obj.get("isStateGroup").and_then(|v| v.as_bool()).unwrap_or(false);
                if node_type == "SYMBOL" { components.insert(guid.clone(), refine_metadata(node)); }
                else if node_type == "COMPONENT_SET" || (node_type == "FRAME" && is_state_group) { component_sets.insert(guid.clone(), refine_metadata(node)); }
            }
            if let Some(style_type_obj) = obj.get("styleType") {
                if style_type_obj.get("value").is_some() { styles.insert(guid, refine_metadata(node)); }
            }
        }
    }
    (serde_json::Value::Object(components), serde_json::Value::Object(styles), serde_json::Value::Object(component_sets))
}

fn refine_metadata(node: &serde_json::Value) -> serde_json::Value {
    let mut metadata = serde_json::Map::new();
    if let Some(obj) = node.as_object() {
        for field in ["name", "description", "key", "remote"] {
            if let Some(val) = obj.get(field) {
                if let Some(inner) = val.get("value") { metadata.insert(field.to_string(), inner.clone()); }
                else { metadata.insert(field.to_string(), val.clone()); }
            }
        }
        if let Some(style_type) = obj.get("styleType") {
            if let Some(val) = style_type.get("value") { metadata.insert("styleType".to_string(), val.clone()); }
            else { metadata.insert("styleType".to_string(), style_type.clone()); }
        }
        if !metadata.contains_key("description") { metadata.insert("description".to_string(), serde_json::json!("")); }
    }
    serde_json::Value::Object(metadata)
}
