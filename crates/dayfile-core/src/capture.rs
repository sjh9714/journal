//! Capture: turn the day's traces (git commits, calendar events, photos) into a
//! draft entry the user reviews. Nothing runs in the background; a capture
//! happens when the CLI, the app or an agent asks for one. Running it again
//! for the same day only adds traces no entry holds yet.

use crate::entry::{now_in, parse_tz, system_timezone, EntrySummary, NewEntry};
use crate::index::{Drafts, ListFilter};
use crate::store::Store;
use crate::Result;
use chrono::{DateTime, Duration, NaiveDate, NaiveDateTime, TimeZone};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use walkdir::WalkDir;

/// Where traces come from. Lives in the app's config.json under `capture`,
/// never in the journal folder: paths and calendar URLs are per device.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CaptureConfig {
    /// Folders to search (a few levels deep) for git repositories.
    #[serde(default)]
    pub git: Vec<PathBuf>,
    /// Only commits by this author email. Defaults to `git config --global user.email`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_author: Option<String>,
    /// `.ics` files, or http(s)/webcal URLs of calendar feeds.
    #[serde(default)]
    pub calendars: Vec<String>,
    /// Folders of photos; the EXIF capture time decides the day.
    #[serde(default)]
    pub photos: Vec<PathBuf>,
}

impl CaptureConfig {
    pub fn is_empty(&self) -> bool {
        self.git.is_empty() && self.calendars.is_empty() && self.photos.is_empty()
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct CaptureReport {
    pub date: NaiveDate,
    /// Traces new to the journal in this run.
    pub added: usize,
    pub commits: usize,
    pub events: usize,
    pub photos: usize,
    /// The day's draft after this run, if there is one.
    pub draft: Option<EntrySummary>,
    /// True when this run started the draft.
    pub created: bool,
    /// One line per source that failed; the other sources still ran.
    pub errors: Vec<String>,
}

/// Photos embedded per run; the rest are still counted in the summary.
const MAX_PHOTOS: usize = 12;

struct Commit {
    key: String,
    repo: String,
    at: DateTime<Tz>,
    subject: String,
}

struct Event {
    key: String,
    start: Option<DateTime<Tz>>,
    end: Option<DateTime<Tz>>,
    summary: String,
}

struct Photo {
    key: String,
    at: DateTime<Tz>,
    path: PathBuf,
}

impl Store {
    /// Gather `date`'s traces into its draft entry (created on first use).
    pub fn capture(&mut self, date: NaiveDate, cfg: &CaptureConfig) -> Result<CaptureReport> {
        let tz = parse_tz(&system_timezone()).unwrap_or(chrono_tz::UTC);
        let mut report = CaptureReport { date, ..Default::default() };

        let commits = git_commits(cfg, date, tz, &mut report.errors);
        let events = calendar_events(cfg, date, tz, &mut report.errors);
        let photos = photos_on(cfg, date, tz, &mut report.errors);

        let keys: Vec<String> = commits.iter().map(|c| c.key.clone()).chain(events.iter().map(|e| e.key.clone())).chain(photos.iter().map(|p| p.key.clone())).collect();
        let known = self.index_ref().known_traces(&keys)?;
        let commits: Vec<Commit> = commits.into_iter().filter(|c| !known.contains(&c.key)).collect();
        let events: Vec<Event> = events.into_iter().filter(|e| !known.contains(&e.key)).collect();
        let mut photos: Vec<Photo> = photos.into_iter().filter(|p| !known.contains(&p.key)).collect();
        photos.sort_by_key(|p| p.at);

        report.commits = commits.len();
        report.events = events.len();
        report.photos = photos.len();
        report.added = commits.len() + events.len() + photos.len();
        let existing = self.drafts_on(date)?.into_iter().next();
        if report.added == 0 {
            report.draft = existing;
            return Ok(report);
        }

        let mut images = Vec::new();
        for p in photos.iter().take(MAX_PHOTOS) {
            images.push(Store::attachment_link(&self.attach(&p.path)?));
        }
        let lines = render_lines(&commits, &events, &photos);
        let mut block = lines.join("\n");
        if !images.is_empty() {
            block.push_str("\n\n");
            block.push_str(&images.iter().map(|i| format!("![]({i})")).collect::<Vec<_>>().join("\n\n"));
        }
        let new_keys: Vec<String> = keys.into_iter().filter(|k| !known.contains(k)).collect();

        let entry = match existing {
            Some(d) => {
                let mut e = self.get(&d.uuid)?;
                e.meta.traces.extend(new_keys);
                e.body = format!("{}\n\n{block}\n", e.body.trim_end());
                self.write_entry(&e)?;
                e
            }
            None => {
                report.created = true;
                // Today's draft is stamped now; a past day's at 21:00 local.
                let created = if date == crate::entry::today() {
                    now_in(&tz)
                } else {
                    tz.from_local_datetime(&date.and_hms_opt(21, 0, 0).unwrap()).earliest().map(|d| d.fixed_offset()).unwrap_or_else(|| now_in(&tz))
                };
                let body = format!("{}\n\n{block}", summary_line(&commits, &events, photos.len()));
                self.create(NewEntry { body, created: Some(created), timezone: Some(tz.name().to_string()), draft: true, traces: new_keys, ..Default::default() })?
            }
        };
        report.draft = Some(entry.summary());
        Ok(report)
    }

    /// Drafts dated `date`, oldest first.
    pub fn drafts_on(&self, date: NaiveDate) -> Result<Vec<EntrySummary>> {
        self.list(&ListFilter { from: Some(date), to: Some(date), ascending: true, drafts: Drafts::Only, ..Default::default() })
    }

    /// Every unreviewed draft, newest first.
    pub fn drafts(&self) -> Result<Vec<EntrySummary>> {
        self.list(&ListFilter { drafts: Drafts::Only, ..Default::default() })
    }

    /// Keep a draft: it becomes an ordinary entry (its trace keys stay, so the
    /// same commits and photos are never captured again).
    pub fn accept_draft(&mut self, id: &str) -> Result<crate::Entry> {
        let mut e = self.get(id)?;
        e.meta.status = None;
        self.write_entry(&e)?;
        Ok(e)
    }
}

fn clip(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(max - 1).collect::<String>().trim_end())
    }
}

fn hm(t: DateTime<Tz>) -> String {
    t.format("%H:%M").to_string()
}

fn render_lines(commits: &[Commit], events: &[Event], photos: &[Photo]) -> Vec<String> {
    // (sort key, line); all-day events first.
    let mut lines: Vec<(i64, String)> = Vec::new();
    for e in events {
        let line = match (e.start, e.end) {
            (None, _) => format!("- All day · {}", e.summary),
            (Some(s), Some(end)) => format!("- {}–{} · {}", hm(s), hm(end), e.summary),
            (Some(s), None) => format!("- {} · {}", hm(s), e.summary),
        };
        lines.push((e.start.map(|s| s.timestamp()).unwrap_or(i64::MIN), line));
    }
    let mut by_repo: BTreeMap<&str, Vec<&Commit>> = BTreeMap::new();
    for c in commits {
        by_repo.entry(&c.repo).or_default().push(c);
    }
    for (repo, mut cs) in by_repo {
        cs.sort_by_key(|c| c.at);
        let (first, last) = (cs[0].at, cs[cs.len() - 1].at);
        let when = if first == last { hm(first) } else { format!("{}–{}", hm(first), hm(last)) };
        let shown: Vec<String> = cs.iter().take(3).map(|c| clip(&c.subject, 60)).collect();
        let more = if cs.len() > 3 { format!("; +{} more", cs.len() - 3) } else { String::new() };
        let n = if cs.len() == 1 { "1 commit".to_string() } else { format!("{} commits", cs.len()) };
        lines.push((first.timestamp(), format!("- {when} · {repo} · {n} ({}{more})", shown.join("; "))));
    }
    if let (Some(first), Some(last)) = (photos.first(), photos.last()) {
        let when = if photos.len() == 1 || hm(first.at) == hm(last.at) { hm(first.at) } else { format!("{}–{}", hm(first.at), hm(last.at)) };
        let n = if photos.len() == 1 { "1 photo".to_string() } else { format!("{} photos", photos.len()) };
        lines.push((first.at.timestamp(), format!("- {when} · {n}")));
    }
    lines.sort_by_key(|(t, _)| *t);
    lines.into_iter().map(|(_, l)| l).collect()
}

/// First line of a new draft, which also becomes its title.
fn summary_line(commits: &[Commit], events: &[Event], photos: usize) -> String {
    let mut parts = Vec::new();
    if let Some(e) = events.iter().find(|e| e.start.is_some()).or(events.first()) {
        parts.push(if events.len() > 1 { format!("{} and {} more", e.summary, events.len() - 1) } else { e.summary.clone() });
    }
    if !commits.is_empty() {
        let mut repos: Vec<&str> = commits.iter().map(|c| c.repo.as_str()).collect();
        repos.sort();
        repos.dedup();
        let n = if commits.len() == 1 { "1 commit".to_string() } else { format!("{} commits", commits.len()) };
        parts.push(match repos.len() {
            1 => format!("{n} to {}", repos[0]),
            k => format!("{n} across {k} repos"),
        });
    }
    if photos > 0 {
        parts.push(if photos == 1 { "1 photo".into() } else { format!("{photos} photos") });
    }
    parts.join(", ")
}

pub(crate) fn expand_home(p: &Path) -> PathBuf {
    match p.strip_prefix("~") {
        Ok(rest) => dirs::home_dir().map(|h| h.join(rest)).unwrap_or_else(|| p.to_path_buf()),
        Err(_) => p.to_path_buf(),
    }
}

/// Skip big generated trees and hidden folders while looking for repos or photos.
fn worth_descending(name: &str) -> bool {
    !(name.starts_with('.') || matches!(name, "node_modules" | "target" | "build" | "dist" | "vendor" | "Library"))
}

fn git_commits(cfg: &CaptureConfig, date: NaiveDate, tz: Tz, errors: &mut Vec<String>) -> Vec<Commit> {
    if cfg.git.is_empty() {
        return Vec::new();
    }
    let author = cfg.git_author.clone().or_else(|| {
        let out = Command::new("git").args(["config", "--global", "user.email"]).output().ok()?;
        Some(String::from_utf8_lossy(&out.stdout).trim().to_string()).filter(|s| !s.is_empty())
    });
    // Other people's commit messages stay out of the journal (and out of the
    // agents that read it): no known author, no git capture.
    let Some(author) = author else {
        errors.push("git: set an author email (`dayfile sources author <email>` or Settings) to capture commits".into());
        return Vec::new();
    };
    let mut repos: Vec<PathBuf> = Vec::new();
    let mut seen = HashSet::new();
    for root in &cfg.git {
        let root = expand_home(root);
        if !root.is_dir() {
            errors.push(format!("git: {} is not a folder", root.display()));
            continue;
        }
        let walker = WalkDir::new(&root).max_depth(4).into_iter().filter_entry(|e| e.depth() == 0 || worth_descending(&e.file_name().to_string_lossy()));
        for d in walker.filter_map(|e| e.ok()).filter(|e| e.file_type().is_dir()) {
            if d.path().join(".git").exists() {
                let canon = d.path().canonicalize().unwrap_or_else(|_| d.path().to_path_buf());
                if seen.insert(canon.clone()) {
                    repos.push(canon);
                }
            }
        }
    }
    // A day in the journal's zone, padded so committer-date filtering never drops an author date.
    let since = (date - Duration::days(1)).format("%Y-%m-%d").to_string();
    let until = (date + Duration::days(2)).format("%Y-%m-%d").to_string();
    let mut out = Vec::new();
    for repo in repos {
        let name = repo.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let mut cmd = Command::new("git");
        cmd.arg("-C").arg(&repo).args(["log", "--all", "--no-merges", "--format=%H%x1f%aI%x1f%ae%x1f%s"]).arg(format!("--since={since}")).arg(format!("--until={until}"));
        // --author is a regex substring match; it only narrows, the exact check is below.
        cmd.arg(format!("--author={author}"));
        let output = match cmd.output() {
            Ok(o) if o.status.success() => o,
            Ok(o) => {
                errors.push(format!("git {name}: {}", String::from_utf8_lossy(&o.stderr).trim()));
                continue;
            }
            Err(e) => {
                errors.push(format!("git: {e}"));
                break;
            }
        };
        for line in String::from_utf8_lossy(&output.stdout).lines() {
            let mut f = line.split('\x1f');
            let (Some(sha), Some(at), Some(email), Some(subject)) = (f.next(), f.next(), f.next(), f.next()) else { continue };
            if !email.eq_ignore_ascii_case(&author) {
                continue;
            }
            let Ok(at) = DateTime::parse_from_rfc3339(at) else { continue };
            let at = at.with_timezone(&tz);
            if at.date_naive() == date {
                out.push(Commit { key: format!("git:{sha}"), repo: name.clone(), at, subject: subject.to_string() });
            }
        }
    }
    out
}

fn calendar_events(cfg: &CaptureConfig, date: NaiveDate, tz: Tz, errors: &mut Vec<String>) -> Vec<Event> {
    let mut out = Vec::new();
    for src in &cfg.calendars {
        let text = match read_calendar(src) {
            Ok(t) => t,
            Err(e) => {
                errors.push(format!("calendar {}: {e}", source_label(src)));
                continue;
            }
        };
        for o in crate::ics::occurrences_on(&text, date, tz) {
            let when = o.start.map(|s| s.to_rfc3339()).unwrap_or_else(|| date.to_string());
            let plain = plain_words(&o.summary);
            let summary = if plain.is_empty() { "(untitled event)".into() } else { clip(&plain, 80) };
            out.push(Event { key: format!("cal:{}:{when}", o.uid), start: o.start, end: o.end, summary });
        }
    }
    out
}

/// Invites are written by other people: keep their words, drop anything that
/// could become a link, an embed or formatting once it is Markdown.
fn plain_words(s: &str) -> String {
    let spaced: String = s.chars().map(|c| if matches!(c, '[' | ']' | '(' | ')' | '<' | '>' | '`' | '*' | '_' | '#' | '!' | '|' | '~' | '\\') { ' ' } else { c }).collect();
    spaced.split_whitespace().filter(|w| !linkish(w)).collect::<Vec<_>>().join(" ")
}

/// Anything a linkifier could turn into a link: emails, `scheme:`, and
/// `name.tld` domains. Times (`10:30`) and versions (`v1.2`) stay.
fn linkish(w: &str) -> bool {
    if w.contains('@') {
        return true;
    }
    let colon = w.char_indices().any(|(i, c)| c == ':' && !w[i + 1..].starts_with(|n: char| n.is_ascii_digit()));
    let domain = w.split(['/', '?', '#']).next().unwrap_or(w).rsplit_once('.').is_some_and(|(host, tld)| {
        let tld = tld.trim_end_matches(|c: char| !c.is_alphanumeric());
        !host.is_empty() && host.chars().last().is_some_and(|c| c.is_alphanumeric()) && tld.len() >= 2 && tld.chars().all(|c| c.is_ascii_alphabetic())
    });
    colon || domain
}


fn read_calendar(src: &str) -> std::result::Result<String, String> {
    let url = src.strip_prefix("webcal://").map(|r| format!("https://{r}")).unwrap_or_else(|| src.to_string());
    if url.starts_with("http://") || url.starts_with("https://") {
        let agent: ureq::Agent = ureq::Agent::config_builder().timeout_global(Some(std::time::Duration::from_secs(20))).build().into();
        let mut res = agent.get(&url).call().map_err(|e| e.to_string())?;
        res.body_mut().with_config().limit(8 * 1024 * 1024).read_to_string().map_err(|e| e.to_string())
    } else {
        std::fs::read_to_string(expand_home(Path::new(src))).map_err(|e| e.to_string())
    }
}

/// Private calendar URLs carry a secret; errors show only the host.
fn source_label(src: &str) -> String {
    match src.split_once("://") {
        Some((_, rest)) => rest.split('/').next().unwrap_or(rest).to_string(),
        None => src.to_string(),
    }
}

fn photos_on(cfg: &CaptureConfig, date: NaiveDate, tz: Tz, errors: &mut Vec<String>) -> Vec<Photo> {
    let mut out = Vec::new();
    for dir in &cfg.photos {
        let dir = expand_home(dir);
        if !dir.is_dir() {
            errors.push(format!("photos: {} is not a folder", dir.display()));
            continue;
        }
        let walker = WalkDir::new(&dir).max_depth(3).into_iter().filter_entry(|e| e.depth() == 0 || worth_descending(&e.file_name().to_string_lossy()));
        for f in walker.filter_map(|e| e.ok()).filter(|e| e.file_type().is_file()).take(50_000) {
            let ext = f.path().extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).unwrap_or_default();
            if !matches!(ext.as_str(), "jpg" | "jpeg" | "heic" | "heif" | "tif" | "tiff") {
                continue;
            }
            let Some(at) = exif_time(f.path(), tz) else { continue };
            if at.date_naive() != date {
                continue;
            }
            let Ok(bytes) = std::fs::read(f.path()) else { continue };
            let hex: String = Sha256::digest(&bytes).iter().map(|b| format!("{b:02x}")).collect();
            out.push(Photo { key: format!("photo:{hex}"), at, path: f.path().to_path_buf() });
        }
    }
    // The same photo in two folders is one trace.
    let mut seen = HashSet::new();
    out.retain(|p| seen.insert(p.key.clone()));
    out
}

/// DateTimeOriginal, with OffsetTimeOriginal when the camera wrote one;
/// otherwise the wall time is taken to be in the journal's zone.
fn exif_time(path: &Path, tz: Tz) -> Option<DateTime<Tz>> {
    let file = std::fs::File::open(path).ok()?;
    let exif = exif::Reader::new().read_from_container(&mut std::io::BufReader::new(file)).ok()?;
    let ascii = |tag| match exif.get_field(tag, exif::In::PRIMARY).map(|f| &f.value) {
        Some(exif::Value::Ascii(v)) => v.first().map(|b| String::from_utf8_lossy(b).trim().to_string()),
        _ => None,
    };
    let when = ascii(exif::Tag::DateTimeOriginal).or_else(|| ascii(exif::Tag::DateTime))?;
    let naive = NaiveDateTime::parse_from_str(&when, "%Y:%m:%d %H:%M:%S").ok()?;
    if let Some(off) = ascii(exif::Tag::OffsetTimeOriginal) {
        if let Ok(d) = DateTime::parse_from_str(&format!("{when} {off}"), "%Y:%m:%d %H:%M:%S %:z") {
            return Some(d.with_timezone(&tz));
        }
    }
    tz.from_local_datetime(&naive).earliest()
}

#[cfg(test)]
mod tests {
    use super::plain_words;

    #[test]
    fn event_titles_lose_anything_link_like() {
        assert_eq!(plain_words("![x](https://evil.example/p.png) [Call](https://evil.example)"), "x Call");
        assert_eq!(plain_words("Sync with a@evil.com at evil.com/x or evil.co"), "Sync with at or");
        assert_eq!(plain_words("mailto:me javascript:alert(1) www.evil.example"), "1");
        assert_eq!(plain_words("Standup 10:30 v1.2 release U.S. trip 회의.준비"), "Standup 10:30 v1.2 release U.S. trip 회의.준비");
    }
}
