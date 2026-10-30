//! MCP server over stdio. Every tool is a thin call into `dayfile_core::Store`.

use anyhow::{anyhow, Result};
use chrono::NaiveDate;
use dayfile_core::store::EntryPatch;
use dayfile_core::{today, Entry, EntrySummary, ListFilter, NewEntry, Store};
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::{Json, Parameters};
use rmcp::model::{ErrorData, Implementation, ServerCapabilities, ServerConfig};
use rmcp::transport::stdio;
use rmcp::{tool, tool_handler, tool_router, ServerHandler, ServiceExt};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct JournalServer {
    store: Arc<Mutex<Store>>,
    read_only: bool,
    tool_router: ToolRouter<Self>,
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
    pub fn new(store: Store, read_only: bool) -> Self {
        Self { store: Arc::new(Mutex::new(store)), read_only, tool_router: Self::tool_router() }
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
        let f = ListFilter { journal: p.journal, tag: p.tag, from, to, starred: p.starred, limit: Some(clamp(p.limit)), offset: p.offset.unwrap_or(0), ascending: false };
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
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for JournalServer {
    fn get_info(&self) -> ServerConfig {
        let mode = if self.read_only { "read-only" } else { "read-write" };
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("dayfile", env!("CARGO_PKG_VERSION")))
            .with_instructions(format!(
            "Personal journal stored as Markdown files ({mode}). Use search_entries or list_entries to find entries, \
             get_entry for a full body, on_this_day for anniversaries, and append_to_today to add to today's entry. \
             Dates are YYYY-MM-DD in the user's local timezone."
        ))
    }
}

/// Run the server on stdin/stdout until the client disconnects.
pub async fn serve(store: Store, read_only: bool) -> Result<()> {
    let service = JournalServer::new(store, read_only).serve(stdio()).await.map_err(|e| anyhow!("mcp: {e}"))?;
    service.waiting().await?;
    Ok(())
}
