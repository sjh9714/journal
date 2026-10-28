use chrono::{DateTime, FixedOffset, NaiveDate, Timelike, Utc};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Location {
    pub latitude: f64,
    pub longitude: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub place_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Weather {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature_c: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conditions: Option<String>,
}

/// The YAML frontmatter stored at the top of every entry file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Frontmatter {
    pub uuid: String,
    /// RFC 3339 with the entry's local offset, e.g. 2026-10-28T21:03:00+09:00
    pub created: DateTime<FixedOffset>,
    /// IANA zone name, e.g. Asia/Seoul
    pub timezone: String,
    #[serde(default = "default_journal")]
    pub journal: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub starred: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<Location>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weather: Option<Weather>,
    /// Anything an importer wants to preserve verbatim (e.g. Day One fields).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extra: Option<serde_json::Value>,
}

fn default_journal() -> String {
    "default".to_string()
}
fn is_false(b: &bool) -> bool {
    !*b
}

/// A full entry: frontmatter + Markdown body + where it lives on disk.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Entry {
    #[serde(flatten)]
    pub meta: Frontmatter,
    pub body: String,
    /// Path relative to the journal root, e.g. entries/2026/2026-10-28-ab12cd34.md
    pub path: PathBuf,
}

impl Entry {
    /// First non-empty line of the body, with Markdown heading markers stripped.
    pub fn title(&self) -> String {
        title_of(&self.body)
    }

    /// Calendar date in the entry's own timezone.
    pub fn local_date(&self) -> NaiveDate {
        self.meta.created.date_naive()
    }

    pub fn created_utc(&self) -> DateTime<Utc> {
        self.meta.created.with_timezone(&Utc)
    }

    pub fn summary(&self) -> EntrySummary {
        EntrySummary {
            uuid: self.meta.uuid.clone(),
            created: self.meta.created,
            date: self.local_date(),
            journal: self.meta.journal.clone(),
            tags: self.meta.tags.clone(),
            starred: self.meta.starred,
            title: self.title(),
            path: self.path.clone(),
        }
    }
}

/// Lightweight row used for listings; no body.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EntrySummary {
    pub uuid: String,
    pub created: DateTime<FixedOffset>,
    pub date: NaiveDate,
    pub journal: String,
    pub tags: Vec<String>,
    pub starred: bool,
    pub title: String,
    pub path: PathBuf,
}

/// Input for creating an entry. Everything optional has a sensible default.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct NewEntry {
    pub body: String,
    /// Defaults to now in `timezone`.
    pub created: Option<DateTime<FixedOffset>>,
    /// Defaults to the system timezone, falling back to UTC.
    pub timezone: Option<String>,
    pub journal: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub starred: bool,
    pub location: Option<Location>,
    pub weather: Option<Weather>,
    pub extra: Option<serde_json::Value>,
    /// Importers may supply a stable uuid so re-imports are idempotent.
    pub uuid: Option<String>,
}

/// First line that reads as a title: Markdown markers stripped, image-only
/// and horizontal-rule lines skipped. Empty when nothing qualifies.
pub fn title_of(body: &str) -> String {
    for raw in body.lines() {
        let mut l = raw.trim();
        if l.is_empty() {
            continue;
        }
        // Images: `![alt](src)` -> keep alt text if any, else skip the line.
        if l.starts_with("![") {
            let alt = l[2..].split(']').next().unwrap_or("").trim();
            if alt.is_empty() {
                continue;
            }
            l = alt;
        }
        if l.chars().all(|c| matches!(c, '-' | '*' | '_' | ' ')) {
            continue; // horizontal rule
        }
        let l = l.trim_start_matches(['#', '>', '-', '*', '+', ' ']);
        // Ordered-list marker ("1. " / "12) ") only; never bare numbers like a date.
        let digits = l.chars().take_while(|c| c.is_ascii_digit()).count();
        let l = match l[digits..].chars().next() {
            Some('.' | ')') if digits > 0 && l[digits + 1..].starts_with(' ') => &l[digits + 2..],
            _ => l,
        };
        let l = l.trim();
        let l = l.trim_matches(['*', '_', '`']);
        if !l.is_empty() {
            return l.to_string();
        }
    }
    String::new()
}

#[cfg(test)]
mod tests {
    use super::title_of;

    #[test]
    fn titles() {
        assert_eq!(title_of("# Hello\n\nbody"), "Hello");
        assert_eq!(title_of("\n\n![](../../attachments/x.png)\n\nAfter the photo"), "After the photo");
        assert_eq!(title_of("![Beach day](x.png)"), "Beach day");
        assert_eq!(title_of("![](x.png)"), "");
        assert_eq!(title_of("- first item\n- second"), "first item");
        assert_eq!(title_of("1. first step"), "first step");
        assert_eq!(title_of("# 2026-10-29 감사한 것"), "2026-10-29 감사한 것");
        assert_eq!(title_of("2026 was a year"), "2026 was a year");
        assert_eq!(title_of("> quoted"), "quoted");
        assert_eq!(title_of("**bold start** rest"), "bold start** rest");
        assert_eq!(title_of("---\n\nreal"), "real");
    }
}

/// Resolve an IANA timezone name, validating it.
pub fn parse_tz(name: &str) -> crate::Result<Tz> {
    name.parse::<Tz>()
        .map_err(|_| crate::Error::Timezone(name.to_string()))
}

/// Best-effort system timezone name; UTC if unknown.
pub fn system_timezone() -> String {
    if let Ok(tz) = std::env::var("TZ") {
        if parse_tz(&tz).is_ok() {
            return tz;
        }
    }
    // macOS/Linux: /etc/localtime -> .../zoneinfo/Area/City
    if let Ok(target) = std::fs::read_link("/etc/localtime") {
        let s = target.to_string_lossy();
        if let Some(idx) = s.find("zoneinfo/") {
            let name = &s[idx + "zoneinfo/".len()..];
            if parse_tz(name).is_ok() {
                return name.to_string();
            }
        }
    }
    "UTC".to_string()
}

/// Today's calendar date in the journal's timezone (`TZ`, then the system
/// zone, then UTC). Use this instead of `chrono::Local`, which ignores `TZ`
/// on Windows and would disagree with the zone new entries are stamped with.
pub fn today() -> NaiveDate {
    let tz = parse_tz(&system_timezone()).unwrap_or(chrono_tz::UTC);
    now_in(&tz).date_naive()
}

/// Current time expressed in the given zone as a fixed-offset datetime.
pub fn now_in(tz: &Tz) -> DateTime<FixedOffset> {
    let now = Utc::now().with_timezone(tz);
    // Whole seconds keep files tidy and match what other journal apps store.
    now.fixed_offset().with_nanosecond(0).unwrap_or_else(|| now.fixed_offset())
}
