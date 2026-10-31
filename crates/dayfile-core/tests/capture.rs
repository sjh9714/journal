use chrono::NaiveDate;
use dayfile_core::{CaptureConfig, ListFilter, Store};
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

const DAY: (i32, u32, u32) = (2026, 10, 28);

fn day() -> NaiveDate {
    NaiveDate::from_ymd_opt(DAY.0, DAY.1, DAY.2).unwrap()
}

fn git(repo: &Path, args: &[&str], when: &str) {
    git_as(repo, "me@example.com", args, when);
}

fn git_as(repo: &Path, email: &str, args: &[&str], when: &str) {
    let st = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["-c", "user.name=Someone", "-c", &format!("user.email={email}"), "-c", "commit.gpgsign=false"])
        .args(args)
        .env("GIT_AUTHOR_DATE", when)
        .env("GIT_COMMITTER_DATE", when)
        .status()
        .unwrap();
    assert!(st.success());
}

fn commit(repo: &Path, msg: &str, when: &str) {
    commit_as(repo, "me@example.com", msg, when);
}

fn commit_as(repo: &Path, email: &str, msg: &str, when: &str) {
    std::fs::write(repo.join("f.txt"), msg).unwrap();
    git_as(repo, email, &["add", "."], when);
    git_as(repo, email, &["commit", "-q", "-m", msg], when);
}

/// Smallest JPEG carrying an EXIF DateTimeOriginal.
fn jpeg_taken_at(path: &Path, when: &str) {
    let mut tiff = b"II*\0\x08\0\0\0".to_vec();
    // IFD0: one entry, ExifIFD pointer (0x8769, LONG) -> offset 26.
    tiff.extend([1, 0, 0x69, 0x87, 4, 0, 1, 0, 0, 0, 26, 0, 0, 0, 0, 0, 0, 0]);
    // Exif IFD: DateTimeOriginal (0x9003, ASCII, 20 bytes at offset 44).
    tiff.extend([1, 0, 0x03, 0x90, 2, 0, 20, 0, 0, 0, 44, 0, 0, 0, 0, 0, 0, 0]);
    tiff.extend(when.as_bytes());
    tiff.push(0);
    let mut jpg = vec![0xFF, 0xD8, 0xFF, 0xE1];
    let len = (2 + 6 + tiff.len()) as u16;
    jpg.extend(len.to_be_bytes());
    jpg.extend(b"Exif\0\0");
    jpg.extend(&tiff);
    jpg.extend([0xFF, 0xD9]);
    std::fs::write(path, jpg).unwrap();
}

#[test]
fn capture_drafts_the_day_once_and_accept_keeps_it() {
    std::env::set_var("TZ", "Asia/Seoul");
    let dir = TempDir::new().unwrap();
    std::env::set_var("DAYFILE_INDEX_DIR", dir.path().join("index"));
    let mut store = Store::init(dir.path().join("Journal")).unwrap();

    let code = dir.path().join("code");
    let repo = code.join("shop-api");
    std::fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "-q"], "2026-10-28T09:00:00+09:00");
    commit(&repo, "Add order endpoint", "2026-10-28T10:12:00+09:00");
    commit(&repo, "Fix refund rounding", "2026-10-28T16:40:00+09:00");
    commit(&repo, "Yesterday's work", "2026-10-27T23:00:00+09:00");
    // Someone whose email merely contains ours stays out.
    commit_as(&repo, "notme@example.com.evil", "Their change", "2026-10-28T12:00:00+09:00");

    let ics = dir.path().join("cal.ics");
    std::fs::write(&ics, "BEGIN:VCALENDAR\nBEGIN:VEVENT\nUID:s1\nSUMMARY:Standup\nDTSTART;TZID=Asia/Seoul:20261028T093000\nDTEND;TZID=Asia/Seoul:20261028T094500\nEND:VEVENT\nBEGIN:VEVENT\nUID:s2\nSUMMARY:![x](https://evil.example/p.png) [Call](https://evil.example) www.evil.example\nDTSTART;TZID=Asia/Seoul:20261028T200000\nEND:VEVENT\nEND:VCALENDAR\n").unwrap();

    let photos = dir.path().join("photos");
    std::fs::create_dir_all(&photos).unwrap();
    jpeg_taken_at(&photos.join("IMG_1.jpg"), "2026:10:28 18:20:00");
    jpeg_taken_at(&photos.join("IMG_2.jpg"), "2026:10:29 08:00:00");

    let cfg = CaptureConfig {
        git: vec![code.clone()],
        git_author: Some("me@example.com".into()),
        calendars: vec![ics.to_string_lossy().into_owned()],
        photos: vec![photos.clone()],
    };

    let r = store.capture(day(), &cfg).unwrap();
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    assert_eq!((r.commits, r.events, r.photos, r.created), (2, 2, 1, true));
    assert_eq!(r.describe(), "2 commits, 2 events, 1 photo");
    let draft = r.draft.unwrap();
    assert!(draft.draft);
    assert_eq!(draft.title, "Standup +1, 2 commits to shop-api, 1 photo");
    let e = store.get(&draft.uuid).unwrap();
    assert!(e.body.contains("- 09:30–09:45 · Standup"), "{}", e.body);
    assert!(e.body.contains("- 10:12–16:40 · shop-api · 2 commits (Add order endpoint; Fix refund rounding)"), "{}", e.body);
    assert!(e.body.contains("- 18:20 · 1 photo"), "{}", e.body);
    assert!(e.body.contains("- 20:00 · x Call\n"), "markdown and links stripped: {}", e.body);
    assert!(!e.body.contains("evil") && !e.body.contains("http"), "{}", e.body);
    assert!(!e.body.contains("Their change"), "{}", e.body);
    assert!(e.body.contains("](../../attachments/"), "{}", e.body);
    assert_eq!(e.meta.traces.len(), 5);
    assert_eq!(e.local_date(), day());

    // Drafts stay out of the timeline, search and stats until accepted.
    assert_eq!(store.count(&ListFilter::default()).unwrap(), 0);
    assert!(store.search("refund", 10).unwrap().is_empty());
    assert_eq!(store.drafts().unwrap().len(), 1);

    // Same day again: nothing new.
    let again = store.capture(day(), &cfg).unwrap();
    assert_eq!((again.added, again.created), (0, false));

    // A later commit lands in the same draft.
    commit(&repo, "Ship it", "2026-10-28T21:05:00+09:00");
    let more = store.capture(day(), &cfg).unwrap();
    assert_eq!((more.added, more.created), (1, false));
    let e = store.get(&draft.uuid).unwrap();
    assert!(e.body.contains("- 21:05 · shop-api · 1 commit (Ship it)"), "{}", e.body);

    // Accepted: an ordinary entry, and its traces are never captured again.
    store.accept_draft(&draft.uuid).unwrap();
    assert_eq!(store.count(&ListFilter::default()).unwrap(), 1);
    assert_eq!(store.search("refund", 10).unwrap().len(), 1);
    let after = store.capture(day(), &cfg).unwrap();
    assert_eq!((after.added, after.draft.is_none()), (0, true));

    // Rebuilding the index from files keeps all of that.
    store.reindex(true).unwrap();
    assert_eq!(store.capture(day(), &cfg).unwrap().added, 0);
}

#[test]
fn a_failing_source_does_not_stop_the_others() {
    std::env::set_var("TZ", "Asia/Seoul");
    let dir = TempDir::new().unwrap();
    std::env::set_var("DAYFILE_INDEX_DIR", dir.path().join("index"));
    let mut store = Store::init(dir.path().join("Journal")).unwrap();
    let ics = dir.path().join("cal.ics");
    std::fs::write(&ics, "BEGIN:VCALENDAR\nBEGIN:VEVENT\nUID:a\nSUMMARY:Dentist\nDTSTART;VALUE=DATE:20261028\nEND:VEVENT\nEND:VCALENDAR\n").unwrap();
    let cfg = CaptureConfig {
        git: vec![dir.path().join("missing")],
        calendars: vec![ics.to_string_lossy().into_owned(), "https://calendar.invalid/private-token/basic.ics".into()],
        ..Default::default()
    };
    let r = store.capture(day(), &cfg).unwrap();
    assert_eq!(r.describe(), "1 event");
    assert_eq!(r.errors.len(), 2, "{:?}", r.errors);
    assert!(r.errors.iter().all(|e| !e.contains("private-token")), "secret URL path leaked: {:?}", r.errors);
    let e = store.get(&r.draft.unwrap().uuid).unwrap();
    assert!(e.body.starts_with("Dentist\n\n- All day · Dentist"), "{}", e.body);
}
