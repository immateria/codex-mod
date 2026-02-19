use anyhow::Context;
use code_core::config::Config;
use code_core::SessionCatalog;
use code_core::SessionQuery;
use code_core::entry_to_rollout_path;
use code_protocol::protocol::SessionSource;
use std::path::PathBuf;

pub(crate) async fn resolve_resume_path(
    config: &Config,
    args: &crate::cli::ResumeArgs,
) -> anyhow::Result<Option<PathBuf>> {
    if !args.last && args.session_id.is_none() {
        return Ok(None);
    }

    let catalog = SessionCatalog::new(config.code_home.clone());

    if let Some(id_str) = args.session_id.as_deref() {
        let entry = catalog
            .find_by_id(id_str)
            .await
            .context("failed to look up session by id")?;
        Ok(entry.map(|entry| entry_to_rollout_path(&config.code_home, &entry)))
    } else if args.last {
        let query = SessionQuery {
            cwd: (!args.all).then(|| config.cwd.clone()),
            git_root: None,
            sources: vec![SessionSource::Cli, SessionSource::VSCode, SessionSource::Exec],
            min_user_messages: 1,
            include_archived: false,
            include_deleted: false,
            limit: Some(1),
        };
        let entry = catalog
            .get_latest(&query)
            .await
            .context("failed to get latest session from catalog")?;
        Ok(entry.map(|entry| entry_to_rollout_path(&config.code_home, &entry)))
    } else {
        Ok(None)
    }
}
