use chrono::{DateTime, FixedOffset, NaiveDate};
use journal_core::store::EntryPatch;
use journal_core::{ListFilter, NewEntry, Store};
use tempfile::TempDir;

fn tmp_store() -> (TempDir, Store) {
    let dir = TempDir::new().unwrap();
    std::env::set_var("JOURNAL_INDEX_DIR", dir.path().join("index"));
    let store = Store::init(dir.path().join("Journal")).unwrap();
    (dir, store)
}

fn at(s: &str) -> DateTime<FixedOffset> {
    DateTime::parse_from_rfc3339(s).unwrap()
}

#[test]
fn create_and_read_roundtrip() {
    let (_d, mut store) = tmp_store();
    let e = store
        .create(NewEntry {
            body: "# Hello\n\nFirst entry with **bold**.\n".into(),
            created: Some(at("2026-10-28T21:03:00+09:00")),
            timezone: Some("Asia/Seoul".into()),
            tags: vec!["#work".into(), "life".into(), "life".into()],
            ..Default::default()
        })
        .unwrap();
    assert_eq!(e.title(), "Hello");
    assert_eq!(e.meta.tags, vec!["life", "work"]);
    assert!(e.path.to_string_lossy().starts_with("entries/2026/2026-10-28-"));
    assert!(store.root().join(&e.path).exists());

    let back = store.get(&e.meta.uuid).unwrap();
    assert_eq!(back, e);
    // prefix lookup
    let by_prefix = store.get(&e.meta.uuid[..8]).unwrap();
    assert_eq!(by_prefix.meta.uuid, e.meta.uuid);
}

#[test]
fn file_is_plain_markdown_with_frontmatter() {
    let (_d, mut store) = tmp_store();
    let e = store
        .create(NewEntry { body: "Body text".into(), timezone: Some("UTC".into()), ..Default::default() })
        .unwrap();
    let text = std::fs::read_to_string(store.root().join(&e.path)).unwrap();
    assert!(text.starts_with("---\nuuid: "));
    assert!(text.contains("timezone: UTC"));
    assert!(text.ends_with("---\n\nBody text\n"));
}

#[test]
fn list_search_on_this_day() {
    let (_d, mut store) = tmp_store();
    for (i, (date, body, tag)) in [
        ("2024-10-28T09:00:00+09:00", "Ran 5km by the river", "fitness"),
        ("2025-10-28T09:00:00+09:00", "Interview went well, nervous but fine", "work"),
        ("2026-10-27T09:00:00+09:00", "Read a book about rivers", "reading"),
        ("2026-10-28T09:00:00+09:00", "Today is quiet", "life"),
    ]
    .iter()
    .enumerate()
    {
        store
            .create(NewEntry {
                body: body.to_string(),
                created: Some(at(date)),
                timezone: Some("Asia/Seoul".into()),
                tags: vec![tag.to_string()],
                journal: Some(if i % 2 == 0 { "default".into() } else { "work".into() }),
                ..Default::default()
            })
            .unwrap();
    }
    let all = store.list(&ListFilter::default()).unwrap();
    assert_eq!(all.len(), 4);
    assert_eq!(all[0].title, "Today is quiet"); // newest first

    let work = store.list(&ListFilter { journal: Some("work".into()), ..Default::default() }).unwrap();
    assert_eq!(work.len(), 2);

    let tagged = store.list(&ListFilter { tag: Some("fitness".into()), ..Default::default() }).unwrap();
    assert_eq!(tagged.len(), 1);

    let hits = store.search("river", 10).unwrap();
    assert_eq!(hits.len(), 1, "exact token match only");
    let hits = store.search("river*", 10).unwrap();
    assert_eq!(hits.len(), 2, "prefix match covers rivers");

    let otd = store.on_this_day(NaiveDate::from_ymd_opt(2026, 10, 28).unwrap()).unwrap();
    assert_eq!(otd.len(), 2);
    assert_eq!(otd[0].date, NaiveDate::from_ymd_opt(2025, 10, 28).unwrap());

    let today = store.on_date(NaiveDate::from_ymd_opt(2026, 10, 28).unwrap()).unwrap();
    assert_eq!(today.len(), 1);

    let tags = store.tags().unwrap();
    assert_eq!(tags.len(), 4);
}

#[test]
fn update_append_delete() {
    let (_d, mut store) = tmp_store();
    let e = store.create(NewEntry { body: "one".into(), timezone: Some("UTC".into()), ..Default::default() }).unwrap();
    let e = store.append(&e.meta.uuid, "two").unwrap();
    assert_eq!(e.body, "one\n\ntwo\n");
    let e = store
        .update(&e.meta.uuid, EntryPatch { starred: Some(true), tags: Some(vec!["x".into()]), ..Default::default() })
        .unwrap();
    assert!(e.meta.starred);
    let starred = store.list(&ListFilter { starred: Some(true), ..Default::default() }).unwrap();
    assert_eq!(starred.len(), 1);
    store.delete(&e.meta.uuid).unwrap();
    assert!(store.get(&e.meta.uuid).is_err());
    assert!(!store.root().join(&e.path).exists());
}

#[test]
fn reindex_picks_up_external_edits_and_deletes() {
    let (_d, mut store) = tmp_store();
    let e = store.create(NewEntry { body: "original".into(), timezone: Some("UTC".into()), ..Default::default() }).unwrap();
    let abs = store.root().join(&e.path);
    // Edit on disk as a sync tool or editor would.
    std::thread::sleep(std::time::Duration::from_millis(20));
    let text = std::fs::read_to_string(&abs).unwrap().replace("original", "edited elsewhere");
    std::fs::write(&abs, text).unwrap();
    let stats = store.reindex(false).unwrap();
    assert_eq!(stats.updated, 1);
    assert_eq!(store.search("elsewhere", 10).unwrap().len(), 1);

    std::fs::remove_file(&abs).unwrap();
    let stats = store.reindex(false).unwrap();
    assert_eq!(stats.removed, 1);
    assert_eq!(store.count(&ListFilter::default()).unwrap(), 0);

    // Reopen builds the same index from files only.
    std::fs::write(&abs, "---\nuuid: 11111111-2222-3333-4444-555555555555\ncreated: 2026-01-02T03:04:05+00:00\ntimezone: UTC\n---\n\nhand written\n").unwrap();
    let store2 = Store::open(store.root()).unwrap();
    assert_eq!(store2.count(&ListFilter::default()).unwrap(), 1);
    assert_eq!(store2.get("11111111").unwrap().body, "hand written\n");
}

#[test]
fn broken_file_is_skipped_not_fatal() {
    let (_d, mut store) = tmp_store();
    store.create(NewEntry { body: "fine".into(), timezone: Some("UTC".into()), ..Default::default() }).unwrap();
    std::fs::write(store.root().join("entries/broken.md"), "no frontmatter here").unwrap();
    let stats = store.reindex(true).unwrap();
    assert_eq!(stats.added, 1);
    assert_eq!(stats.skipped, 1);
    assert_eq!(store.count(&ListFilter::default()).unwrap(), 1);
}

#[test]
fn attach_is_content_addressed() {
    let (d, store) = tmp_store();
    let src = d.path().join("pic.PNG");
    std::fs::write(&src, b"\x89PNG fake").unwrap();
    let a = store.attach(&src).unwrap();
    let b = store.attach(&src).unwrap();
    assert_eq!(a, b);
    assert!(a.to_string_lossy().starts_with("attachments/"));
    assert!(a.to_string_lossy().ends_with(".png"));
    assert!(store.root().join(&a).exists());
}
