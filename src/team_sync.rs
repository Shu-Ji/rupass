use std::path::Path;

use anyhow::Result;

use crate::git_sync::sync_team_repo;
use crate::s3_sync::sync_team_store;
use crate::storage::{SyncBackend, TeamConfig};

pub(crate) fn has_remote(config: &TeamConfig) -> bool {
    config.has_remote()
}

pub(crate) fn sync_team_backends(repo_dir: &Path, config: &TeamConfig) -> Result<()> {
    match config.effective_sync_backend() {
        Some(SyncBackend::Git) => sync_team_repo(repo_dir, config)?,
        Some(SyncBackend::S3) => sync_team_store(repo_dir, config)?,
        None => {}
    }
    Ok(())
}
