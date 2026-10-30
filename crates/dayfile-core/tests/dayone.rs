use dayfile_core::{import_dayone, ImportOptions, ListFilter, Store};
use std::path::PathBuf;
use tempfile::TempDir;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures").join(name)
}

fn tmp_store() -> (TempDir, Store) {
    let dir = TempDir::new().unwrap();
    std::env::set_var("DAYFILE_INDEX_DIR", dir.path().join("index"));
    let store = Store::init(dir.path().join("Journal")).unwrap();
    (dir, store)
}

#[test]
fn imports_folder_with_media_and_metadata() {
    let (_d, mut store) = tmp_store();
    let r = import_dayone(&mut store, &fixture("dayone-sample"), &ImportOptions::default()).unwrap();
    assert_eq!(r.entries, 4);
    assert_eq!(r.created, 4);
    assert_eq!(r.attachments, 1);
    assert_eq!(r.missing_media, 1);
    assert_eq!(r.journals, vec!["Journal", "Work"]);

    let e = store.get("6b7f8e1a").unwrap();
    assert_eq!(e.meta.uuid, "6b7f8e1a-2c3d-4e5f-6a7b-8c9d0e1f2a3b");
    assert_eq!(e.meta.timezone, "Asia/Seoul");
    assert_eq!(e.meta.created.to_rfc3339(), "2024-05-01T12:34:56+09:00");
    assert_eq!(e.local_date().to_string(), "2024-05-01");
    assert_eq!(e.meta.journal, "Journal");
    assert!(e.meta.starred);
    assert_eq!(e.meta.tags, vec!["산책", "운동"]);
    let loc = e.meta.location.as_ref().unwrap();
    assert_eq!(loc.place_name.as_deref(), Some("Han River Park"));
    assert_eq!(e.meta.weather.as_ref().unwrap().temperature_c, Some(18.5));
    assert!(e.body.contains("![](../../attachments/"), "moment link rewritten: {}", e.body);
    assert!(e.body.contains(".png)"));
    let link = e.body.split("](").nth(1).unwrap().split(')').next().unwrap();
    assert!(store.root().join("entries/2024").join(link).canonicalize().unwrap().exists());
    let extra = e.meta.extra.unwrap();
    assert_eq!(extra["dayone"]["creationDevice"], "iPhone");
    assert!(extra["dayone"].get("richText").is_none());

    // New York entry: 23:30Z on the 27th is 19:30 local on the 27th
    let ny = store.get("0f0e0d0c").unwrap();
    assert_eq!(ny.meta.created.to_rfc3339(), "2025-10-27T19:30:00-04:00");
    assert!(ny.path.to_string_lossy().contains("2025-10-27"));

    let missing = store.get("ffffffff").unwrap();
    assert!(missing.body.contains("missing-media://"));

    let work = store.list(&ListFilter { journal: Some("Work".into()), ..Default::default() }).unwrap();
    assert_eq!(work.len(), 1);
}

#[test]
fn import_is_idempotent_and_zip_matches_folder() {
    let (_d, mut store) = tmp_store();
    let r1 = import_dayone(&mut store, &fixture("dayone-sample.zip"), &ImportOptions::default()).unwrap();
    assert_eq!(r1.created, 4);
    let r2 = import_dayone(&mut store, &fixture("dayone-sample"), &ImportOptions::default()).unwrap();
    assert_eq!(r2.created, 0);
    assert_eq!(r2.already_present, 4);
    assert_eq!(store.count(&ListFilter::default()).unwrap(), 4);
    // one attachment file on disk, content addressed
    let n = std::fs::read_dir(store.root().join("attachments")).unwrap().count();
    assert_eq!(n, 1);
}

#[test]
fn dry_run_and_journal_override() {
    let (_d, mut store) = tmp_store();
    let r = import_dayone(
        &mut store,
        &fixture("dayone-sample"),
        &ImportOptions { journal: Some("imported".into()), dry_run: true },
    )
    .unwrap();
    assert_eq!(r.created, 4);
    assert_eq!(r.journals, vec!["imported"]);
    assert_eq!(store.count(&ListFilter::default()).unwrap(), 0);
    let r = import_dayone(&mut store, &fixture("dayone-sample"), &ImportOptions { journal: Some("imported".into()), dry_run: false }).unwrap();
    assert_eq!(r.created, 4);
    assert_eq!(store.journals().unwrap(), vec![("imported".to_string(), 4)]);
}
