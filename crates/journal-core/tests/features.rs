use chrono::NaiveDate;
use journal_core::{ListFilter, NewEntry, Store};
use tempfile::TempDir;

fn tmp_store() -> (TempDir, Store) {
    let dir = TempDir::new().unwrap();
    std::env::set_var("JOURNAL_INDEX_DIR", dir.path().join("index"));
    let store = Store::init(dir.path().join("Journal")).unwrap();
    (dir, store)
}

#[test]
fn delete_moves_to_trash_and_restore_brings_it_back() {
    let (_d, mut store) = tmp_store();
    let e = store.create(NewEntry { body: "# Keep me\n\nbody".into(), timezone: Some("UTC".into()), tags: vec!["t".into()], ..Default::default() }).unwrap();
    let abs = store.root().join(&e.path);
    store.delete(&e.meta.uuid).unwrap();
    assert!(!abs.exists(), "file left the entries folder");
    assert!(store.get(&e.meta.uuid).is_err());
    assert_eq!(store.count(&ListFilter::default()).unwrap(), 0);

    let trash = store.trash_list().unwrap();
    assert_eq!(trash.len(), 1);
    assert_eq!(trash[0].uuid, e.meta.uuid);
    assert_eq!(trash[0].title, "Keep me");
    assert!(store.root().join(&trash[0].trash_path).exists());

    let back = store.restore(&e.meta.uuid[..8]).unwrap();
    assert_eq!(back.body, e.body);
    assert_eq!(back.meta.tags, e.meta.tags);
    assert!(abs.exists(), "restored to the original path");
    assert!(store.trash_list().unwrap().is_empty());
    assert_eq!(store.count(&ListFilter::default()).unwrap(), 1);
}

#[test]
fn trash_purge_and_empty_and_permanent() {
    let (_d, mut store) = tmp_store();
    let a = store.create(NewEntry { body: "old".into(), timezone: Some("UTC".into()), ..Default::default() }).unwrap();
    let b = store.create(NewEntry { body: "new".into(), timezone: Some("UTC".into()), ..Default::default() }).unwrap();
    let c = store.create(NewEntry { body: "gone".into(), timezone: Some("UTC".into()), ..Default::default() }).unwrap();
    store.delete(&a.meta.uuid).unwrap();
    store.delete(&b.meta.uuid).unwrap();
    // Backdate a's trash file name by 40 days.
    let dir = store.root().join(".journal/trash");
    let old = std::fs::read_dir(&dir).unwrap().filter_map(|e| e.ok()).find(|e| e.file_name().to_string_lossy().contains(&a.meta.uuid[..8])).unwrap();
    let name = old.file_name().to_string_lossy().into_owned();
    let (ts, rest) = name.split_once("__").unwrap();
    let ts: i64 = ts.parse().unwrap();
    std::fs::rename(old.path(), dir.join(format!("{}__{}", ts - 40 * 86_400, rest))).unwrap();

    assert_eq!(store.purge_trash(30).unwrap(), 1);
    let left = store.trash_list().unwrap();
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].uuid, b.meta.uuid);

    let c_abs = store.root().join(&c.path);
    store.delete_permanent(&c.meta.uuid).unwrap();
    assert!(!c_abs.exists());
    assert_eq!(store.trash_list().unwrap().len(), 1, "permanent delete skips the trash");

    assert_eq!(store.empty_trash().unwrap(), 1);
    assert!(store.trash_list().unwrap().is_empty());
}

#[test]
fn restore_never_overwrites_a_newer_file() {
    let (_d, mut store) = tmp_store();
    let e = store.create(NewEntry { body: "first".into(), timezone: Some("UTC".into()), ..Default::default() }).unwrap();
    store.delete(&e.meta.uuid).unwrap();
    // Something (sync, another device) writes the same path again.
    let abs = store.root().join(&e.path);
    std::fs::create_dir_all(abs.parent().unwrap()).unwrap();
    std::fs::write(&abs, format!("---\nuuid: {}\ncreated: {}\ntimezone: UTC\n---\n\nsecond\n", e.meta.uuid, e.meta.created.to_rfc3339())).unwrap();
    store.reindex(false).unwrap();
    store.restore(&e.meta.uuid).unwrap();
    let names: Vec<String> = std::fs::read_dir(abs.parent().unwrap()).unwrap().map(|x| x.unwrap().file_name().to_string_lossy().into_owned()).collect();
    assert_eq!(names.len(), 2, "{names:?}");
    assert!(names.iter().any(|n| n.ends_with("-restored.md")));
}

#[test]
fn default_templates_exist_and_render() {
    let (_d, mut store) = tmp_store();
    let ts = store.templates().unwrap();
    let slugs: Vec<&str> = ts.iter().map(|t| t.slug.as_str()).collect();
    assert!(slugs.contains(&"gratitude") && slugs.contains(&"weekly-review") && slugs.contains(&"daily"), "{slugs:?}");
    let g = store.template("gratitude").unwrap();
    assert_eq!(g.name, "Gratitude");
    assert_eq!(g.tags, vec!["gratitude"]);

    let created = chrono::DateTime::parse_from_rfc3339("2026-10-29T21:00:00+09:00").unwrap();
    let e = store.create_from_template("gratitude", NewEntry { created: Some(created), timezone: Some("Asia/Seoul".into()), ..Default::default() }).unwrap();
    assert!(e.body.starts_with("# Thursday, 2026-10-29\n"), "{}", e.body);
    assert_eq!(e.meta.tags, vec!["gratitude"]);
    assert!(!e.body.contains("{{"));

    // User-written template with a journal override and Korean content.
    std::fs::write(store.root().join("templates/감사.md"), "---\nname: 감사 일기\njournal: 마음\ntags: [감사]\n---\n{{date}} 오늘 감사한 것\n\n- \n").unwrap();
    let k = store.create_from_template("감사", NewEntry { timezone: Some("Asia/Seoul".into()), ..Default::default() }).unwrap();
    assert_eq!(k.meta.journal, "마음");
    assert!(k.body.contains("오늘 감사한 것"));
    assert!(store.templates().unwrap().iter().any(|t| t.name == "감사 일기"));
}

#[test]
fn stats_streak_counts_words() {
    let (_d, mut store) = tmp_store();
    let at = |s: &str| Some(chrono::DateTime::parse_from_rfc3339(s).unwrap());
    for (d, body) in [
        ("2026-10-29T08:00:00+09:00", "오늘은 맑음"),
        ("2026-10-28T08:00:00+09:00", "two words"),
        ("2026-10-27T08:00:00+09:00", "three little words"),
        ("2026-10-20T08:00:00+09:00", "x"),
        ("2025-01-01T08:00:00+09:00", "last year"),
    ] {
        store.create(NewEntry { body: body.into(), created: at(d), timezone: Some("Asia/Seoul".into()), ..Default::default() }).unwrap();
    }
    let s = store.stats(NaiveDate::from_ymd_opt(2026, 10, 29).unwrap()).unwrap();
    assert_eq!(s.total, 5);
    assert_eq!(s.this_month, 4);
    assert_eq!(s.this_year, 4);
    assert_eq!(s.streak_days, 3);
    assert_eq!(s.longest_streak, 3);
    assert_eq!(s.words, 5 + 2 + 3 + 1 + 2); // CJK chars count one each
    assert_eq!(s.first_entry, NaiveDate::from_ymd_opt(2025, 1, 1));
    assert_eq!(s.last_entry, NaiveDate::from_ymd_opt(2026, 10, 29));
}
