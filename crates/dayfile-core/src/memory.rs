//! Memory: facts about the user's life that agents record from entries, kept
//! as plain files the user can read and correct (`memory/<topic>.md`).
//!
//! Facts are bi-temporal. Valid time says when something was true in the
//! world (`valid_from`..`valid_to`); transaction time says when the journal
//! learned it (`recorded_at`), when a newer fact closed it (`closed_at`) and
//! when it was found wrong (`retracted_at`). Facts are never deleted, so
//! "where did I live in 2023?" and "what did the journal believe last
//! month?" both have answers. The SQLite `facts` table is rebuilt from the
//! files like the rest of the index.

use crate::entry::{now_in, parse_tz, system_timezone};
use crate::{Error, Result, Store};
use chrono::{DateTime, FixedOffset, NaiveDate};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const MEMORY_DIR: &str = "memory";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Fact {
    pub id: String,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valid_from: Option<NaiveDate>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valid_to: Option<NaiveDate>,
    pub recorded_at: DateTime<FixedOffset>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub closed_at: Option<DateTime<FixedOffset>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retracted_at: Option<DateTime<FixedOffset>>,
    /// uuid of the entry the fact came from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// Ids of the facts this one closed when it was recorded.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub replaces: Vec<String>,
}

/// One `memory/<slug>.md` file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopicFile {
    pub topic: String,
    #[serde(default)]
    pub facts: Vec<Fact>,
}

/// A fact with its topic, as queries return it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TopicFact {
    pub topic: String,
    #[serde(flatten)]
    pub fact: Fact,
}

#[derive(Debug, Clone, Default)]
pub struct Recall {
    pub topic: Option<String>,
    /// Case-insensitive substring of the fact text.
    pub query: Option<String>,
    /// World date the facts must hold on. Defaults to today.
    pub as_of: Option<NaiveDate>,
    /// Answer with what the journal knew at this moment. Defaults to now.
    pub known_at: Option<DateTime<FixedOffset>>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct NewFact {
    pub topic: String,
    pub text: String,
    pub valid_from: Option<NaiveDate>,
    pub source: Option<String>,
    /// Facts this one supersedes: they end where this one starts.
    #[serde(default)]
    pub replaces: Vec<String>,
}

/// `Home`, `Mom's health`, `회사` -> `home`, `mom-s-health`, `회사`.
pub fn slug(topic: &str) -> String {
    let s: String = topic.trim().to_lowercase().chars().map(|c| if c.is_alphanumeric() { c } else { '-' }).collect();
    let s = s.split('-').filter(|p| !p.is_empty()).collect::<Vec<_>>().join("-");
    if s.is_empty() { "misc".into() } else { s }
}

impl Store {
    fn topic_path(&self, topic: &str) -> PathBuf {
        self.root().join(MEMORY_DIR).join(format!("{}.md", slug(topic)))
    }

    fn read_topic(&self, path: &Path) -> Result<TopicFile> {
        let text = std::fs::read_to_string(path)?;
        let yaml = frontmatter_yaml(&text).ok_or_else(|| Error::Frontmatter { path: path.to_path_buf(), msg: "missing --- block".into() })?;
        serde_norway::from_str(yaml).map_err(|e| Error::Frontmatter { path: path.to_path_buf(), msg: e.to_string() })
    }

    fn write_topic(&mut self, file: &TopicFile) -> Result<()> {
        let path = self.topic_path(&file.topic);
        std::fs::create_dir_all(path.parent().unwrap())?;
        let yaml = serde_norway::to_string(file)?;
        let text = format!("---\n{yaml}---\n\nFacts recorded from journal entries. Edit freely; keep the block above valid YAML.\n");
        let tmp = path.with_extension("md.tmp");
        std::fs::write(&tmp, text)?;
        std::fs::rename(&tmp, &path)?;
        self.reindex_memory()
    }

    /// Record a fact. Facts listed in `replaces` (any topic) are closed:
    /// their validity ends where the new one starts.
    pub fn remember(&mut self, new: NewFact) -> Result<TopicFact> {
        let tz = parse_tz(&system_timezone()).unwrap_or(chrono_tz::UTC);
        let now = now_in(&tz);
        let text = new.text.trim().to_string();
        if text.is_empty() || new.topic.trim().is_empty() {
            return Err(Error::Invalid("a fact needs a topic and text".into()));
        }
        let ends = new.valid_from.unwrap_or_else(crate::entry::today);
        for id in &new.replaces {
            let mut file = self.find_fact(id)?;
            let f = file.facts.iter_mut().find(|f| &f.id == id).unwrap();
            if f.closed_at.is_none() {
                f.valid_to = Some(ends);
                f.closed_at = Some(now);
            }
            self.write_topic(&file)?;
        }
        let path = self.topic_path(&new.topic);
        let mut file = if path.exists() { self.read_topic(&path)? } else { TopicFile { topic: new.topic.trim().to_string(), facts: Vec::new() } };
        let fact = Fact {
            id: format!("f{}", &uuid::Uuid::new_v4().simple().to_string()[..8]),
            text,
            valid_from: new.valid_from,
            valid_to: None,
            recorded_at: now,
            closed_at: None,
            retracted_at: None,
            source: new.source,
            replaces: new.replaces,
        };
        file.facts.push(fact.clone());
        self.write_topic(&file)?;
        Ok(TopicFact { topic: file.topic, fact })
    }

    /// Mark a fact as never true. It stays in the file for history.
    pub fn retract(&mut self, id: &str) -> Result<TopicFact> {
        let tz = parse_tz(&system_timezone()).unwrap_or(chrono_tz::UTC);
        let mut file = self.find_fact(id)?;
        let f = file.facts.iter_mut().find(|f| f.id == id).unwrap();
        f.retracted_at.get_or_insert_with(|| now_in(&tz));
        let out = TopicFact { topic: file.topic.clone(), fact: f.clone() };
        self.write_topic(&file)?;
        Ok(out)
    }

    fn find_fact(&self, id: &str) -> Result<TopicFile> {
        let topic = self.index_ref().fact_topic(id)?.ok_or_else(|| Error::NotFound(id.to_string()))?;
        self.read_topic(&self.topic_path(&topic))
    }

    /// Facts that hold on `as_of`, as the journal knew them at `known_at`.
    pub fn recall(&self, q: &Recall) -> Result<Vec<TopicFact>> {
        let tz = parse_tz(&system_timezone()).unwrap_or(chrono_tz::UTC);
        let as_of = q.as_of.unwrap_or_else(crate::entry::today);
        let known_at = q.known_at.unwrap_or_else(|| now_in(&tz));
        self.index_ref().recall(q.topic.as_deref().map(slug).as_deref(), q.query.as_deref(), as_of, known_at)
    }

    /// Every fact of a topic, including closed and retracted ones, oldest first.
    pub fn fact_history(&self, topic: &str) -> Result<Vec<TopicFact>> {
        let path = self.topic_path(topic);
        if !path.exists() {
            return Ok(Vec::new());
        }
        let file = self.read_topic(&path)?;
        let mut facts: Vec<TopicFact> = file.facts.into_iter().map(|fact| TopicFact { topic: file.topic.clone(), fact }).collect();
        facts.sort_by_key(|f| (f.fact.valid_from, f.fact.recorded_at));
        Ok(facts)
    }

    /// Topics with how many facts currently hold.
    pub fn topics(&self) -> Result<Vec<(String, usize)>> {
        self.index_ref().topics(crate::entry::today())
    }

    /// Reload every memory file into the index. Broken files are reported
    /// with the other problems and left alone.
    pub(crate) fn reindex_memory(&mut self) -> Result<()> {
        let dir = self.root().join(MEMORY_DIR);
        let mut facts = Vec::new();
        let mut problems = Vec::new();
        if dir.is_dir() {
            let mut paths: Vec<PathBuf> = std::fs::read_dir(&dir)?.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().and_then(|e| e.to_str()) == Some("md")).collect();
            paths.sort();
            for p in paths {
                match self.read_topic(&p) {
                    Ok(file) => facts.extend(file.facts.into_iter().map(|fact| (slug(&file.topic), file.topic.clone(), fact))),
                    Err(e) => problems.push(crate::store::Problem {
                        path: format!("{MEMORY_DIR}/{}", p.file_name().unwrap().to_string_lossy()),
                        reason: match e {
                            Error::Frontmatter { msg, .. } => msg,
                            other => other.to_string(),
                        },
                    }),
                }
            }
        }
        self.index_mut().replace_facts(&facts)?;
        self.set_memory_problems(problems);
        Ok(())
    }
}

fn frontmatter_yaml(text: &str) -> Option<&str> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let rest = text.strip_prefix("---\n").or_else(|| text.strip_prefix("---\r\n"))?;
    let end = rest.find("\n---").map(|i| i + 1)?;
    Some(&rest[..end])
}
