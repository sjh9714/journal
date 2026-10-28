//! SQLite index over entry files. Disposable: `Store::reindex` rebuilds it.

use crate::entry::{Entry, EntrySummary};
use crate::{Error, Result};
use chrono::{DateTime, Datelike, FixedOffset, NaiveDate};
use rusqlite::{params, Connection, OptionalExtension, Row};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub struct Index {
    conn: Connection,
}

/// Bump when derived columns (title, words, …) change meaning; the index is
/// then rebuilt from the files on next open.
const INDEX_VERSION: i64 = 2;

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS entries (
    uuid        TEXT PRIMARY KEY,
    path        TEXT NOT NULL UNIQUE,
    created     TEXT NOT NULL,
    created_utc INTEGER NOT NULL,
    date        TEXT NOT NULL,
    year        INTEGER NOT NULL,
    month       INTEGER NOT NULL,
    day         INTEGER NOT NULL,
    timezone    TEXT NOT NULL,
    journal     TEXT NOT NULL,
    starred     INTEGER NOT NULL DEFAULT 0,
    tags        TEXT NOT NULL DEFAULT '[]',
    title       TEXT NOT NULL DEFAULT '',
    mtime       INTEGER NOT NULL,
    words       INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_entries_created ON entries(created_utc DESC);
CREATE INDEX IF NOT EXISTS idx_entries_md ON entries(month, day);
CREATE INDEX IF NOT EXISTS idx_entries_journal ON entries(journal);
CREATE TABLE IF NOT EXISTS entry_tags (
    uuid TEXT NOT NULL,
    tag  TEXT NOT NULL,
    PRIMARY KEY (uuid, tag)
);
CREATE INDEX IF NOT EXISTS idx_entry_tags_tag ON entry_tags(tag);
CREATE VIRTUAL TABLE IF NOT EXISTS entries_fts USING fts5(
    uuid UNINDEXED, title, body, tags, tokenize = 'unicode61 remove_diacritics 2'
);
"#;

#[derive(Debug, Clone, Default)]
pub struct ListFilter {
    pub journal: Option<String>,
    pub tag: Option<String>,
    pub from: Option<NaiveDate>,
    pub to: Option<NaiveDate>,
    pub starred: Option<bool>,
    pub limit: Option<usize>,
    pub offset: usize,
    /// Oldest first when true; default newest first.
    pub ascending: bool,
}

impl Index {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL;")?;
        Self::with_conn(conn)
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::with_conn(Connection::open_in_memory()?)
    }

    fn with_conn(conn: Connection) -> Result<Self> {
        conn.execute_batch(SCHEMA)?;
        // Older indexes predate the words column; adding it is idempotent enough.
        let _ = conn.execute_batch("ALTER TABLE entries ADD COLUMN words INTEGER NOT NULL DEFAULT 0");
        let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version != INDEX_VERSION {
            conn.execute_batch("DELETE FROM entries; DELETE FROM entry_tags; DELETE FROM entries_fts;")?;
            conn.execute_batch(&format!("PRAGMA user_version = {INDEX_VERSION}"))?;
        }
        Ok(Self { conn })
    }

    pub fn upsert(&mut self, entry: &Entry, mtime: i64) -> Result<()> {
        let tx = self.conn.transaction()?;
        let m = &entry.meta;
        let date = entry.local_date();
        let tags_json = serde_json::to_string(&m.tags)?;
        tx.execute(
            "INSERT INTO entries (uuid, path, created, created_utc, date, year, month, day, timezone, journal, starred, tags, title, mtime, words)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
             ON CONFLICT(uuid) DO UPDATE SET
               path=excluded.path, created=excluded.created, created_utc=excluded.created_utc,
               date=excluded.date, year=excluded.year, month=excluded.month, day=excluded.day,
               timezone=excluded.timezone, journal=excluded.journal, starred=excluded.starred,
               tags=excluded.tags, title=excluded.title, mtime=excluded.mtime, words=excluded.words",
            params![
                m.uuid,
                rel(&entry.path),
                m.created.to_rfc3339(),
                entry.created_utc().timestamp(),
                date.to_string(),
                date.year(),
                date.month(),
                date.day(),
                m.timezone,
                m.journal,
                m.starred as i64,
                tags_json,
                entry.title(),
                mtime,
                crate::stats::count_words(&entry.body) as i64,
            ],
        )?;
        tx.execute("DELETE FROM entry_tags WHERE uuid = ?1", params![m.uuid])?;
        for tag in &m.tags {
            tx.execute(
                "INSERT OR IGNORE INTO entry_tags (uuid, tag) VALUES (?1, ?2)",
                params![m.uuid, tag],
            )?;
        }
        tx.execute("DELETE FROM entries_fts WHERE uuid = ?1", params![m.uuid])?;
        tx.execute(
            "INSERT INTO entries_fts (uuid, title, body, tags) VALUES (?1, ?2, ?3, ?4)",
            params![m.uuid, entry.title(), entry.body, m.tags.join(" ")],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn remove(&mut self, uuid: &str) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM entries WHERE uuid = ?1", params![uuid])?;
        tx.execute("DELETE FROM entry_tags WHERE uuid = ?1", params![uuid])?;
        tx.execute("DELETE FROM entries_fts WHERE uuid = ?1", params![uuid])?;
        tx.commit()?;
        Ok(())
    }

    pub fn clear(&mut self) -> Result<()> {
        self.conn
            .execute_batch("DELETE FROM entries; DELETE FROM entry_tags; DELETE FROM entries_fts;")?;
        Ok(())
    }

    pub fn path_of(&self, uuid: &str) -> Result<Option<PathBuf>> {
        let p: Option<String> = self
            .conn
            .query_row("SELECT path FROM entries WHERE uuid = ?1", params![uuid], |r| r.get(0))
            .optional()?;
        Ok(p.map(PathBuf::from))
    }

    /// Resolve a full uuid or a unique prefix (dashes optional) to a uuid.
    pub fn resolve(&self, id: &str) -> Result<String> {
        if let Some(p) = self.path_of(id)? {
            let _ = p;
            return Ok(id.to_string());
        }
        let needle = id.replace('-', "").to_lowercase();
        if needle.is_empty() {
            return Err(Error::NotFound(id.to_string()));
        }
        let mut stmt = self
            .conn
            .prepare("SELECT uuid FROM entries WHERE replace(lower(uuid), '-', '') LIKE ?1 || '%'")?;
        let matches: Vec<String> = stmt
            .query_map(params![needle], |r| r.get(0))?
            .collect::<std::result::Result<_, _>>()?;
        match matches.len() {
            0 => Err(Error::NotFound(id.to_string())),
            1 => Ok(matches.into_iter().next().unwrap()),
            n => Err(Error::Ambiguous(id.to_string(), n)),
        }
    }

    pub fn list(&self, f: &ListFilter) -> Result<Vec<EntrySummary>> {
        let mut sql = String::from("SELECT uuid, path, created, date, journal, starred, tags, title FROM entries e WHERE 1=1");
        let mut args: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        if let Some(j) = &f.journal {
            sql.push_str(" AND journal = ?");
            args.push(Box::new(j.clone()));
        }
        if let Some(t) = &f.tag {
            sql.push_str(" AND EXISTS (SELECT 1 FROM entry_tags t WHERE t.uuid = e.uuid AND t.tag = ?)");
            args.push(Box::new(t.clone()));
        }
        if let Some(d) = f.from {
            sql.push_str(" AND date >= ?");
            args.push(Box::new(d.to_string()));
        }
        if let Some(d) = f.to {
            sql.push_str(" AND date <= ?");
            args.push(Box::new(d.to_string()));
        }
        if let Some(s) = f.starred {
            sql.push_str(" AND starred = ?");
            args.push(Box::new(s as i64));
        }
        sql.push_str(if f.ascending {
            " ORDER BY created_utc ASC"
        } else {
            " ORDER BY created_utc DESC"
        });
        if let Some(l) = f.limit {
            sql.push_str(" LIMIT ? OFFSET ?");
            args.push(Box::new(l as i64));
            args.push(Box::new(f.offset as i64));
        }
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(args.iter().map(|a| a.as_ref())), row_to_summary)?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn count(&self, f: &ListFilter) -> Result<usize> {
        let f = ListFilter { limit: None, offset: 0, ..f.clone() };
        Ok(self.list(&f)?.len())
    }

    /// Full-text search. Query uses FTS5 syntax; plain words are ANDed.
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<EntrySummary>> {
        let q = sanitize_fts(query);
        if q.is_empty() {
            return Ok(Vec::new());
        }
        let mut stmt = self.conn.prepare(
            "SELECT e.uuid, e.path, e.created, e.date, e.journal, e.starred, e.tags, e.title
             FROM entries_fts f JOIN entries e ON e.uuid = f.uuid
             WHERE entries_fts MATCH ?1
             ORDER BY bm25(entries_fts, 0.0, 2.0, 1.0, 1.5), e.created_utc DESC
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![q, limit as i64], row_to_summary)?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// Entries whose local date shares month/day with `date` but a different year.
    pub fn on_this_day(&self, date: NaiveDate) -> Result<Vec<EntrySummary>> {
        let mut stmt = self.conn.prepare(
            "SELECT uuid, path, created, date, journal, starred, tags, title FROM entries
             WHERE month = ?1 AND day = ?2 AND year != ?3 ORDER BY created_utc DESC",
        )?;
        let rows = stmt.query_map(params![date.month(), date.day(), date.year()], row_to_summary)?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn tags(&self) -> Result<Vec<(String, usize)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT tag, COUNT(*) FROM entry_tags GROUP BY tag ORDER BY COUNT(*) DESC, tag")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? as usize)))?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn journals(&self) -> Result<Vec<(String, usize)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT journal, COUNT(*) FROM entries GROUP BY journal ORDER BY journal")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? as usize)))?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// Distinct local dates (newest first), total words, total entries.
    pub fn dates_and_words(&self) -> Result<(Vec<NaiveDate>, usize, usize)> {
        let mut stmt = self.conn.prepare("SELECT DISTINCT date FROM entries ORDER BY date DESC")?;
        let dates: Vec<NaiveDate> = stmt
            .query_map([], |r| r.get::<_, String>(0))?
            .filter_map(|r| r.ok())
            .filter_map(|d| NaiveDate::parse_from_str(&d, "%Y-%m-%d").ok())
            .collect();
        let (words, total): (i64, i64) = self
            .conn
            .query_row("SELECT COALESCE(SUM(words), 0), COUNT(*) FROM entries", [], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok((dates, words as usize, total as usize))
    }

    /// Entries whose local date starts with `prefix` (YYYY or YYYY-MM).
    pub fn count_prefix(&self, prefix: &str) -> Result<usize> {
        let n: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM entries WHERE date LIKE ?1 || '%'",
            params![prefix],
            |r| r.get(0),
        )?;
        Ok(n as usize)
    }

    /// path -> (uuid, mtime) for every indexed entry.
    pub fn known_files(&self) -> Result<HashMap<String, (String, i64)>> {
        let mut stmt = self.conn.prepare("SELECT path, uuid, mtime FROM entries")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, (r.get::<_, String>(1)?, r.get::<_, i64>(2)?))))?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }
}

fn rel(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/")
}

fn row_to_summary(r: &Row<'_>) -> rusqlite::Result<EntrySummary> {
    let created: String = r.get(2)?;
    let created = DateTime::<FixedOffset>::parse_from_rfc3339(&created)
        .map_err(|e| rusqlite::Error::FromSqlConversionFailure(2, rusqlite::types::Type::Text, Box::new(e)))?;
    let date: String = r.get(3)?;
    let date = NaiveDate::parse_from_str(&date, "%Y-%m-%d")
        .map_err(|e| rusqlite::Error::FromSqlConversionFailure(3, rusqlite::types::Type::Text, Box::new(e)))?;
    let tags: String = r.get(6)?;
    let tags: Vec<String> = serde_json::from_str(&tags).unwrap_or_default();
    Ok(EntrySummary {
        uuid: r.get(0)?,
        path: PathBuf::from(r.get::<_, String>(1)?),
        created,
        date,
        journal: r.get(4)?,
        starred: r.get::<_, i64>(5)? != 0,
        tags,
        title: r.get(7)?,
    })
}

/// Turn free text into a safe FTS5 query: each token quoted, joined with AND.
/// Tokens ending in `*` keep prefix matching.
fn sanitize_fts(q: &str) -> String {
    q.split_whitespace()
        .filter_map(|t| {
            let prefix = t.ends_with('*');
            let core = t.trim_end_matches('*').replace('"', "\"\"");
            if core.is_empty() {
                None
            } else if prefix {
                Some(format!("\"{core}\"*"))
            } else {
                Some(format!("\"{core}\""))
            }
        })
        .collect::<Vec<_>>()
        .join(" AND ")
}
