use crate::INTERACTIVE_SESSION_SOURCES;
use crate::config_edit;
use crate::config_types::Personality;
use crate::session_catalog::SessionCatalog;
use crate::session_catalog::SessionQuery;
use std::io;
use std::path::Path;
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;

pub const PERSONALITY_MIGRATION_FILENAME: &str = ".model_personality_migration";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PersonalityMigrationStatus {
    SkippedMarker,
    SkippedExplicitPersonality,
    SkippedNoSessions,
    Applied,
}

pub async fn maybe_migrate_model_personality(
    code_home: &Path,
    active_profile: Option<&str>,
    effective_personality: Option<Personality>,
) -> io::Result<PersonalityMigrationStatus> {
    let marker_path = code_home.join(PERSONALITY_MIGRATION_FILENAME);
    if tokio::fs::try_exists(&marker_path).await? {
        return Ok(PersonalityMigrationStatus::SkippedMarker);
    }

    if effective_personality.is_some() {
        create_marker(&marker_path).await?;
        return Ok(PersonalityMigrationStatus::SkippedExplicitPersonality);
    }

    if !has_recorded_sessions(code_home).await? {
        create_marker(&marker_path).await?;
        return Ok(PersonalityMigrationStatus::SkippedNoSessions);
    }

    config_edit::persist_overrides(
        code_home,
        active_profile,
        &[(&["model_personality"], "pragmatic")],
    )
    .await
    .map_err(|err| io::Error::other(format!("failed to persist personality migration: {err}")))?;

    create_marker(&marker_path).await?;
    Ok(PersonalityMigrationStatus::Applied)
}

async fn has_recorded_sessions(code_home: &Path) -> io::Result<bool> {
    let catalog = SessionCatalog::new(code_home.to_path_buf());
    let latest = catalog
        .get_latest(&SessionQuery {
            sources: INTERACTIVE_SESSION_SOURCES.to_vec(),
            min_user_messages: 1,
            include_archived: true,
            include_deleted: false,
            limit: Some(1),
            ..SessionQuery::default()
        })
        .await
        .map_err(|err| io::Error::other(format!("failed to query session catalog: {err}")))?;

    Ok(latest.is_some())
}

async fn create_marker(marker_path: &Path) -> io::Result<()> {
    match OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(marker_path)
        .await
    {
        Ok(mut file) => file.write_all(b"v1\n").await,
        Err(err) if err.kind() == io::ErrorKind::AlreadyExists => Ok(()),
        Err(err) => Err(err),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ConfigToml;
    use crate::config_profile::ConfigProfile;
    use crate::config_types::Personality;
    use code_protocol::ThreadId;
    use code_protocol::models::ContentItem;
    use code_protocol::models::ResponseItem;
    use code_protocol::protocol::RolloutItem;
    use code_protocol::protocol::RolloutLine;
    use code_protocol::protocol::SessionMeta;
    use code_protocol::protocol::SessionMetaLine;
    use code_protocol::protocol::SessionSource;
    use std::path::PathBuf;
    use tempfile::TempDir;

    const TEST_FILE_TIMESTAMP: &str = "2025-01-01T00-00-00";
    const TEST_LINE_TIMESTAMP: &str = "2025-01-01T00:00:00Z";

    async fn read_config_toml(code_home: &Path) -> io::Result<ConfigToml> {
        let contents = tokio::fs::read_to_string(code_home.join("config.toml")).await?;
        toml::from_str(&contents).map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))
    }

    async fn write_rollout_with_user_message(code_home: &Path) -> io::Result<()> {
        let thread_id = ThreadId::new();
        let sessions_dir = code_home.join("sessions").join("2025").join("01").join("01");
        tokio::fs::create_dir_all(&sessions_dir).await?;
        let rollout_path =
            sessions_dir.join(format!("rollout-{TEST_FILE_TIMESTAMP}-{thread_id}.jsonl"));

        let session_meta_line = RolloutLine {
            timestamp: TEST_LINE_TIMESTAMP.to_string(),
            item: RolloutItem::SessionMeta(SessionMetaLine {
                meta: SessionMeta {
                    id: thread_id,
                    forked_from_id: None,
                    timestamp: TEST_LINE_TIMESTAMP.to_string(),
                    cwd: PathBuf::from("."),
                    originator: "test-originator".to_string(),
                    cli_version: "test-version".to_string(),
                    source: SessionSource::Cli,
                    model_provider: None,
                    base_instructions: None,
                    dynamic_tools: None,
                },
                git: None,
            }),
        };

        let user_message_line = RolloutLine {
            timestamp: TEST_LINE_TIMESTAMP.to_string(),
            item: RolloutItem::ResponseItem(ResponseItem::Message {
                id: None,
                role: "user".to_string(),
                content: vec![ContentItem::InputText {
                    text: "hello".to_string(),
                }],
                end_turn: None,
                phase: None,
            }),
        };

        let mut file = tokio::fs::File::create(rollout_path).await?;
        file.write_all(format!("{}\n", serde_json::to_string(&session_meta_line)?).as_bytes())
            .await?;
        file.write_all(format!("{}\n", serde_json::to_string(&user_message_line)?).as_bytes())
            .await?;
        Ok(())
    }

    #[tokio::test]
    async fn applies_when_sessions_exist_and_no_personality() -> io::Result<()> {
        let temp = TempDir::new()?;
        write_rollout_with_user_message(temp.path()).await?;

        let status = maybe_migrate_model_personality(temp.path(), None, None).await?;
        assert_eq!(status, PersonalityMigrationStatus::Applied);
        assert!(temp.path().join(PERSONALITY_MIGRATION_FILENAME).exists());

        let persisted = read_config_toml(temp.path()).await?;
        assert_eq!(persisted.model_personality, Some(Personality::Pragmatic));
        Ok(())
    }

    #[tokio::test]
    async fn applies_to_active_profile_when_present() -> io::Result<()> {
        let temp = TempDir::new()?;
        write_rollout_with_user_message(temp.path()).await?;

        let status = maybe_migrate_model_personality(temp.path(), Some("work"), None).await?;
        assert_eq!(status, PersonalityMigrationStatus::Applied);

        let persisted = read_config_toml(temp.path()).await?;
        let profile = persisted
            .profiles
            .get("work")
            .cloned()
            .unwrap_or_else(ConfigProfile::default);
        assert_eq!(profile.model_personality, Some(Personality::Pragmatic));
        Ok(())
    }

    #[tokio::test]
    async fn skips_when_marker_exists() -> io::Result<()> {
        let temp = TempDir::new()?;
        create_marker(&temp.path().join(PERSONALITY_MIGRATION_FILENAME)).await?;

        let status = maybe_migrate_model_personality(temp.path(), None, None).await?;
        assert_eq!(status, PersonalityMigrationStatus::SkippedMarker);
        assert!(!temp.path().join("config.toml").exists());
        Ok(())
    }

    #[tokio::test]
    async fn skips_when_personality_is_explicit() -> io::Result<()> {
        let temp = TempDir::new()?;
        config_edit::persist_overrides(
            temp.path(),
            None,
            &[(&["model_personality"], "friendly")],
        )
        .await
        .map_err(|err| io::Error::other(format!("failed to write config: {err}")))?;

        let status = maybe_migrate_model_personality(
            temp.path(),
            None,
            Some(Personality::Friendly),
        )
        .await?;
        assert_eq!(
            status,
            PersonalityMigrationStatus::SkippedExplicitPersonality
        );
        assert!(temp.path().join(PERSONALITY_MIGRATION_FILENAME).exists());

        let persisted = read_config_toml(temp.path()).await?;
        assert_eq!(persisted.model_personality, Some(Personality::Friendly));
        Ok(())
    }

    #[tokio::test]
    async fn skips_when_no_sessions_exist() -> io::Result<()> {
        let temp = TempDir::new()?;
        let status = maybe_migrate_model_personality(temp.path(), None, None).await?;
        assert_eq!(status, PersonalityMigrationStatus::SkippedNoSessions);
        assert!(temp.path().join(PERSONALITY_MIGRATION_FILENAME).exists());
        Ok(())
    }
}
