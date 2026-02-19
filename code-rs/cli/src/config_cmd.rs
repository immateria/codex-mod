use anyhow::Context;
use anyhow::Result;
use clap::Parser;
use clap::Subcommand;
use clap::ValueEnum;
use code_core::config::find_code_home;
use jsonschema::Draft;
use jsonschema::JSONSchema;
use serde_json::Value as JsonValue;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;

#[derive(Debug, Parser)]
pub struct ConfigCli {
    #[command(subcommand)]
    subcommand: ConfigSubcommand,
}

#[derive(Debug, Subcommand)]
enum ConfigSubcommand {
    /// Print the JSON Schema for `config.toml`.
    Schema(SchemaArgs),

    /// Validate a config file against one or more schemas.
    Validate(ValidateArgs),
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum SchemaKind {
    /// Upstream Codex schema (openai/codex).
    Codex,
    /// This fork's schema (code-rs).
    Code,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum ValidateSchemaKind {
    Codex,
    Code,
    Both,
}

#[derive(Debug, Parser)]
struct SchemaArgs {
    /// Which schema to output.
    #[arg(long, value_enum, default_value_t = SchemaKind::Code)]
    schema: SchemaKind,

    /// Optional path to write the schema JSON to. When omitted, prints to stdout.
    #[arg(short, long, value_name = "PATH")]
    out: Option<PathBuf>,
}

#[derive(Debug, Parser)]
struct ValidateArgs {
    /// Which schema(s) to validate against.
    #[arg(long, value_enum, default_value_t = ValidateSchemaKind::Both)]
    schema: ValidateSchemaKind,

    /// Path to the config file to validate (defaults to `CODE_HOME/config.toml`).
    #[arg(short, long, value_name = "PATH")]
    path: Option<PathBuf>,
}

impl ConfigCli {
    pub async fn run(self) -> Result<()> {
        match self.subcommand {
            ConfigSubcommand::Schema(args) => run_schema(args),
            ConfigSubcommand::Validate(args) => run_validate(args),
        }
    }
}

fn run_schema(args: SchemaArgs) -> Result<()> {
    let schema_json = match args.schema {
        SchemaKind::Codex => code_core::config::schema::codex_config_schema_json().to_vec(),
        SchemaKind::Code => code_core::config::schema::config_schema_json()?,
    };

    match args.out {
        Some(out) => {
            std::fs::write(&out, schema_json)
                .with_context(|| format!("failed to write schema to {out}", out = out.display()))?;
        }
        None => {
            // Print schema JSON to stdout (valid JSON + trailing newline).
            let mut out = schema_json;
            out.push(b'\n');
            std::io::stdout()
                .lock()
                .write_all(&out)
                .context("failed to write schema to stdout")?;
        }
    }

    Ok(())
}

fn run_validate(args: ValidateArgs) -> Result<()> {
    let path = match args.path {
        Some(path) => path,
        None => find_code_home()
            .context("failed to resolve CODE_HOME")?
            .join("config.toml"),
    };

    let contents = std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read config file {path}", path = path.display()))?;

    let toml_value: toml::Value = toml::from_str(&contents)
        .with_context(|| format!("failed to parse TOML in {path}", path = path.display()))?;
    let instance_json: JsonValue =
        serde_json::to_value(toml_value).context("failed to convert TOML to JSON value")?;

    let include_codex = matches!(
        args.schema,
        ValidateSchemaKind::Codex | ValidateSchemaKind::Both
    );
    let include_code = matches!(args.schema, ValidateSchemaKind::Code | ValidateSchemaKind::Both);

    let mut ok = true;
    if include_codex {
        ok &= validate_one(
            "codex",
            code_core::config::schema::codex_config_schema_json(),
            &instance_json,
            &path,
        )?;
    }
    if include_code {
        let schema = code_core::config::schema::config_schema_json()?;
        ok &= validate_one("code", &schema, &instance_json, &path)?;
    }

    if ok {
        Ok(())
    } else {
        Err(anyhow::anyhow!("config validation failed"))
    }
}

fn validate_one(
    label: &str,
    schema_json: &[u8],
    instance_json: &JsonValue,
    path: &Path,
) -> Result<bool> {
    let schema_value: JsonValue = serde_json::from_slice(schema_json)
        .with_context(|| format!("failed to parse {label} schema JSON"))?;

    let compiled = JSONSchema::options()
        .with_draft(Draft::Draft7)
        .compile(&schema_value)
        .map_err(|err| anyhow::anyhow!("failed to compile {label} schema: {err}"))?;

    match compiled.validate(instance_json) {
        Ok(()) => {
            println!("Schema {label}: PASS ({path})", path = path.display());
            Ok(true)
        }
        Err(errors) => {
            println!("Schema {label}: FAIL ({path})", path = path.display());
            for error in errors {
                let instance_path = error.instance_path.to_string();
                if instance_path.is_empty() {
                    println!("- {error}");
                } else {
                    println!("- {instance_path}: {error}");
                }
            }
            Ok(false)
        }
    }
}
