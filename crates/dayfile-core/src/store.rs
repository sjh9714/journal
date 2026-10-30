//! The one place every surface (CLI, MCP, desktop) goes through.

use crate::entry::{now_in, parse_tz, system_timezone, Entry, EntrySummary, Frontmatter, Location, NewEntry, Weather};
use crate::frontmatter;
use crate::index::Index;
use crate::{Error, Result};
use chrono::{Datelike, NaiveDate};
use sha2::{Digest, Sha256};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;
use walkdir::WalkDir;

pub use crate::index::{Drafts, ListFilter};

pub const ENTRIES_DIR: &str = "entries";
pub const ATTACHMENTS_DIR: &str = "attachments";
pub const MARKER_DIR: &str = ".dayfile";

pub struct Store {
    root: PathBuf,
    index: Index,
    /// Files under entries/ the last reindex could not parse. Shown to the
    /// user so a sync conflict or a hand edit never goes unnoticed.
    problems: Vec<Problem>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Problem {
    /// Path relative to the journal root.
    pub path: String,
    pub reason: String,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct ReindexStats {
    pub added: usize,
    pub updated: usize,
    pub removed: usize,
    pub unchanged: usize,
    /// Files that could not be parsed and were left out of the index.
    pub skipped: usize,
}

/// Partial update. Outer `None` = leave alone; for `location`/`weather`
/// an explicit JSON `null` clears the field.
#[derive(Debug, Default, Clone, serde::Deserialize)]
pub struct EntryPatch {
    pub body: Option<String>,
    pub tags: Option<Vec<String>>,
    pub starred: Option<bool>,
    pub journal: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    pub location: Option<Option<Location>>,
    #[serde(default, deserialize_with = "double_option")]
    pub weather: Option<Option<Weather>>,
}

fn double_option<'de, T, D>(de: D) -> std::result::Result<Option<Option<T>>, D::Error>
where
    T: serde::Deserialize<'de>,
    D: serde::Deserializer<'de>,
{
    Ok(Some(Option::<T>::deserialize(de)?))
}

impl Store {
    /// Create the folder layout if needed and open the store.
    pub fn init(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref().to_path_buf();
        migrate_marker(&root)?;
        std::fs::create_dir_all(root.join(ENTRIES_DIR))?;
        std::fs::create_dir_all(root.join(ATTACHMENTS_DIR))?;
        std::fs::create_dir_all(root.join(MARKER_DIR))?;
        let marker = root.join(MARKER_DIR).join("dayfile.toml");
        if !marker.exists() {
            std::fs::write(&marker, "# dayfile journal folder marker. Safe to sync.\nformat = 1\n")?;
        }
        let store = Self::open(root)?;
        store.ensure_default_templates()?;
        Ok(store)
    }

    /// Open an existing journal folder. Runs an incremental reindex.
    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref().to_path_buf();
        if !root.join(ENTRIES_DIR).is_dir() {
            return Err(Error::RootMissing(root));
        }
        migrate_marker(&root)?;
        let root = root.canonicalize()?;
        let index = Index::open(&Self::index_path(&root))?;
        let mut store = Self { root, index, problems: Vec::new() };
        store.reindex(false)?;
        let _ = store.purge_trash(crate::trash::RETENTION_DAYS);
        Ok(store)
    }

    /// Open with a throwaway in-memory index (tests, one-shot tools).
    pub fn open_ephemeral(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref().canonicalize()?;
        if !root.join(ENTRIES_DIR).is_dir() {
            return Err(Error::RootMissing(root));
        }
        let index = Index::open_in_memory()?;
        let mut store = Self { root, index, problems: Vec::new() };
        store.reindex(true)?;
        Ok(store)
    }

    /// Where the SQLite index lives: outside the journal folder so sync tools
    /// never see it. `DAYFILE_INDEX_DIR` overrides the base directory.
    pub fn index_path(root: &Path) -> PathBuf {
        let base = std::env::var_os("DAYFILE_INDEX_DIR")
            .map(PathBuf::from)
            .or_else(|| dirs::data_local_dir().map(|d| d.join("dayfile")))
            .unwrap_or_else(|| root.join(MARKER_DIR));
        let digest = Sha256::digest(root.to_string_lossy().as_bytes());
        let short = digest.iter().take(6).map(|b| format!("{b:02x}")).collect::<String>();
        base.join(short).join("index.db")
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Files skipped by the last reindex, with the parse error.
    pub fn problems(&self) -> &[Problem] {
        &self.problems
    }

    pub(crate) fn index_mut(&mut self) -> &mut Index {
        &mut self.index
    }

    pub(crate) fn index_ref(&self) -> &Index {
        &self.index
    }

    // ----- write -----

    pub fn create(&mut self, new: NewEntry) -> Result<Entry> {
        let timezone = new.timezone.unwrap_or_else(system_timezone);
        let tz = parse_tz(&timezone)?;
        let created = new.created.unwrap_or_else(|| now_in(&tz));
        let uuid = new.uuid.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        if self.index.path_of(&uuid)?.is_some() {
            return self.get(&uuid);
        }
        let meta = Frontmatter {
            uuid: uuid.clone(),
            created,
            timezone,
            journal: new.journal.unwrap_or_else(|| "default".into()),
            tags: normalize_tags(new.tags),
            starred: new.starred,
            location: new.location,
            weather: new.weather,
            extra: new.extra,
            status: new.draft.then(|| crate::entry::DRAFT.to_string()),
            traces: new.traces,
        };
        let path = entry_rel_path(&meta);
        let entry = Entry { meta, body: normalize_body(new.body), path };
        self.write_entry(&entry)?;
        Ok(entry)
    }

    /// Create an entry from a template. Variables are rendered with the entry's
    /// own creation time; template tags/journal apply unless `new` sets them.
    pub fn create_from_template(&mut self, slug: &str, mut new: NewEntry) -> Result<Entry> {
        let t = self.template(slug)?;
        let timezone = new.timezone.clone().unwrap_or_else(system_timezone);
        let tz = parse_tz(&timezone)?;
        let created = new.created.unwrap_or_else(|| now_in(&tz));
        let rendered = crate::template::render(&t.body, created);
        new.body = if new.body.trim().is_empty() { rendered } else { format!("{rendered}\n{}", new.body) };
        if new.tags.is_empty() {
            new.tags = t.tags.clone();
        }
        if new.journal.is_none() {
            new.journal = t.journal.clone();
        }
        new.created = Some(created);
        new.timezone = Some(timezone);
        self.create(new)
    }

    pub fn update(&mut self, id: &str, patch: EntryPatch) -> Result<Entry> {
        let mut entry = self.get(id)?;
        if let Some(b) = patch.body {
            entry.body = normalize_body(b);
        }
        if let Some(t) = patch.tags {
            entry.meta.tags = normalize_tags(t);
        }
        if let Some(s) = patch.starred {
            entry.meta.starred = s;
        }
        if let Some(j) = patch.journal {
            entry.meta.journal = j;
        }
        if let Some(l) = patch.location {
            entry.meta.location = l;
        }
        if let Some(w) = patch.weather {
            entry.meta.weather = w;
        }
        self.write_entry(&entry)?;
        Ok(entry)
    }

    pub fn append(&mut self, id: &str, text: &str) -> Result<Entry> {
        let entry = self.get(id)?;
        let mut body = normalize_body(entry.body.clone());
        if !body.is_empty() {
            body.push('\n');
        }
        body.push_str(text);
        self.update(&entry.meta.uuid, EntryPatch { body: Some(body), ..Default::default() })
    }

    /// Soft delete: moves the file to the trash (see `trash.rs`).
    pub fn delete(&mut self, id: &str) -> Result<Entry> {
        self.trash(id)
    }

    /// Copy a file into attachments/ (content-addressed) and return the
    /// relative path to reference from Markdown.
    pub fn attach(&self, source: &Path) -> Result<PathBuf> {
        let bytes = std::fs::read(source)?;
        let digest = Sha256::digest(&bytes);
        let hex = digest.iter().map(|b| format!("{b:02x}")).collect::<String>();
        let ext = source
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_lowercase())
            .unwrap_or_else(|| "bin".into());
        let rel = PathBuf::from(ATTACHMENTS_DIR).join(format!("{hex}.{ext}"));
        let dest = self.root.join(&rel);
        if !dest.exists() {
            std::fs::write(&dest, &bytes)?;
        }
        Ok(rel)
    }

    // ----- read -----

    pub fn exists(&self, uuid: &str) -> Result<bool> {
        Ok(self.index.path_of(uuid)?.is_some())
    }

    /// Markdown link target for an attachment, relative to an entry file
    /// (entries/YYYY/x.md -> ../../attachments/x).
    pub fn attachment_link(rel: &Path) -> String {
        format!("../../{}", rel.to_string_lossy().replace('\\', "/"))
    }

    pub fn get(&self, id: &str) -> Result<Entry> {
        let uuid = self.index.resolve(id)?;
        let rel = self.index.path_of(&uuid)?.ok_or_else(|| Error::NotFound(id.to_string()))?;
        self.read_entry(&rel)
    }

    pub fn list(&self, filter: &ListFilter) -> Result<Vec<EntrySummary>> {
        self.index.list(filter)
    }

    pub fn count(&self, filter: &ListFilter) -> Result<usize> {
        self.index.count(filter)
    }

    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<EntrySummary>> {
        self.index.search(query, limit)
    }

    pub fn on_this_day(&self, date: NaiveDate) -> Result<Vec<EntrySummary>> {
        self.index.on_this_day(date)
    }

    /// Entries whose local date is `date`.
    pub fn on_date(&self, date: NaiveDate) -> Result<Vec<EntrySummary>> {
        self.index.list(&ListFilter { from: Some(date), to: Some(date), ascending: true, ..Default::default() })
    }

    pub fn tags(&self) -> Result<Vec<(String, usize)>> {
        self.index.tags()
    }

    pub fn journals(&self) -> Result<Vec<(String, usize)>> {
        self.index.journals()
    }

    pub fn read_entry(&self, rel: &Path) -> Result<Entry> {
        let abs = self.root.join(rel);
        let text = std::fs::read_to_string(&abs)?;
        let (meta, body) = frontmatter::parse(&abs, &text)?;
        Ok(Entry { meta, body, path: rel.to_path_buf() })
    }

    // ----- index maintenance -----

    /// Walk the entries folder and bring the index up to date.
    /// `full` drops the index first.
    pub fn reindex(&mut self, full: bool) -> Result<ReindexStats> {
        if full {
            self.index.clear()?;
        }
        let known = self.index.known_files()?;
        let mut seen: HashMap<String, ()> = HashMap::new();
        let mut stats = ReindexStats::default();
        let mut problems = Vec::new();
        let entries_dir = self.root.join(ENTRIES_DIR);
        for item in WalkDir::new(&entries_dir).into_iter().filter_map(|e| e.ok()) {
            if !item.file_type().is_file() || item.path().extension().and_then(|e| e.to_str()) != Some("md") {
                continue;
            }
            let rel = item.path().strip_prefix(&self.root).unwrap_or(item.path()).to_path_buf();
            let key = rel.to_string_lossy().replace('\\', "/");
            let mtime = file_mtime(item.path());
            seen.insert(key.clone(), ());
            let is_new = match known.get(&key) {
                Some((_, m)) if *m == mtime => {
                    stats.unchanged += 1;
                    // Still broken since last time: keep reporting it.
                    if let Some(p) = self.problems.iter().find(|p| p.path == key) {
                        problems.push(p.clone());
                    }
                    continue;
                }
                Some(_) => false,
                None => true,
            };
            match self.read_entry(&rel) {
                Ok(entry) => {
                    self.index.upsert(&entry, mtime)?;
                    if is_new {
                        stats.added += 1;
                    } else {
                        stats.updated += 1;
                    }
                }
                Err(e) => {
                    // A broken file must not take the whole index down.
                    eprintln!("journal: skipping {}: {e}", rel.display());
                    stats.skipped += 1;
                    let reason = match &e {
                        Error::Frontmatter { msg, .. } => msg.clone(),
                        other => other.to_string(),
                    };
                    problems.push(Problem { path: key.clone(), reason });
                }
            }
        }
        for (path, (uuid, _)) in known {
            if !seen.contains_key(&path) {
                self.index.remove(&uuid)?;
                stats.removed += 1;
            }
        }
        problems.sort_by(|a, b| a.path.cmp(&b.path));
        let memory: Vec<Problem> = self.problems.drain(..).filter(|p| p.path.starts_with(crate::memory::MEMORY_DIR)).collect();
        self.problems = problems;
        self.problems.extend(memory);
        self.reindex_memory()?;
        Ok(stats)
    }

    /// Swap in the memory files' problems, keeping the entries' ones.
    pub(crate) fn set_memory_problems(&mut self, memory: Vec<Problem>) {
        self.problems.retain(|p| !p.path.starts_with(crate::memory::MEMORY_DIR));
        self.problems.extend(memory);
    }

    pub(crate) fn write_entry(&mut self, entry: &Entry) -> Result<()> {
        let abs = self.root.join(&entry.path);
        if let Some(parent) = abs.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = frontmatter::render(&entry.meta, &entry.body)?;
        // Write to a temp file then rename so sync tools never see a half file.
        let tmp = abs.with_extension("md.tmp");
        std::fs::write(&tmp, text)?;
        std::fs::rename(&tmp, &abs)?;
        self.index.upsert(entry, file_mtime(&abs))?;
        Ok(())
    }
}

/// Folders made before the rename keep their marker and trash in `.journal/`.
fn migrate_marker(root: &Path) -> Result<()> {
    let old = root.join(".journal");
    let new = root.join(MARKER_DIR);
    if old.is_dir() && !new.exists() {
        std::fs::rename(&old, &new)?;
        let marker = new.join("journal.toml");
        if marker.exists() {
            std::fs::rename(marker, new.join("dayfile.toml"))?;
        }
    }
    Ok(())
}

fn file_mtime(p: &Path) -> i64 {
    std::fs::metadata(p)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_nanos() as i64)
        .unwrap_or(0)
}

/// entries/YYYY/YYYY-MM-DD-<uuid8>.md, dated in the entry's own timezone.
pub fn entry_rel_path(meta: &Frontmatter) -> PathBuf {
    let d = meta.created.date_naive();
    let short: String = meta.uuid.chars().filter(|c| c.is_ascii_hexdigit()).take(8).collect();
    PathBuf::from(ENTRIES_DIR)
        .join(d.year().to_string())
        .join(format!("{}-{short}.md", d.format("%Y-%m-%d")))
}

/// Bodies end with exactly one newline (or are empty) so what `create`
/// returns is byte-for-byte what `get` reads back from disk.
fn normalize_body(body: String) -> String {
    let trimmed = body.trim_end_matches(['\n', '\r']);
    if trimmed.is_empty() {
        String::new()
    } else {
        format!("{trimmed}\n")
    }
}

fn normalize_tags(tags: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = tags
        .into_iter()
        .map(|t| t.trim().trim_start_matches('#').to_string())
        .filter(|t| !t.is_empty())
        .collect();
    out.sort();
    out.dedup();
    out
}
