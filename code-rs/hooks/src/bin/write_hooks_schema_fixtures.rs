use std::path::PathBuf;

fn main() -> anyhow::Result<()> {
    let schema_root = std::env::args_os()
        .nth(1).map_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("schema"), PathBuf::from);
    code_hooks::write_schema_fixtures(&schema_root)
}
