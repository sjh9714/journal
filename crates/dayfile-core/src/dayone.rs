//! Import a Day One JSON export (zip or extracted folder).
//!
//! Layout Day One produces: `<Journal>.json` per journal plus `photos/`,
//! `videos/`, `audios/`, `pdfs/` folders with files named by md5.
//! Media are referenced from entry text as `![](dayone-moment://<id>)`
//! (photos) or `![](dayone-moment:/video/<id>)` etc.

use crate::entry::{parse_tz, Location, NewEntry, Weather};
use crate::store::Store;
use crate::{Error, Result};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default)]
pub struct ImportOptions {
    /// Put every entry into this journal instead of one per JSON file.
    pub journal: Option<String>,
    /// Parse and report without writing anything.
    pub dry_run: bool,
}

#[derive(Debug, Default, Clone, serde::Serialize)]
pub struct ImportReport {
    pub files: Vec<String>,
    pub entries: usize,
    pub created: usize,
    pub already_present: usize,
    pub attachments: usize,
    pub missing_media: usize,
    pub journals: Vec<String>,
}

#[derive(Deserialize)]
struct Export {
    #[serde(default)]
    entries: Vec<RawEntry>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawEntry {
    uuid: String,
    creation_date: DateTime<Utc>,
    #[serde(default)]
    time_zone: Option<String>,
    #[serde(default)]
    text: String,
    #[serde(default)]
    starred: bool,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    location: Option<RawLocation>,
    #[serde(default)]
    weather: Option<RawWeather>,
    #[serde(default)]
    photos: Vec<RawMedia>,
    #[serde(default)]
    videos: Vec<RawMedia>,
    #[serde(default)]
    audios: Vec<RawMedia>,
    #[serde(default)]
    pdf_attachments: Vec<RawMedia>,
    /// Everything else, preserved verbatim under `extra.dayone`.
    #[serde(flatten)]
    rest: serde_json::Map<String, serde_json::Value>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawLocation {
    latitude: Option<f64>,
    longitude: Option<f64>,
    place_name: Option<String>,
    locality_name: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawWeather {
    temperature_celsius: Option<f64>,
    conditions_description: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawMedia {
    identifier: String,
    md5: Option<String>,
    #[serde(rename = "type")]
    kind: Option<String>,
}

pub fn import_dayone(store: &mut Store, source: &Path, opts: &ImportOptions) -> Result<ImportReport> {
    let _tmp;
    let dir: PathBuf = if source.is_file() {
        let t = tempfile::tempdir()?;
        extract_zip(source, t.path())?;
        let p = t.path().to_path_buf();
        _tmp = Some(t);
        p
    } else if source.is_dir() {
        _tmp = None;
        source.to_path_buf()
    } else {
        return Err(Error::Io(std::io::Error::new(std::io::ErrorKind::NotFound, source.display().to_string())));
    };
    let base = find_export_root(&dir).unwrap_or(dir);

    let mut json_files: Vec<PathBuf> = std::fs::read_dir(&base)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("json"))
        .collect();
    json_files.sort();
    if json_files.is_empty() {
        return Err(Error::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("no Day One .json files in {}", base.display()),
        )));
    }

    let mut report = ImportReport::default();
    for file in json_files {
        let journal_name = opts
            .journal
            .clone()
            .unwrap_or_else(|| file.file_stem().and_then(|s| s.to_str()).unwrap_or("default").to_string());
        let journal_name = sanitize_journal(&journal_name);
        report.files.push(file.file_name().unwrap_or_default().to_string_lossy().into_owned());
        if !report.journals.contains(&journal_name) {
            report.journals.push(journal_name.clone());
        }
        let text = std::fs::read_to_string(&file)?;
        let export: Export = serde_json::from_str(&text)?;
        for raw in export.entries {
            report.entries += 1;
            import_entry(store, &base, raw, &journal_name, opts, &mut report)?;
        }
    }
    Ok(report)
}

fn import_entry(
    store: &mut Store,
    base: &Path,
    raw: RawEntry,
    journal: &str,
    opts: &ImportOptions,
    report: &mut ImportReport,
) -> Result<()> {
    let uuid = canonical_uuid(&raw.uuid);
    if store.exists(&uuid)? {
        report.already_present += 1;
        return Ok(());
    }
    let tz_name = raw.time_zone.clone().filter(|t| parse_tz(t).is_ok()).unwrap_or_else(|| "UTC".into());
    let tz = parse_tz(&tz_name)?;
    let created = raw.creation_date.with_timezone(&tz).fixed_offset();

    // media id -> (folder, md5, type)
    let mut media: HashMap<String, (&'static str, String, Option<String>)> = HashMap::new();
    for (folder, list) in [("photos", &raw.photos), ("videos", &raw.videos), ("audios", &raw.audios), ("pdfs", &raw.pdf_attachments)] {
        for m in list {
            if let Some(md5) = &m.md5 {
                media.insert(m.identifier.to_lowercase(), (folder, md5.to_lowercase(), m.kind.clone()));
            }
        }
    }

    let mut body = raw.text.clone();
    for (id, (folder, md5, kind)) in &media {
        let needle_variants = [
            format!("dayone-moment://{}", id.to_uppercase()),
            format!("dayone-moment:/video/{}", id.to_uppercase()),
            format!("dayone-moment:/audio/{}", id.to_uppercase()),
            format!("dayone-moment:/pdfAttachment/{}", id.to_uppercase()),
            format!("dayone-moment://{}", id),
        ];
        if !needle_variants.iter().any(|n| body.contains(n.as_str())) {
            continue;
        }
        let replacement = match locate_media(base, folder, md5, kind.as_deref()) {
            Some(src) => {
                if opts.dry_run {
                    report.attachments += 1;
                    format!("../../attachments/{md5}")
                } else {
                    let rel = store.attach(&src)?;
                    report.attachments += 1;
                    Store::attachment_link(&rel)
                }
            }
            None => {
                report.missing_media += 1;
                format!("missing-media://{md5}")
            }
        };
        for n in &needle_variants {
            body = body.replace(n.as_str(), &replacement);
        }
    }

    let location = raw.location.and_then(|l| match (l.latitude, l.longitude) {
        (Some(latitude), Some(longitude)) => Some(Location {
            latitude,
            longitude,
            place_name: l.place_name.or(l.locality_name),
        }),
        _ => None,
    });
    let weather = raw.weather.map(|w| Weather { temperature_c: w.temperature_celsius, conditions: w.conditions_description });

    let mut rest = raw.rest;
    rest.remove("richText");
    let extra = if rest.is_empty() {
        None
    } else {
        Some(serde_json::json!({ "dayone": serde_json::Value::Object(rest) }))
    };

    if opts.dry_run {
        report.created += 1;
        return Ok(());
    }
    store.create(NewEntry {
        body,
        created: Some(created),
        timezone: Some(tz_name),
        journal: Some(journal.to_string()),
        tags: raw.tags,
        starred: raw.starred,
        location,
        weather,
        extra,
        uuid: Some(uuid),
        ..Default::default()
    })?;
    report.created += 1;
    Ok(())
}

/// Day One ids are 32 upper-case hex chars; make them canonical 8-4-4-4-12.
pub fn canonical_uuid(raw: &str) -> String {
    let hex: String = raw.chars().filter(|c| c.is_ascii_hexdigit()).collect::<String>().to_lowercase();
    if hex.len() != 32 {
        return raw.to_lowercase();
    }
    format!("{}-{}-{}-{}-{}", &hex[0..8], &hex[8..12], &hex[12..16], &hex[16..20], &hex[20..32])
}

fn sanitize_journal(name: &str) -> String {
    let n: String = name.chars().filter(|c| !matches!(c, '/' | '\\' | ':' | '\n')).collect();
    let n = n.trim();
    if n.is_empty() {
        "default".into()
    } else {
        n.to_string()
    }
}

/// Find `<folder>/<md5>.<ext>` trying the declared type first, then any extension.
fn locate_media(base: &Path, folder: &str, md5: &str, kind: Option<&str>) -> Option<PathBuf> {
    let dir = base.join(folder);
    if let Some(k) = kind {
        let p = dir.join(format!("{md5}.{k}"));
        if p.exists() {
            return Some(p);
        }
        // Day One writes jpeg as .jpeg but some exports use .jpg
        if k == "jpeg" {
            let p = dir.join(format!("{md5}.jpg"));
            if p.exists() {
                return Some(p);
            }
        }
    }
    std::fs::read_dir(&dir).ok()?.filter_map(|e| e.ok()).map(|e| e.path()).find(|p| {
        p.file_stem().and_then(|s| s.to_str()).map(|s| s.eq_ignore_ascii_case(md5)).unwrap_or(false)
    })
}

/// Exports sometimes nest everything one folder down inside the zip.
fn find_export_root(dir: &Path) -> Option<PathBuf> {
    let has_json = |d: &Path| {
        std::fs::read_dir(d)
            .ok()
            .map(|it| it.filter_map(|e| e.ok()).any(|e| e.path().extension().and_then(|x| x.to_str()) == Some("json")))
            .unwrap_or(false)
    };
    if has_json(dir) {
        return Some(dir.to_path_buf());
    }
    let subs: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_dir() && !p.file_name().map(|n| n.to_string_lossy().starts_with("__MACOSX")).unwrap_or(false))
        .collect();
    subs.into_iter().find(|s| has_json(s))
}

fn extract_zip(zip_path: &Path, dest: &Path) -> Result<()> {
    let file = std::fs::File::open(zip_path)?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| Error::Io(std::io::Error::other(e.to_string())))?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| Error::Io(std::io::Error::other(e.to_string())))?;
        let Some(rel) = entry.enclosed_name() else { continue };
        let out = dest.join(rel);
        if entry.is_dir() {
            std::fs::create_dir_all(&out)?;
            continue;
        }
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut f = std::fs::File::create(&out)?;
        std::io::copy(&mut entry, &mut f)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::canonical_uuid;

    #[test]
    fn uuid_canonicalization() {
        assert_eq!(
            canonical_uuid("6B7F8E1A2C3D4E5F6A7B8C9D0E1F2A3B"),
            "6b7f8e1a-2c3d-4e5f-6a7b-8c9d0e1f2a3b"
        );
        assert_eq!(canonical_uuid("not-a-uuid"), "not-a-uuid");
    }
}
