use serde::Deserialize;
use serde::Serialize;
use serde::ser::{SerializeStruct, Serializer};
use serde_json::Value as JsonValue;
use serde_json::json;
use std::collections::BTreeMap;

/// Whether additional properties are allowed, and if so, any required schema
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub(crate) enum AdditionalProperties {
    Boolean(bool),
    Schema(Box<JsonSchema>),
}

impl From<bool> for AdditionalProperties {
    fn from(b: bool) -> Self {
        Self::Boolean(b)
    }
}

impl From<JsonSchema> for AdditionalProperties {
    fn from(s: JsonSchema) -> Self {
        Self::Schema(Box::new(s))
    }
}

/// Generic JSON‑Schema subset needed for our tool definitions
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "lowercase")]
pub(crate) enum JsonSchema {
    Boolean {
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
    },
    String {
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        #[serde(rename = "enum", skip_serializing_if = "Option::is_none")]
        allowed_values: Option<Vec<String>>,
    },
    /// MCP schema allows "number" | "integer" for Number
    #[serde(alias = "integer")]
    Number {
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
    },
    Array {
        items: Box<JsonSchema>,
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
    },
    Object {
        properties: BTreeMap<String, JsonSchema>,
        #[serde(skip_serializing_if = "Option::is_none")]
        required: Option<Vec<String>>,
        #[serde(
            rename = "additionalProperties",
            skip_serializing_if = "Option::is_none"
        )]
        additional_properties: Option<AdditionalProperties>,
    },
}

impl Serialize for JsonSchema {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            JsonSchema::Boolean { description } => {
                let mut state = serializer.serialize_struct("JsonSchema", if description.is_some() { 2 } else { 1 })?;
                state.serialize_field("type", "boolean")?;
                if let Some(desc) = description {
                    state.serialize_field("description", desc)?;
                }
                state.end()
            }
            JsonSchema::String {
                description,
                allowed_values,
            } => {
                let mut fields = 1;
                if description.is_some() {
                    fields += 1;
                }
                if allowed_values.is_some() {
                    fields += 1;
                }
                let mut state = serializer.serialize_struct("JsonSchema", fields)?;
                state.serialize_field("type", "string")?;
                if let Some(desc) = description {
                    state.serialize_field("description", desc)?;
                }
                if let Some(values) = allowed_values {
                    state.serialize_field("enum", values)?;
                }
                state.end()
            }
            JsonSchema::Number { description } => {
                let mut state = serializer.serialize_struct("JsonSchema", if description.is_some() { 2 } else { 1 })?;
                state.serialize_field("type", "number")?;
                if let Some(desc) = description {
                    state.serialize_field("description", desc)?;
                }
                state.end()
            }
            JsonSchema::Array { items, description } => {
                let mut fields = 2; // type + items
                if description.is_some() {
                    fields += 1;
                }
                let mut state = serializer.serialize_struct("JsonSchema", fields)?;
                state.serialize_field("type", "array")?;
                state.serialize_field("items", items)?;
                if let Some(desc) = description {
                    state.serialize_field("description", desc)?;
                }
                state.end()
            }
            JsonSchema::Object {
                properties,
                required,
                additional_properties,
            } => {
                let req: Vec<String> = match required {
                    Some(explicit) => explicit.clone(),
                    None => properties.keys().cloned().collect(),
                };
                let mut fields = 3; // type, properties, required
                if additional_properties.is_some() {
                    fields += 1;
                }
                let mut state = serializer.serialize_struct("JsonSchema", fields)?;
                state.serialize_field("type", "object")?;
                state.serialize_field("properties", properties)?;
                state.serialize_field("required", &req)?;
                if let Some(additional) = additional_properties {
                    state.serialize_field("additionalProperties", additional)?;
                }
                state.end()
            }
        }
    }
}

pub(super) fn parse_tool_input_schema(input_schema: &JsonValue) -> Result<JsonSchema, serde_json::Error> {
    let mut input_schema = input_schema.clone();
    sanitize_json_schema(&mut input_schema);
    serde_json::from_value::<JsonSchema>(input_schema)
}

/// Sanitize a JSON Schema (as serde_json::Value) so it can fit our limited
/// JsonSchema enum. This function:
/// - Ensures every schema object has a "type". If missing, infers it from
///   common keywords (properties => object, items => array, enum/const/format => string)
///   and otherwise defaults to "string".
/// - Fills required child fields (e.g. array items, object properties) with
///   permissive defaults when absent.
pub(super) fn sanitize_json_schema(value: &mut JsonValue) {
    match value {
        JsonValue::Bool(_) => {
            // JSON Schema boolean form: true/false. Coerce to an accept-all string.
            *value = json!({ "type": "string" });
        }
        JsonValue::Array(arr) => {
            for v in arr.iter_mut() {
                sanitize_json_schema(v);
            }
        }
        JsonValue::Object(map) => {
            // First, recursively sanitize known nested schema holders
            if let Some(props) = map.get_mut("properties")
                && let Some(props_map) = props.as_object_mut()
            {
                for (_k, v) in props_map.iter_mut() {
                    sanitize_json_schema(v);
                }
            }
            if let Some(items) = map.get_mut("items") {
                sanitize_json_schema(items);
            }
            // Some schemas use oneOf/anyOf/allOf - sanitize their entries
            for combiner in ["oneOf", "anyOf", "allOf", "prefixItems"] {
                if let Some(v) = map.get_mut(combiner) {
                    sanitize_json_schema(v);
                }
            }

            // Normalize/ensure type
            let mut ty = map.get("type").and_then(|v| v.as_str()).map(str::to_string);

            // If type is an array (union), pick first supported; else leave to inference
            if ty.is_none()
                && let Some(JsonValue::Array(types)) = map.get("type")
            {
                for t in types {
                    if let Some(tt) = t.as_str()
                        && matches!(
                            tt,
                            "object" | "array" | "string" | "number" | "integer" | "boolean"
                        )
                    {
                        ty = Some(tt.to_string());
                        break;
                    }
                }
            }

            // Infer type if still missing
            if ty.is_none() {
                if map.contains_key("properties")
                    || map.contains_key("required")
                    || map.contains_key("additionalProperties")
                {
                    ty = Some("object".to_string());
                } else if map.contains_key("items") || map.contains_key("prefixItems") {
                    ty = Some("array".to_string());
                } else if map.contains_key("enum")
                    || map.contains_key("const")
                    || map.contains_key("format")
                {
                    ty = Some("string".to_string());
                } else if map.contains_key("minimum")
                    || map.contains_key("maximum")
                    || map.contains_key("exclusiveMinimum")
                    || map.contains_key("exclusiveMaximum")
                    || map.contains_key("multipleOf")
                {
                    ty = Some("number".to_string());
                }
            }
            // If we still couldn't infer, default to string
            let ty = ty.unwrap_or_else(|| "string".to_string());
            map.insert("type".to_string(), JsonValue::String(ty.to_string()));

            // Ensure object schemas have properties map
            if ty == "object" {
                if !map.contains_key("properties") {
                    map.insert(
                        "properties".to_string(),
                        JsonValue::Object(serde_json::Map::new()),
                    );
                }
                // If additionalProperties is an object schema, sanitize it too.
                // Leave booleans as-is, since JSON Schema allows boolean here.
                if let Some(ap) = map.get_mut("additionalProperties") {
                    let is_bool = matches!(ap, JsonValue::Bool(_));
                    if !is_bool {
                        sanitize_json_schema(ap);
                    }
                }
            }

            // Ensure array schemas have items
            if ty == "array" && !map.contains_key("items") {
                map.insert("items".to_string(), json!({ "type": "string" }));
            }
        }
        _ => {}
    }
}
