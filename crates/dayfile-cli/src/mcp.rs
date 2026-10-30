//! MCP server over stdio. Every tool is a thin call into `dayfile_core::Store`.

use anyhow::{anyhow, Result};
use chrono::NaiveDate;
use dayfile_core::store::EntryPatch;
use dayfile_core::{today, CaptureConfig, CaptureReport, Entry, EntrySummary, ListFilter, NewEntry, NewFact, Recall, Store, TopicFact};
use rmcp::handler::server::router::prompt::PromptRouter;
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::{Json, Parameters};
use rmcp::model::{ErrorData, Implementation, PromptMessage, Role, ServerCapabilities, ServerConfig};
use rmcp::transport::stdio;
use rmcp::{prompt, prompt_handler, prompt_router, tool, tool_handler, tool_router, ServerHandler, ServiceExt};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct JournalServer {
    store: Arc<Mutex<Store>>,
    read_only: bool,
    capture: CaptureConfig,
    tool_router: ToolRouter<Self>,
    prompt_router: PromptRouter<Self>,
}

// ---------- outputs ----------

#[derive(Serialize, JsonSchema)]
pub struct EntryOut {
    pub uuid: String,
    /// RFC 3339 with the entry's local offset.
    pub created: String,
    /// Local calendar date, YYYY-MM-DD.
    pub date: String,
    pub journal: String,
    pub tags: Vec<String>,
    pub starred: bool,
    pub title: String,
    /// Path relative to the journal root.
    pub path: String,
    /// Captured and not yet reviewed by the user.
    pub draft: bool,
}

impl From<EntrySummary> for EntryOut {
    fn from(s: EntrySummary) -> Self {
        Self {
            uuid: s.uuid,
            created: s.created.to_rfc3339(),
            date: s.date.to_string(),
            journal: s.journal,
            tags: s.tags,
            starred: s.starred,
            title: s.title,
            path: s.path.to_string_lossy().into_owned(),
            draft: s.draft,
        }
    }
}

#[derive(Serialize, JsonSchema)]
pub struct FullEntryOut {
    #[serde(flatten)]
    pub summary: EntryOut,
    pub timezone: String,
    pub body: String,
    pub location: Option<LocationOut>,
    pub weather: Option<WeatherOut>,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone)]
pub struct LocationOut {
    pub latitude: f64,
    pub longitude: f64,
    pub place_name: Option<String>,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone)]
pub struct WeatherOut {
    pub temperature_c: Option<f64>,
    pub conditions: Option<String>,
}

impl From<Entry> for FullEntryOut {
    fn from(e: Entry) -> Self {
        let location = e.meta.location.clone().map(|l| LocationOut { latitude: l.latitude, longitude: l.longitude, place_name: l.place_name });
        let weather = e.meta.weather.clone().map(|w| WeatherOut { temperature_c: w.temperature_c, conditions: w.conditions });
        Self { summary: e.summary().into(), timezone: e.meta.timezone.clone(), body: e.body, location, weather }
    }
}

#[derive(Serialize, JsonSchema)]
pub struct TagCount {
    pub tag: String,
    pub count: usize,
}

#[derive(Serialize, JsonSchema)]
pub struct JournalCount {
    pub journal: String,
    pub count: usize,
}

// ---------- inputs ----------

#[derive(Deserialize, JsonSchema)]
pub struct ListParams {
    /// Only entries in this journal.
    pub journal: Option<String>,
    /// Only entries carrying this tag.
    pub tag: Option<String>,
    /// Inclusive start date, YYYY-MM-DD.
    pub from: Option<String>,
    /// Inclusive end date, YYYY-MM-DD.
    pub to: Option<String>,
    /// Only starred entries when true.
    pub starred: Option<bool>,
    /// Max rows, default 20, max 200.
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

#[derive(Deserialize, JsonSchema)]
pub struct GetParams {
    /// Full uuid or a unique prefix.
    pub id: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct SearchParams {
    /// Words are ANDed. Append * to a word for prefix match, e.g. "river*".
    pub query: String,
    /// Max rows, default 20, max 200.
    pub limit: Option<usize>,
}

#[derive(Deserialize, JsonSchema)]
pub struct DateParams {
    /// YYYY-MM-DD. Defaults to today.
    pub date: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
pub struct CreateParams {
    /// Markdown body. First line becomes the title.
    pub body: String,
    /// Journal name, default "default".
    pub journal: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub starred: bool,
    /// RFC 3339 creation time with offset, e.g. 2026-10-28T21:00:00+09:00. Defaults to now.
    pub created: Option<String>,
    pub location: Option<LocationOut>,
    pub weather: Option<WeatherOut>,
}

#[derive(Deserialize, JsonSchema)]
pub struct AppendParams {
    /// Text to append as a new paragraph.
    pub text: String,
    /// Journal to append into when creating today's entry, default "default".
    pub journal: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
pub struct UpdateParams {
    /// Full uuid or a unique prefix.
    pub id: String,
    /// Replace the whole Markdown body.
    pub body: Option<String>,
    /// Replace the tag list.
    pub tags: Option<Vec<String>>,
    pub starred: Option<bool>,
    pub journal: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
pub struct DeleteParams {
    /// Full uuid or a unique prefix.
    pub id: String,
}

#[derive(Serialize, JsonSchema)]
pub struct FactOut {
    pub id: String,
    pub topic: String,
    pub text: String,
    /// YYYY-MM-DD the fact became true, if known.
    pub valid_from: Option<String>,
    /// YYYY-MM-DD it stopped being true (exclusive), if it has.
    pub valid_to: Option<String>,
    /// When the journal learned it (RFC 3339).
    pub recorded_at: String,
    /// When it was found never true, if it was.
    pub retracted_at: Option<String>,
    /// uuid of the entry it came from.
    pub source: Option<String>,
    pub replaces: Vec<String>,
}

impl From<TopicFact> for FactOut {
    fn from(t: TopicFact) -> Self {
        let f = t.fact;
        Self {
            id: f.id,
            topic: t.topic,
            text: f.text,
            valid_from: f.valid_from.map(|d| d.to_string()),
            valid_to: f.valid_to.map(|d| d.to_string()),
            recorded_at: f.recorded_at.to_rfc3339(),
            retracted_at: f.retracted_at.map(|d| d.to_rfc3339()),
            source: f.source,
            replaces: f.replaces,
        }
    }
}

#[derive(Serialize, JsonSchema)]
pub struct CaptureOut {
    pub date: String,
    /// Traces new to the journal in this run.
    pub added: usize,
    pub commits: usize,
    pub events: usize,
    pub photos: usize,
    /// The day's draft, if there is one; read it with get_entry.
    pub draft: Option<EntryOut>,
    pub created: bool,
    /// Sources that failed; the others still ran.
    pub errors: Vec<String>,
}

impl From<CaptureReport> for CaptureOut {
    fn from(r: CaptureReport) -> Self {
        Self { date: r.date.to_string(), added: r.added, commits: r.commits, events: r.events, photos: r.photos, draft: r.draft.map(Into::into), created: r.created, errors: r.errors }
    }
}

#[derive(Deserialize, JsonSchema)]
pub struct RememberParams {
    /// Short subject the fact belongs to, e.g. "Home", "Work", "Minji", "Health".
    pub topic: String,
    /// One self-contained statement, e.g. "Works at Acme as a backend intern".
    pub text: String,
    /// YYYY-MM-DD the fact became true, if known.
    pub valid_from: Option<String>,
    /// uuid of the entry the fact comes from.
    pub source: Option<String>,
    /// Ids of facts this one supersedes (from recall); they end at valid_from.
    #[serde(default)]
    pub replaces: Vec<String>,
}

#[derive(Deserialize, JsonSchema)]
pub struct RecallParams {
    /// Only this topic.
    pub topic: Option<String>,
    /// Case-insensitive text to look for.
    pub query: Option<String>,
    /// YYYY-MM-DD: facts true on that day. Defaults to today.
    pub as_of: Option<String>,
    /// RFC 3339: answer with what the journal knew at that moment.
    pub known_at: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
pub struct TopicParams {
    pub topic: String,
}

#[derive(Serialize, JsonSchema)]
pub struct TopicCount {
    pub topic: String,
    /// Facts holding today.
    pub current: usize,
}

#[derive(Deserialize, JsonSchema)]
pub struct DayPromptArgs {
    /// YYYY-MM-DD, default today.
    pub date: Option<String>,
}

fn internal(e: impl std::fmt::Display) -> ErrorData {
    ErrorData::internal_error(e.to_string(), None)
}
fn bad(e: impl std::fmt::Display) -> ErrorData {
    ErrorData::invalid_params(e.to_string(), None)
}
fn parse_date(s: Option<String>) -> Result<NaiveDate, ErrorData> {
    match s {
        None => Ok(today()),
        Some(s) => NaiveDate::parse_from_str(&s, "%Y-%m-%d").map_err(bad),
    }
}
fn clamp(limit: Option<usize>) -> usize {
    limit.unwrap_or(20).clamp(1, 200)
}

#[tool_router(router = tool_router)]
impl JournalServer {
    pub fn new(store: Store, read_only: bool, capture: CaptureConfig) -> Self {
        Self { store: Arc::new(Mutex::new(store)), read_only, capture, tool_router: Self::tool_router(), prompt_router: Self::prompt_router() }
    }

    fn guard_write(&self) -> Result<(), ErrorData> {
        if self.read_only {
            Err(ErrorData::invalid_request("journal MCP server is running in read-only mode", None))
        } else {
            Ok(())
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Store> {
        self.store.lock().unwrap_or_else(|p| p.into_inner())
    }

    #[tool(description = "List journal entries, newest first. Filter by journal, tag, date range or starred.")]
    async fn list_entries(&self, p: Parameters<ListParams>) -> Result<Json<Vec<EntryOut>>, ErrorData> {
        let p = p.0;
        let from = p.from.map(|s| NaiveDate::parse_from_str(&s, "%Y-%m-%d")).transpose().map_err(bad)?;
        let to = p.to.map(|s| NaiveDate::parse_from_str(&s, "%Y-%m-%d")).transpose().map_err(bad)?;
        let f = ListFilter { journal: p.journal, tag: p.tag, from, to, starred: p.starred, limit: Some(clamp(p.limit)), offset: p.offset.unwrap_or(0), ascending: false, ..Default::default() };
        let rows = self.lock().list(&f).map_err(internal)?;
        Ok(Json(rows.into_iter().map(Into::into).collect()))
    }

    #[tool(description = "Read one entry in full (metadata and Markdown body) by uuid or unique prefix.")]
    async fn get_entry(&self, p: Parameters<GetParams>) -> Result<Json<FullEntryOut>, ErrorData> {
        let e = self.lock().get(&p.0.id).map_err(bad)?;
        Ok(Json(e.into()))
    }

    #[tool(description = "Full-text search over titles, bodies and tags. Returns summaries; call get_entry for the body.")]
    async fn search_entries(&self, p: Parameters<SearchParams>) -> Result<Json<Vec<EntryOut>>, ErrorData> {
        let rows = self.lock().search(&p.0.query, clamp(p.0.limit)).map_err(internal)?;
        Ok(Json(rows.into_iter().map(Into::into).collect()))
    }

    #[tool(description = "Entries written on this calendar day in previous years (the 'On This Day' view).")]
    async fn on_this_day(&self, p: Parameters<DateParams>) -> Result<Json<Vec<EntryOut>>, ErrorData> {
        let d = parse_date(p.0.date)?;
        let rows = self.lock().on_this_day(d).map_err(internal)?;
        Ok(Json(rows.into_iter().map(Into::into).collect()))
    }

    #[tool(description = "Entries written on a given date (default today), oldest first.")]
    async fn entries_on_date(&self, p: Parameters<DateParams>) -> Result<Json<Vec<EntryOut>>, ErrorData> {
        let d = parse_date(p.0.date)?;
        let rows = self.lock().on_date(d).map_err(internal)?;
        Ok(Json(rows.into_iter().map(Into::into).collect()))
    }

    #[tool(description = "All tags with how many entries use each.")]
    async fn list_tags(&self) -> Result<Json<Vec<TagCount>>, ErrorData> {
        let rows = self.lock().tags().map_err(internal)?;
        Ok(Json(rows.into_iter().map(|(tag, count)| TagCount { tag, count }).collect()))
    }

    #[tool(description = "All journals (notebooks) with entry counts.")]
    async fn list_journals(&self) -> Result<Json<Vec<JournalCount>>, ErrorData> {
        let rows = self.lock().journals().map_err(internal)?;
        Ok(Json(rows.into_iter().map(|(journal, count)| JournalCount { journal, count }).collect()))
    }

    #[tool(description = "Create a new entry. Returns the created entry.")]
    async fn create_entry(&self, p: Parameters<CreateParams>) -> Result<Json<FullEntryOut>, ErrorData> {
        self.guard_write()?;
        let p = p.0;
        let created = p.created.map(|s| chrono::DateTime::parse_from_rfc3339(&s)).transpose().map_err(bad)?;
        let e = self
            .lock()
            .create(NewEntry {
                body: p.body,
                created,
                journal: p.journal,
                tags: p.tags,
                starred: p.starred,
                location: p.location.map(|l| dayfile_core::Location { latitude: l.latitude, longitude: l.longitude, place_name: l.place_name }),
                weather: p.weather.map(|w| dayfile_core::Weather { temperature_c: w.temperature_c, conditions: w.conditions }),
                ..Default::default()
            })
            .map_err(internal)?;
        Ok(Json(e.into()))
    }

    #[tool(description = "Append a paragraph to today's entry in the given journal, creating the entry if today has none.")]
    async fn append_to_today(&self, p: Parameters<AppendParams>) -> Result<Json<FullEntryOut>, ErrorData> {
        self.guard_write()?;
        let p = p.0;
        let journal = p.journal.unwrap_or_else(|| "default".into());
        let today = today();
        let mut store = self.lock();
        let existing = store
            .on_date(today)
            .map_err(internal)?
            .into_iter()
            .find(|s| s.journal == journal);
        let e = match existing {
            Some(s) => store.append(&s.uuid, &p.text).map_err(internal)?,
            None => store.create(NewEntry { body: p.text, journal: Some(journal), ..Default::default() }).map_err(internal)?,
        };
        Ok(Json(e.into()))
    }

    #[tool(description = "Update an entry's body, tags, starred flag or journal. Omitted fields are left unchanged.")]
    async fn update_entry(&self, p: Parameters<UpdateParams>) -> Result<Json<FullEntryOut>, ErrorData> {
        self.guard_write()?;
        let p = p.0;
        let e = self
            .lock()
            .update(&p.id, EntryPatch { body: p.body, tags: p.tags, starred: p.starred, journal: p.journal, ..Default::default() })
            .map_err(bad)?;
        Ok(Json(e.into()))
    }

    #[tool(description = "Move an entry to the trash (restorable for 30 days from the app or `dayfile trash restore`). Returns the removed entry.")]
    async fn delete_entry(&self, p: Parameters<DeleteParams>) -> Result<Json<EntryOut>, ErrorData> {
        self.guard_write()?;
        let e = self.lock().delete(&p.0.id).map_err(bad)?;
        Ok(Json(e.summary().into()))
    }

    #[tool(description = "Draft a day (default today) from the user's git commits, calendar events and photos. Adds only traces no entry holds yet; returns the draft. Drafts are hidden from list and search until accepted.")]
    async fn capture_day(&self, p: Parameters<DateParams>) -> Result<Json<CaptureOut>, ErrorData> {
        self.guard_write()?;
        if self.capture.is_empty() {
            return Err(ErrorData::invalid_request("no capture sources configured; the user adds them in the app's Settings or with `dayfile sources add`", None));
        }
        let d = parse_date(p.0.date)?;
        Ok(Json(self.lock().capture(d, &self.capture).map_err(internal)?.into()))
    }

    #[tool(description = "Captured drafts waiting for the user's review, newest first.")]
    async fn list_drafts(&self) -> Result<Json<Vec<EntryOut>>, ErrorData> {
        let rows = self.lock().drafts().map_err(internal)?;
        Ok(Json(rows.into_iter().map(Into::into).collect()))
    }

    #[tool(description = "Keep a draft as an ordinary entry. Only when the user asks to keep it.")]
    async fn accept_draft(&self, p: Parameters<GetParams>) -> Result<Json<FullEntryOut>, ErrorData> {
        self.guard_write()?;
        Ok(Json(self.lock().accept_draft(&p.0.id).map_err(bad)?.into()))
    }

    #[tool(description = "Record a durable fact about the user's life (people, places, work, health, plans) in their memory. Call recall first; pass replaces when the fact updates an earlier one so history stays correct.")]
    async fn remember(&self, p: Parameters<RememberParams>) -> Result<Json<FactOut>, ErrorData> {
        self.guard_write()?;
        let p = p.0;
        let valid_from = p.valid_from.map(|s| NaiveDate::parse_from_str(&s, "%Y-%m-%d")).transpose().map_err(bad)?;
        let mut store = self.lock();
        let source = p.source.map(|s| store.get(&s).map(|e| e.meta.uuid)).transpose().map_err(bad)?;
        Ok(Json(store.remember(NewFact { topic: p.topic, text: p.text, valid_from, source, replaces: p.replaces }).map_err(bad)?.into()))
    }

    #[tool(description = "Facts from the user's memory that hold on a date (default today). known_at answers with what the journal believed at that moment. Each fact cites its source entry.")]
    async fn recall(&self, p: Parameters<RecallParams>) -> Result<Json<Vec<FactOut>>, ErrorData> {
        let p = p.0;
        let as_of = p.as_of.map(|s| NaiveDate::parse_from_str(&s, "%Y-%m-%d")).transpose().map_err(bad)?;
        let known_at = p.known_at.map(|s| chrono::DateTime::parse_from_rfc3339(&s)).transpose().map_err(bad)?;
        Ok(Json(self.lock().recall(&Recall { topic: p.topic, query: p.query, as_of, known_at }).map_err(internal)?.into_iter().map(Into::into).collect()))
    }

    #[tool(description = "Every fact of a topic over time, including ended and retracted ones, oldest first.")]
    async fn fact_history(&self, p: Parameters<TopicParams>) -> Result<Json<Vec<FactOut>>, ErrorData> {
        Ok(Json(self.lock().fact_history(&p.0.topic).map_err(internal)?.into_iter().map(Into::into).collect()))
    }

    #[tool(description = "Memory topics with how many facts hold today.")]
    async fn list_topics(&self) -> Result<Json<Vec<TopicCount>>, ErrorData> {
        let rows = self.lock().topics().map_err(internal)?;
        Ok(Json(rows.into_iter().map(|(topic, current)| TopicCount { topic, current }).collect()))
    }

    #[tool(description = "Mark a fact as never true (a mistake). It stays in history. To record a change over time use remember with replaces instead.")]
    async fn retract_fact(&self, p: Parameters<GetParams>) -> Result<Json<FactOut>, ErrorData> {
        self.guard_write()?;
        Ok(Json(self.lock().retract(&p.0.id).map_err(bad)?.into()))
    }
}

#[prompt_router]
impl JournalServer {
    #[prompt(name = "review_day", description = "Turn a captured day into a journal entry and update memory from it.")]
    async fn review_day(&self, p: Parameters<DayPromptArgs>) -> Result<Vec<PromptMessage>, ErrorData> {
        let date = parse_date(p.0.date)?;
        Ok(vec![PromptMessage::new_text(
            Role::User,
            format!(
                "Help me write up {date} in my journal.\n\
                 1. Call capture_day for {date}, then get_entry on the draft it returns. Also read entries_on_date for {date}.\n\
                 2. Ask me two or three short questions about the day that the draft cannot answer (how it felt, what mattered).\n\
                 3. Rewrite the draft as a first-person entry in the language I answer in. Keep every time, name and number from the draft; add my answers. Save it with update_entry. Do not call accept_draft until I say the entry is right.\n\
                 4. Then pull out durable facts about my life (people, places, work, health, plans). For each, call recall to find what memory already says, and call remember with source set to the entry id and replaces set when it updates an older fact. Skip passing moods and one-off events.\n\
                 5. Finish with a short list of what you remembered."
            ),
        )])
    }

    #[prompt(name = "weekly_review", description = "Look back over the last seven days and what changed in memory.")]
    async fn weekly_review(&self, p: Parameters<DayPromptArgs>) -> Result<Vec<PromptMessage>, ErrorData> {
        let end = parse_date(p.0.date)?;
        let start = end - chrono::Duration::days(6);
        let week_ago = start.and_hms_opt(0, 0, 0).unwrap();
        Ok(vec![PromptMessage::new_text(
            Role::User,
            format!(
                "Review my week from {start} to {end}.\n\
                 Use list_entries with from={start} and to={end}, read the entries with get_entry, and check list_drafts for days I have not reviewed. \
                 Compare recall (today) with recall using known_at={week_ago}+00:00 to see what changed in my life this week. \
                 Write: what I spent time on, what moved forward, what I said I would follow up on, and what changed in memory. \
                 Cite entries by date. Do not write anything to the journal unless I ask."
            ),
        )])
    }
}

#[tool_handler(router = self.tool_router)]
#[prompt_handler(router = self.prompt_router)]
impl ServerHandler for JournalServer {
    fn get_info(&self) -> ServerConfig {
        let mode = if self.read_only { "read-only" } else { "read-write" };
        ServerConfig::new(ServerCapabilities::builder().enable_tools().enable_prompts().build())
            .with_server_info(Implementation::new("dayfile", env!("CARGO_PKG_VERSION")))
            .with_instructions(format!(
            "The user's journal and long-term memory, stored as Markdown files they own ({mode}). \
             Entries: search_entries or list_entries to find them, get_entry for a full body, on_this_day for anniversaries, append_to_today to add a line. \
             Days: capture_day drafts a day from git, calendar and photos; drafts wait for the user's review. \
             Memory: recall before answering questions about the user's life; remember durable facts with their source entry, and pass replaces when a fact changes. \
             Dates are YYYY-MM-DD in the user's local timezone."
        ))
    }
}

/// Run the server on stdin/stdout until the client disconnects.
pub async fn serve(store: Store, read_only: bool, capture: CaptureConfig) -> Result<()> {
    let service = JournalServer::new(store, read_only, capture).serve(stdio()).await.map_err(|e| anyhow!("mcp: {e}"))?;
    service.waiting().await?;
    Ok(())
}
