//! Soft delete. Deleted entries move to `.dayfile/trash/` inside the journal
//! folder (so they sync like everything else) and are purged after 30 days.
//! File name: `<deleted_unix_secs>__<original file name>`.

use crate::entry::Entry;
use crate::store::{entry_rel_path, Store, MARKER_DIR};
use crate::{frontmatter, Error, Result};
use chrono::{DateTime, FixedOffset, TimeZone, Utc};
use serde::Serialize;
use std::path::{Path, PathBuf};

pub const TRASH_DIR: &str = ".dayfile/trash";
pub const RETENTION_DAYS: i64 = 30;

#[derive(Debug, Clone, Serialize)]
pub struct TrashedEntry {
    pub uuid: String,
    pub title: String,
    pub created: DateTime<FixedOffset>,
    pub deleted_at: DateTime<Utc>,
    /// Path relative to the journal root, inside the trash folder.
    pub trash_path: String,
    pub journal: String,
}

fn trash_dir(root: &Path) -> PathBuf {
    root.join(TRASH_DIR)
}

fn parse_name(name: &str) -> Option<(i64, &str)> {
    let (ts, rest) = name.split_once("__")?;
    Some((ts.parse().ok()?, rest))
}

impl Store {
    /// Move an entry to the trash. Returns the entry as it was.
    pub fn trash(&mut self, id: &str) -> Result<Entry> {
        let entry = self.get(id)?;
        let dir = trash_dir(self.root());
        std::fs::create_dir_all(&dir)?;
        let file_name = entry.path.file_name().and_then(|n| n.to_str()).unwrap_or("entry.md");
        let dest = dir.join(format!("{}__{}", Utc::now().timestamp(), file_name));
        std::fs::rename(self.root().join(&entry.path), &dest)?;
        self.index_mut().remove(&entry.meta.uuid)?;
        Ok(entry)
    }

    pub fn trash_list(&self) -> Result<Vec<TrashedEntry>> {
        let dir = trash_dir(self.root());
        let mut out = Vec::new();
        let Ok(rd) = std::fs::read_dir(&dir) else { return Ok(out) };
        for item in rd.filter_map(|e| e.ok()) {
            let name = item.file_name().to_string_lossy().into_owned();
            let Some((ts, _)) = parse_name(&name) else { continue };
            if !name.ends_with(".md") {
                continue;
            }
            let text = match std::fs::read_to_string(item.path()) {
                Ok(t) => t,
                Err(_) => continue,
            };
            let Ok((meta, body)) = frontmatter::parse(&item.path(), &text) else { continue };
            out.push(TrashedEntry {
                uuid: meta.uuid.clone(),
                title: crate::entry::title_of(&body),
                created: meta.created,
                deleted_at: Utc.timestamp_opt(ts, 0).single().unwrap_or_else(Utc::now),
                trash_path: format!("{TRASH_DIR}/{name}"),
                journal: meta.journal,
            });
        }
        out.sort_by_key(|t| std::cmp::Reverse(t.deleted_at));
        Ok(out)
    }

    /// Put a trashed entry back where it belongs. `id` is a uuid or prefix.
    pub fn restore(&mut self, id: &str) -> Result<Entry> {
        let needle = id.replace('-', "").to_lowercase();
        let hit = self
            .trash_list()?
            .into_iter()
            .find(|t| t.uuid.replace('-', "").starts_with(&needle))
            .ok_or_else(|| Error::NotFound(id.to_string()))?;
        let src = self.root().join(&hit.trash_path);
        let text = std::fs::read_to_string(&src)?;
        let (meta, _) = frontmatter::parse(&src, &text)?;
        let mut rel = entry_rel_path(&meta);
        if self.root().join(&rel).exists() {
            // Same uuid written again meanwhile: keep both, never overwrite.
            let stem = rel.file_stem().and_then(|s| s.to_str()).unwrap_or("entry").to_string();
            rel.set_file_name(format!("{stem}-restored.md"));
        }
        if let Some(parent) = self.root().join(&rel).parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::rename(&src, self.root().join(&rel))?;
        self.reindex(false)?;
        self.get(&hit.uuid)
    }

    /// Delete everything in the trash for good.
    pub fn empty_trash(&mut self) -> Result<usize> {
        let dir = trash_dir(self.root());
        let mut n = 0;
        if let Ok(rd) = std::fs::read_dir(&dir) {
            for item in rd.filter_map(|e| e.ok()) {
                if item.path().is_file() && std::fs::remove_file(item.path()).is_ok() {
                    n += 1;
                }
            }
        }
        Ok(n)
    }

    /// Permanently delete trashed entries older than `days`. Runs on open.
    pub fn purge_trash(&mut self, days: i64) -> Result<usize> {
        let dir = trash_dir(self.root());
        let cutoff = Utc::now().timestamp() - days * 86_400;
        let mut n = 0;
        if let Ok(rd) = std::fs::read_dir(&dir) {
            for item in rd.filter_map(|e| e.ok()) {
                let name = item.file_name().to_string_lossy().into_owned();
                if let Some((ts, _)) = parse_name(&name) {
                    if ts < cutoff && std::fs::remove_file(item.path()).is_ok() {
                        n += 1;
                    }
                }
            }
        }
        Ok(n)
    }

    /// Bypass the trash. Used by `dayfile delete --permanent`.
    pub fn delete_permanent(&mut self, id: &str) -> Result<Entry> {
        let entry = self.get(id)?;
        std::fs::remove_file(self.root().join(&entry.path))?;
        self.index_mut().remove(&entry.meta.uuid)?;
        Ok(entry)
    }
}

/// Where the marker dir lives, re-exported for callers that need to skip it.
pub fn is_trash_path(root: &Path, p: &Path) -> bool {
    p.starts_with(root.join(MARKER_DIR))
}
