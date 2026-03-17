//! Metadata extraction from rollout files for backfill and indexing.

use std::path::{Path, PathBuf};

use chrono::{DateTime, NaiveDateTime, Utc};

use super::list::parse_timestamp_uuid_from_filename;
use super::policy::{RolloutItem, SessionMetaLine, SessionSource};
use super::recorder::RolloutRecorder;

/// Extracted metadata from a rollout file.
#[derive(Debug, Clone)]
pub struct RolloutMetadata {
    pub thread_id: String,
    pub rollout_path: PathBuf,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub cwd: PathBuf,
    pub source: SessionSource,
    pub model_provider: Option<String>,
    pub cli_version: Option<String>,
    pub git_branch: Option<String>,
    pub git_sha: Option<String>,
    pub git_origin_url: Option<String>,
    pub agent_nickname: Option<String>,
    pub agent_role: Option<String>,
    pub first_user_message: Option<String>,
    pub memory_mode: Option<String>,
}

/// Build metadata from a [`SessionMetaLine`].
pub fn metadata_from_session_meta(
    meta_line: &SessionMetaLine,
    rollout_path: &Path,
) -> Option<RolloutMetadata> {
    let created_at = parse_timestamp_to_utc(&meta_line.meta.timestamp)?;
    Some(RolloutMetadata {
        thread_id: meta_line.meta.id.clone(),
        rollout_path: rollout_path.to_path_buf(),
        created_at,
        updated_at: created_at,
        cwd: meta_line.meta.cwd.clone(),
        source: meta_line.meta.source.clone(),
        model_provider: meta_line.meta.model_provider.clone(),
        cli_version: Some(meta_line.meta.cli_version.clone()),
        git_branch: meta_line.git.as_ref().and_then(|g| g.branch.clone()),
        git_sha: meta_line.git.as_ref().and_then(|g| g.commit_hash.clone()),
        git_origin_url: meta_line.git.as_ref().and_then(|g| g.repository_url.clone()),
        agent_nickname: meta_line.meta.agent_nickname.clone(),
        agent_role: meta_line.meta.agent_role.clone(),
        first_user_message: None,
        memory_mode: meta_line.meta.memory_mode.clone(),
    })
}

/// Build metadata from rollout items, falling back to filename parsing.
pub fn metadata_from_items(
    items: &[RolloutItem],
    rollout_path: &Path,
) -> Option<RolloutMetadata> {
    // Try session meta first.
    if let Some(meta_line) = items.iter().find_map(|item| match item {
        RolloutItem::SessionMeta(m) => Some(m),
        _ => None,
    }) {
        return metadata_from_session_meta(meta_line, rollout_path);
    }

    // Fall back to filename parsing.
    let file_name = rollout_path.file_name()?.to_str()?;
    if !file_name.starts_with("rollout-") || !file_name.ends_with(".jsonl") {
        return None;
    }
    let (ts_str, uuid) = parse_timestamp_uuid_from_filename(file_name)?;
    let created_at = parse_filename_timestamp(&ts_str)?;

    Some(RolloutMetadata {
        thread_id: uuid.to_string(),
        rollout_path: rollout_path.to_path_buf(),
        created_at,
        updated_at: created_at,
        cwd: PathBuf::new(),
        source: SessionSource::default(),
        model_provider: None,
        cli_version: None,
        git_branch: None,
        git_sha: None,
        git_origin_url: None,
        agent_nickname: None,
        agent_role: None,
        first_user_message: None,
        memory_mode: None,
    })
}

/// Extract full metadata from a rollout file, applying all items.
pub async fn extract_metadata_from_rollout(
    rollout_path: &Path,
) -> anyhow::Result<RolloutMetadata> {
    let (items, _thread_id, _parse_errors) =
        RolloutRecorder::load_rollout_items(rollout_path).await?;
    if items.is_empty() {
        return Err(anyhow::anyhow!(
            "empty session file: {}",
            rollout_path.display()
        ));
    }
    let mut metadata = metadata_from_items(&items, rollout_path).ok_or_else(|| {
        anyhow::anyhow!(
            "rollout missing metadata: {}",
            rollout_path.display()
        )
    })?;

    // Scan for first user message and latest memory_mode.
    for item in &items {
        match item {
            RolloutItem::EventMsg(crate::protocol::event::EventMsg::UserMessage(user)) => {
                if metadata.first_user_message.is_none() {
                    let msg = user.message.trim().to_string();
                    if !msg.is_empty() {
                        metadata.first_user_message = Some(msg);
                    }
                }
            }
            RolloutItem::SessionMeta(meta_line) => {
                if meta_line.meta.memory_mode.is_some() {
                    metadata.memory_mode = meta_line.meta.memory_mode.clone();
                }
            }
            _ => {}
        }
    }

    // Update mtime.
    if let Ok(m) = tokio::fs::metadata(rollout_path).await {
        if let Ok(modified) = m.modified() {
            let dt: DateTime<Utc> = modified.into();
            metadata.updated_at = dt;
        }
    }

    Ok(metadata)
}

// ── Timestamp parsing ────────────────────────────────────────────

fn parse_timestamp_to_utc(ts: &str) -> Option<DateTime<Utc>> {
    // Try RFC3339 first.
    if let Ok(dt) = DateTime::parse_from_rfc3339(ts) {
        return Some(dt.with_timezone(&Utc));
    }
    // Try filename format: YYYY-MM-DDThh-mm-ss
    parse_filename_timestamp(ts)
}

fn parse_filename_timestamp(ts: &str) -> Option<DateTime<Utc>> {
    let naive = NaiveDateTime::parse_from_str(ts, "%Y-%m-%dT%H-%M-%S").ok()?;
    Some(DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc))
}
