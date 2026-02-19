use crate::config::ConfigToml;
use crate::config_types::RawMcpServerConfig;
use schemars::r#gen::SchemaGenerator;
use schemars::r#gen::SchemaSettings;
use schemars::schema::InstanceType;
use schemars::schema::ObjectValidation;
use schemars::schema::RootSchema;
use schemars::schema::Schema;
use schemars::schema::SchemaObject;
use serde_json::Map;
use serde_json::Value;
use std::path::Path;

/// Canonical upstream Codex schema, embedded for compatibility validation.
pub fn codex_config_schema_json() -> &'static [u8] {
    include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/config.schema.codex.json"))
}

/// Schema for the `[mcp_servers]` map using the raw input shape.
pub(crate) fn mcp_servers_schema(schema_gen: &mut SchemaGenerator) -> Schema {
    let mut object = SchemaObject {
        instance_type: Some(InstanceType::Object.into()),
        ..Default::default()
    };

    let validation = ObjectValidation {
        additional_properties: Some(Box::new(schema_gen.subschema_for::<RawMcpServerConfig>())),
        ..Default::default()
    };
    object.object = Some(Box::new(validation));

    Schema::Object(object)
}

/// Build the "upgraded" config schema for `config.toml` from `code-core` types.
pub fn config_schema() -> RootSchema {
    SchemaSettings::draft07()
        .with(|settings| {
            settings.option_add_null_type = false;
        })
        .into_generator()
        .into_root_schema_for::<ConfigToml>()
}

/// Canonicalize a JSON value by sorting its keys.
fn canonicalize(value: &Value) -> Value {
    match value {
        Value::Array(items) => Value::Array(items.iter().map(canonicalize).collect()),
        Value::Object(map) => {
            let mut entries: Vec<_> = map.iter().collect();
            entries.sort_by(|(left, _), (right, _)| left.cmp(right));
            let mut sorted = Map::with_capacity(map.len());
            for (key, child) in entries {
                sorted.insert(key.clone(), canonicalize(child));
            }
            Value::Object(sorted)
        }
        _ => value.clone(),
    }
}

/// Render the upgraded config schema as pretty-printed JSON.
pub fn config_schema_json() -> anyhow::Result<Vec<u8>> {
    let schema = config_schema();
    let value = serde_json::to_value(schema)?;
    let value = canonicalize(&value);
    Ok(serde_json::to_vec_pretty(&value)?)
}

/// Write the upgraded config schema fixture to disk.
pub fn write_config_schema(out_path: &Path) -> anyhow::Result<()> {
    std::fs::write(out_path, config_schema_json()?)?;
    Ok(())
}

