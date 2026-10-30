use chrono::{DateTime, NaiveDate};
use dayfile_core::{NewFact, Recall, Store};
use tempfile::TempDir;

fn tmp_store() -> (TempDir, Store) {
    let dir = TempDir::new().unwrap();
    std::env::set_var("DAYFILE_INDEX_DIR", dir.path().join("index"));
    let store = Store::init(dir.path().join("Journal")).unwrap();
    (dir, store)
}

fn d(s: &str) -> NaiveDate {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
}

fn texts(store: &Store, q: Recall) -> Vec<String> {
    store.recall(&q).unwrap().into_iter().map(|f| f.fact.text).collect()
}

#[test]
fn a_newer_fact_closes_the_old_one_and_history_keeps_both() {
    let (_d, mut store) = tmp_store();
    let busan = store.remember(NewFact { topic: "Home".into(), text: "Lives in Busan".into(), valid_from: Some(d("2019-01-01")), ..Default::default() }).unwrap();
    store.remember(NewFact { topic: "Home".into(), text: "Lives in Seoul".into(), valid_from: Some(d("2024-03-01")), replaces: vec![busan.fact.id.clone()], ..Default::default() }).unwrap();

    let home = |store: &Store, as_of: &str| texts(store, Recall { topic: Some("home".into()), as_of: Some(d(as_of)), ..Default::default() });
    assert_eq!(home(&store, "2023-06-01"), ["Lives in Busan"]);
    assert_eq!(home(&store, "2024-03-01"), ["Lives in Seoul"]);
    assert_eq!(home(&store, "2018-01-01"), Vec::<String>::new());

    let history = store.fact_history("Home").unwrap();
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].fact.valid_to, Some(d("2024-03-01")));
    assert!(history[0].fact.closed_at.is_some());

    // The file is the source: a full rebuild gives the same answers.
    let file = std::fs::read_to_string(store.root().join("memory/home.md")).unwrap();
    assert!(file.contains("text: Lives in Seoul"), "{file}");
    store.reindex(true).unwrap();
    assert_eq!(home(&store, "2023-06-01"), ["Lives in Busan"]);
}

#[test]
fn known_at_answers_with_what_the_journal_believed_then() {
    let (_d, mut store) = tmp_store();
    // Hand-written file: Busan recorded in January, closed by Seoul in May.
    std::fs::create_dir_all(store.root().join("memory")).unwrap();
    std::fs::write(
        store.root().join("memory/home.md"),
        "---
topic: Home
facts:
- id: fa
  text: Lives in Busan
  valid_from: 2019-01-01
  valid_to: 2024-03-01
  recorded_at: 2026-01-10T09:00:00+09:00
  closed_at: 2026-05-02T09:00:00+09:00
- id: fb
  text: Lives in Seoul
  valid_from: 2024-03-01
  recorded_at: 2026-05-02T09:00:00+09:00
  replaces: [fa]
- id: fc
  text: Lives in Daegu
  recorded_at: 2026-02-01T09:00:00+09:00
  retracted_at: 2026-02-03T09:00:00+09:00
---
",
    )
    .unwrap();
    store.reindex(false).unwrap();
    let at = |known: &str| {
        let known_at = DateTime::parse_from_rfc3339(known).unwrap();
        texts(&store, Recall { topic: Some("Home".into()), as_of: Some(d("2026-10-01")), known_at: Some(known_at), ..Default::default() })
    };
    // In March the journal still thought Busan, and briefly also Daegu before that was retracted.
    // Facts without a start date sort first.
    assert_eq!(at("2026-02-02T00:00:00+09:00"), ["Lives in Daegu", "Lives in Busan"]);
    assert_eq!(at("2026-03-01T00:00:00+09:00"), ["Lives in Busan"]);
    assert_eq!(at("2026-06-01T00:00:00+09:00"), ["Lives in Seoul"]);
}

#[test]
fn retract_search_topics_and_broken_files() {
    let (_d, mut store) = tmp_store();
    let a = store.remember(NewFact { topic: "회사".into(), text: "커머스 스타트업 백엔드 인턴".into(), ..Default::default() }).unwrap();
    store.remember(NewFact { topic: "People".into(), text: "Minji is my study partner".into(), source: Some("abc".into()), ..Default::default() }).unwrap();
    assert!(store.root().join("memory/회사.md").exists());
    assert_eq!(texts(&store, Recall { query: Some("minji".into()), ..Default::default() }), ["Minji is my study partner"]);

    store.retract(&a.fact.id).unwrap();
    assert!(texts(&store, Recall { topic: Some("회사".into()), ..Default::default() }).is_empty());
    assert_eq!(store.fact_history("회사").unwrap().len(), 1);
    assert_eq!(store.topics().unwrap(), vec![("People".to_string(), 1), ("회사".to_string(), 0)]);

    std::fs::write(store.root().join("memory/broken.md"), "---\nfacts: [oops\n---\n").unwrap();
    store.reindex(false).unwrap();
    assert!(store.problems().iter().any(|p| p.path == "memory/broken.md"), "{:?}", store.problems());
    assert_eq!(store.topics().unwrap().len(), 2);
}
