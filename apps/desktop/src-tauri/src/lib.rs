//! Tauri shell around `dayfile_core::Store`. Every command is a thin wrapper;
//! the `dayfile://` scheme serves attachment files to the webview.

use chrono::NaiveDate;
use dayfile_core::store::{EntryPatch, ReindexStats};
use dayfile_core::{import_dayone, today, CaptureConfig, CaptureReport, Drafts, Entry, EntrySummary, ImportOptions, ImportReport, ListFilter, NewEntry, Problem, Recall, Stats, Store, Template, TopicFact, TrashedEntry};
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use rand_core::OsRng;
use argon2::Argon2;
use std::time::{Duration, Instant};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::{AppHandle, Manager, State};

struct AppState {
    store: Mutex<Option<Store>>,
    config_path: PathBuf,
    /// Failed passcode attempts and the moment the lockout ends.
    attempts: Mutex<(u32, Option<Instant>)>,
}

#[derive(Serialize, Deserialize, Default, Clone)]
struct Config {
    root: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    lock: Option<LockConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    reminder: Option<ReminderConfig>,
    /// Shared with the CLI (`dayfile sources`).
    #[serde(default, skip_serializing_if = "CaptureConfig::is_empty")]
    capture: CaptureConfig,
}

#[derive(Serialize, Deserialize, Clone)]
struct LockConfig {
    /// argon2id PHC string. The passcode itself is never stored.
    hash: String,
    /// 0 = only on launch.
    idle_minutes: u32,
}

#[derive(Serialize, Deserialize, Clone, Default)]
struct ReminderConfig {
    enabled: bool,
    /// "21:00"
    time: String,
}

#[derive(Serialize)]
struct LockInfo {
    enabled: bool,
    idle_minutes: u32,
}

#[derive(Serialize)]
struct VerifyResult {
    ok: bool,
    /// Seconds until another attempt is allowed, when locked out.
    retry_after: u64,
}

const MAX_ATTEMPTS: u32 = 5;
const LOCKOUT: Duration = Duration::from_secs(30);

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
    /// The timeline shows drafts (badged) alongside entries.
    include_drafts: Option<bool>,
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
            drafts: if f.include_drafts.unwrap_or(false) { Drafts::Include } else { Drafts::Exclude },
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

fn update_config(path: &Path, f: impl FnOnce(&mut Config)) -> CmdResult<Config> {
    let mut cfg = load_config(path);
    f(&mut cfg);
    save_config(path, &cfg)?;
    Ok(cfg)
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

fn with_store<T>(state: &State<AppState>, f: impl FnOnce(&mut Store) -> dayfile_core::Result<T>) -> CmdResult<T> {
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
    update_config(&state.config_path, |c| c.root = Some(root))?;
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

// ----- trash / templates / stats -----

#[tauri::command]
fn trash_list(state: State<AppState>) -> CmdResult<Vec<TrashedEntry>> {
    with_store(&state, |s| s.trash_list())
}

#[tauri::command]
fn restore_entry(state: State<AppState>, id: String) -> CmdResult<Entry> {
    with_store(&state, |s| s.restore(&id))
}

#[tauri::command]
fn empty_trash(state: State<AppState>) -> CmdResult<usize> {
    with_store(&state, |s| s.empty_trash())
}

#[tauri::command]
fn list_templates(state: State<AppState>) -> CmdResult<Vec<Template>> {
    with_store(&state, |s| s.templates())
}

#[tauri::command]
fn create_from_template(state: State<AppState>, slug: String, entry: NewEntry) -> CmdResult<Entry> {
    with_store(&state, |s| s.create_from_template(&slug, entry))
}

#[tauri::command]
fn get_stats(state: State<AppState>, today: NaiveDate) -> CmdResult<Stats> {
    with_store(&state, |s| s.stats(today))
}

// ----- lock -----

#[tauri::command]
fn lock_info(state: State<AppState>) -> LockInfo {
    match load_config(&state.config_path).lock {
        Some(l) => LockInfo { enabled: true, idle_minutes: l.idle_minutes },
        None => LockInfo { enabled: false, idle_minutes: 5 },
    }
}

fn hash_passcode(pass: &str) -> CmdResult<String> {
    let salt = SaltString::generate(&mut OsRng);
    Ok(Argon2::default().hash_password(pass.as_bytes(), &salt).map_err(err)?.to_string())
}

fn check(hash: &str, pass: &str) -> bool {
    PasswordHash::new(hash).map(|h| Argon2::default().verify_password(pass.as_bytes(), &h).is_ok()).unwrap_or(false)
}

/// Set or change the passcode. `current` is required when one already exists.
#[tauri::command]
fn set_passcode(state: State<AppState>, current: Option<String>, new: String, idle_minutes: u32) -> CmdResult<LockInfo> {
    if new.chars().count() < 4 {
        return Err("passcode must be at least 4 characters".into());
    }
    let cfg = load_config(&state.config_path);
    if let Some(l) = &cfg.lock {
        if !current.as_deref().map(|c| check(&l.hash, c)).unwrap_or(false) {
            return Err("current passcode is wrong".into());
        }
    }
    let hash = hash_passcode(&new)?;
    update_config(&state.config_path, |c| c.lock = Some(LockConfig { hash, idle_minutes }))?;
    Ok(LockInfo { enabled: true, idle_minutes })
}

#[tauri::command]
fn set_lock_idle(state: State<AppState>, idle_minutes: u32) -> CmdResult<LockInfo> {
    let cfg = update_config(&state.config_path, |c| {
        if let Some(l) = c.lock.as_mut() {
            l.idle_minutes = idle_minutes;
        }
    })?;
    Ok(LockInfo { enabled: cfg.lock.is_some(), idle_minutes })
}

#[tauri::command]
fn clear_passcode(state: State<AppState>, current: String) -> CmdResult<LockInfo> {
    let cfg = load_config(&state.config_path);
    match &cfg.lock {
        Some(l) if check(&l.hash, &current) => {
            update_config(&state.config_path, |c| c.lock = None)?;
            Ok(LockInfo { enabled: false, idle_minutes: 5 })
        }
        Some(_) => Err("current passcode is wrong".into()),
        None => Ok(LockInfo { enabled: false, idle_minutes: 5 }),
    }
}

#[tauri::command]
fn verify_passcode(state: State<AppState>, pass: String) -> CmdResult<VerifyResult> {
    let mut att = state.attempts.lock().unwrap_or_else(|p| p.into_inner());
    if let Some(until) = att.1 {
        let now = Instant::now();
        if now < until {
            return Ok(VerifyResult { ok: false, retry_after: (until - now).as_secs().max(1) });
        }
        *att = (0, None);
    }
    let Some(l) = load_config(&state.config_path).lock else {
        return Ok(VerifyResult { ok: true, retry_after: 0 });
    };
    if check(&l.hash, &pass) {
        *att = (0, None);
        Ok(VerifyResult { ok: true, retry_after: 0 })
    } else {
        att.0 += 1;
        if att.0 >= MAX_ATTEMPTS {
            att.1 = Some(Instant::now() + LOCKOUT);
            att.0 = 0;
            return Ok(VerifyResult { ok: false, retry_after: LOCKOUT.as_secs() });
        }
        Ok(VerifyResult { ok: false, retry_after: 0 })
    }
}

// ----- reminder -----

#[tauri::command]
fn get_reminder(state: State<AppState>) -> ReminderConfig {
    load_config(&state.config_path).reminder.unwrap_or(ReminderConfig { enabled: false, time: "21:00".into() })
}

#[tauri::command]
fn set_reminder(state: State<AppState>, enabled: bool, time: String) -> CmdResult<ReminderConfig> {
    let r = ReminderConfig { enabled, time };
    let saved = r.clone();
    update_config(&state.config_path, |c| c.reminder = Some(r))?;
    Ok(saved)
}

#[tauri::command]
fn get_capture(state: State<AppState>) -> CaptureConfig {
    load_config(&state.config_path).capture
}

#[tauri::command]
fn set_capture(state: State<AppState>, capture: CaptureConfig) -> CmdResult<CaptureConfig> {
    Ok(update_config(&state.config_path, |c| c.capture = capture)?.capture)
}

/// Off the main thread: a calendar feed can take seconds to download.
#[tauri::command(async)]
fn capture_day(state: State<AppState>, date: Option<NaiveDate>) -> CmdResult<CaptureReport> {
    let cfg = load_config(&state.config_path).capture;
    with_store(&state, |s| s.capture(date.unwrap_or_else(today), &cfg))
}

#[tauri::command]
fn accept_draft(state: State<AppState>, id: String) -> CmdResult<Entry> {
    with_store(&state, |s| s.accept_draft(&id))
}

#[tauri::command]
fn recall(state: State<AppState>, topic: Option<String>, query: Option<String>, as_of: Option<NaiveDate>) -> CmdResult<Vec<TopicFact>> {
    with_store(&state, |s| s.recall(&Recall { topic, query, as_of, known_at: None }))
}

#[tauri::command]
fn fact_history(state: State<AppState>, topic: String) -> CmdResult<Vec<TopicFact>> {
    with_store(&state, |s| s.fact_history(&topic))
}

#[tauri::command]
fn list_topics(state: State<AppState>) -> CmdResult<Vec<(String, usize)>> {
    with_store(&state, |s| s.topics())
}

#[tauri::command]
fn retract_fact(state: State<AppState>, id: String) -> CmdResult<TopicFact> {
    with_store(&state, |s| s.retract(&id))
}

#[tauri::command]
fn list_problems(state: State<AppState>) -> CmdResult<Vec<Problem>> {
    with_store(&state, |s| Ok(s.problems().to_vec()))
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

/// dayfile://localhost/attachments/<file>  ->  <root>/attachments/<file>
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
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            // DAYFILE_CONFIG points at an alternative config.json (tests, QA).
            let config_path = match std::env::var_os("DAYFILE_CONFIG") {
                Some(p) => PathBuf::from(p),
                None => app.path().app_config_dir()?.join("config.json"),
            };
            // Settings from before the rename (bundle id dev.journal.desktop).
            if !config_path.exists() {
                if let Some(old) = config_path.parent().and_then(|d| d.parent()).map(|d| d.join("dev.journal.desktop/config.json")) {
                    if old.exists() {
                        let _ = std::fs::create_dir_all(config_path.parent().unwrap());
                        let _ = std::fs::copy(&old, &config_path);
                    }
                }
            }
            let cfg = load_config(&config_path);
            // DAYFILE_ROOT overrides the saved folder without touching the config (testing, scripts).
            let root = std::env::var("DAYFILE_ROOT").ok().or(cfg.root);
            let store = root.as_deref().and_then(|r| Store::open(r).ok());
            app.manage(AppState { store: Mutex::new(store), config_path, attempts: Mutex::new((0, None)) });
            Ok(())
        })
        .register_uri_scheme_protocol("dayfile", |ctx, request| serve_attachment(ctx.app_handle(), request.uri().path()))
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
            list_problems,
            trash_list,
            restore_entry,
            empty_trash,
            list_templates,
            create_from_template,
            get_stats,
            lock_info,
            set_passcode,
            set_lock_idle,
            clear_passcode,
            verify_passcode,
            get_reminder,
            set_reminder,
            attach_file,
            import_dayone_export,
            entry_abs_path,
            get_capture,
            set_capture,
            capture_day,
            accept_draft,
            recall,
            fact_history,
            list_topics,
            retract_fact,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
