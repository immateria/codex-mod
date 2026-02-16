use serde_json::Value;

mod parse;

#[derive(Default)]
pub(super) struct InvocationMetadata {
    pub(super) batch_id: Option<String>,
    pub(super) agent_ids: Vec<String>,
    pub(super) models: Vec<String>,
    pub(super) task: Option<String>,
    pub(super) plan: Vec<String>,
    pub(super) label: Option<String>,
    pub(super) action: Option<String>,
    pub(super) context: Option<String>,
    pub(super) write: Option<bool>,
    pub(super) read_only: Option<bool>,
}

impl InvocationMetadata {
    pub(super) fn from(tool_name: &str, params: Option<&Value>) -> Self {
        let mut meta = InvocationMetadata::default();
        parse::populate_from_params(&mut meta, params);
        parse::finalize(&mut meta, tool_name);
        meta
    }

    pub(super) fn resolved_write_flag(&self) -> Option<bool> {
        if let Some(flag) = self.write {
            Some(flag)
        } else {
            self.read_only.map(|ro_flag| !ro_flag)
        }
    }
}
