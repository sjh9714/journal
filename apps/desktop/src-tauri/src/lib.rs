//! Tauri shell around `journal_core::Store`. Every command is a thin wrapper;
//! the `journal://` scheme serves attachment files to the webview.

use chrono::NaiveDate;
use journal_core::store::{EntryPatch, ReindexStats};
use journal_core::{import_dayone, Entry, EntrySummary, ImportOptions, ImportReport, ListFilter, NewEntry, Store};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::{AppHandle, Manager, State};

struct AppState {
    store: Mutex<Option<Store>>,
    config_path: PathBuf,
}

#[derive(Serialize, Deserialize, Default, Clone)]
struct Config {
    root: Option<String>,
}

#[derive(Serialize)]
struct Status {
    root: Option<String>,
    index_path: Option<String>,
    entries: usize,
    default_root: String,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct FilterDto {
    journal: Option<String>,
    tag: Option<String>,
    from: Option<NaiveDate>,
    to: Option<NaiveDate>,
    starred: Option<bool>,
    limit: Option<usize>,
    offset: Option<usize>,
    ascending: Option<bool>,
}

impl From<FilterDto> for ListFilter {
    fn from(f: FilterDto) -> Self {
        ListFilter {
            journal: f.journal,
            tag: f.tag,
            from: f.from,
            to: f.to,
            starred: f.starred,
            limit: f.limit,
            offset: f.offset.unwrap_or(0),
            ascending: f.ascending.unwrap_or(false),
        }
    }
}

#[derive(Serialize)]
struct Attached {
    /// e.g. attachments/<sha>.png
    rel: String,
    /// Markdown link target relative to the entry file.
    link: String,
}

type CmdResult<T> = Result<T, String>;

fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

fn default_root() -> PathBuf {
    dirs::home_dir().map(|h| h.join("Journal")).unwrap_or_else(|| PathBuf::from("Journal"))
}

fn load_config(path: &Path) -> Config {
    std::fs::read_to_string(path).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

fn save_config(path: &Path, cfg: &Config) -> CmdResult<()> {
    if let Some(p) = path.parent() {
        std::fs::create_dir_all(p).map_err(err)?;
    }
    std::fs::write(path, serde_json::to_string_pretty(cfg).map_err(err)?).map_err(err)
}

fn with_store<T>(state: &State<AppState>, f: impl FnOnce(&mut Store) -> journal_core::Result<T>) -> CmdResult<T> {
    let mut guard = state.store.lock().unwrap_or_else(|p| p.into_inner());
    let store = guard.as_mut().ok_or_else(|| "no journal open".to_string())?;
    f(store).map_err(err)
}

fn status_of(state: &State<AppState>) -> Status {
    let guard = state.store.lock().unwrap_or_else(|p| p.into_inner());
    match guard.as_ref() {
        Some(s) => Status {
            root: Some(s.root().to_string_lossy().into_owned()),
            index_path: Some(Store::index_path(s.root()).to_string_lossy().into_owned()),
            entries: s.count(&ListFilter::default()).unwrap_or(0),
            default_root: default_root().to_string_lossy().into_owned(),
        },
        None => Status { root: None, index_path: None, entries: 0, default_root: default_root().to_string_lossy().into_owned() },
    }
}

/// Webview errors land in the terminal during `tauri dev`.
#[tauri::command]
fn log_js(level: String, msg: String) {
    eprintln!("[webview:{level}] {msg}");
}

#[tauri::command]
fn get_status(state: State<AppState>) -> Status {
    status_of(&state)
}

/// Open (or create) a journal folder and remember it.
#[tauri::command]
fn open_root(state: State<AppState>, path: String, create: bool) -> CmdResult<Status> {
    let p = PathBuf::from(&path);
    let store = if create { Store::init(&p) } else { Store::open(&p) }.map_err(err)?;
    let root = store.root().to_string_lossy().into_owned();
    *state.store.lock().unwrap_or_else(|p| p.into_inner()) = Some(store);
    save_config(&state.config_path, &Config { root: Some(root) })?;
    Ok(status_of(&state))
}

#[tauri::command]
fn list_entries(state: State<AppState>, filter: FilterDto) -> CmdResult<Vec<EntrySummary>> {
    with_store(&state, |s| s.list(&filter.into()))
}

#[tauri::command]
fn get_entry(state: State<AppState>, id: String) -> CmdResult<Entry> {
    with_store(&state, |s| s.get(&id))
}

#[tauri::command]
fn create_entry(state: State<AppState>, entry: NewEntry) -> CmdResult<Entry> {
    with_store(&state, |s| s.create(entry))
}

#[tauri::command]
fn update_entry(state: State<AppState>, id: String, patch: EntryPatch) -> CmdResult<Entry> {
    with_store(&state, |s| s.update(&id, patch))
}

#[tauri::command]
fn delete_entry(state: State<AppState>, id: String) -> CmdResult<Entry> {
    with_store(&state, |s| s.delete(&id))
}

#[tauri::command]
fn search_entries(state: State<AppState>, query: String, limit: Option<usize>) -> CmdResult<Vec<EntrySummary>> {
    with_store(&state, |s| s.search(&query, limit.unwrap_or(50)))
}

#[tauri::command]
fn on_this_day(state: State<AppState>, date: NaiveDate) -> CmdResult<Vec<EntrySummary>> {
    with_store(&state, |s| s.on_this_day(date))
}

#[tauri::command]
fn on_date(state: State<AppState>, date: NaiveDate) -> CmdResult<Vec<EntrySummary>> {
    with_store(&state, |s| s.on_date(date))
}

#[tauri::command]
fn list_tags(state: State<AppState>) -> CmdResult<Vec<(String, usize)>> {
    with_store(&state, |s| s.tags())
}

#[tauri::command]
fn list_journals(state: State<AppState>) -> CmdResult<Vec<(String, usize)>> {
    with_store(&state, |s| s.journals())
}

#[tauri::command]
fn reindex(state: State<AppState>, full: bool) -> CmdResult<ReindexStats> {
    with_store(&state, |s| s.reindex(full))
}

#[tauri::command]
fn attach_file(state: State<AppState>, path: String) -> CmdResult<Attached> {
    with_store(&state, |s| {
        let rel = s.attach(Path::new(&path))?;
        Ok(Attached { link: Store::attachment_link(&rel), rel: rel.to_string_lossy().replace('\\', "/") })
    })
}

#[tauri::command]
fn import_dayone_export(state: State<AppState>, path: String, journal: Option<String>) -> CmdResult<ImportReport> {
    with_store(&state, |s| import_dayone(s, Path::new(&path), &ImportOptions { journal, dry_run: false }))
}

#[tauri::command]
fn entry_abs_path(state: State<AppState>, id: String) -> CmdResult<String> {
    with_store(&state, |s| {
        let e = s.get(&id)?;
        Ok(s.root().join(e.path).to_string_lossy().into_owned())
    })
}

fn mime_for(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).as_deref() {
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("heic") => "image/heic",
        Some("svg") => "image/svg+xml",
        Some("mp4") => "video/mp4",
        Some("mov") => "video/quicktime",
        Some("m4a") => "audio/mp4",
        Some("mp3") => "audio/mpeg",
        Some("pdf") => "application/pdf",
        _ => "application/octet-stream",
    }
}

/// journal://localhost/attachments/<file>  ->  <root>/attachments/<file>
fn serve_attachment(app: &AppHandle, uri_path: &str) -> tauri::http::Response<Vec<u8>> {
    use tauri::http::{Response, StatusCode};
    let decoded = percent_decode(uri_path.trim_start_matches('/'));
    let rel = Path::new(&decoded);
    let ok = rel.components().count() == 2
        && rel.starts_with("attachments")
        && !rel.components().any(|c| matches!(c, std::path::Component::ParentDir));
    let state = app.state::<AppState>();
    let root = state.store.lock().unwrap_or_else(|p| p.into_inner()).as_ref().map(|s| s.root().to_path_buf());
    let (Some(root), true) = (root, ok) else {
        return Response::builder().status(StatusCode::FORBIDDEN).body(Vec::new()).unwrap();
    };
    let abs = root.join(rel);
    match std::fs::read(&abs) {
        Ok(bytes) => Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", mime_for(&abs))
            .header("Cache-Control", "max-age=31536000, immutable")
            .body(bytes)
            .unwrap(),
        Err(_) => Response::builder().status(StatusCode::NOT_FOUND).body(Vec::new()).unwrap(),
    }
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let config_path = app.path().app_config_dir()?.join("config.json");
            let cfg = load_config(&config_path);
            // JOURNAL_ROOT overrides the saved folder without touching the config (testing, scripts).
            let root = std::env::var("JOURNAL_ROOT").ok().or(cfg.root);
            let store = root.as_deref().and_then(|r| Store::open(r).ok());
            app.manage(AppState { store: Mutex::new(store), config_path });
            Ok(())
        })
        .register_uri_scheme_protocol("journal", |ctx, request| serve_attachment(ctx.app_handle(), request.uri().path()))
        .invoke_handler(tauri::generate_handler![
            log_js,
            get_status,
            open_root,
            list_entries,
            get_entry,
            create_entry,
            update_entry,
            delete_entry,
            search_entries,
            on_this_day,
            on_date,
            list_tags,
            list_journals,
            reindex,
            attach_file,
            import_dayone_export,
            entry_abs_path,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
