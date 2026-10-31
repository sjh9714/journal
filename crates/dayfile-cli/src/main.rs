mod config;
mod mcp;

use anyhow::{bail, Context, Result};
use chrono::{Local, NaiveDate};
use clap::{Args, Parser, Subcommand};
use dayfile_core::store::EntryPatch;
use dayfile_core::{import_dayone, today, CaptureConfig, EntrySummary, ImportOptions, ListFilter, NewEntry, NewFact, Recall, Store, TopicFact};
use std::io::{IsTerminal, Read};
use std::path::PathBuf;

/// Your journal, as Markdown files on disk. Readable by you and your agents.
#[derive(Parser)]
#[command(name = "dayfile", version, about)]
struct Cli {
    /// Journal folder. Defaults to $DAYFILE_ROOT, then ~/Journal.
    #[arg(long, global = true, env = "DAYFILE_ROOT")]
    root: Option<PathBuf>,
    /// Output JSON instead of text.
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Create the journal folder layout.
    Init,
    /// Add an entry. Text from args, or stdin when omitted.
    Add(AddArgs),
    /// List entries, newest first.
    List(ListArgs),
    /// Show one entry by uuid or unique prefix.
    Show { id: String },
    /// Full-text search. Append * to a word for prefix match.
    Search {
        query: Vec<String>,
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
    /// Entries written today (or on --date).
    Today {
        #[arg(long)]
        date: Option<NaiveDate>,
    },
    /// Entries from this day in previous years.
    OnThisDay {
        #[arg(long)]
        date: Option<NaiveDate>,
    },
    /// Append text to an entry.
    Append { id: String, text: Vec<String> },
    /// Edit metadata of an entry.
    Edit(EditArgs),
    /// Move an entry to the trash (restorable for 30 days).
    Delete {
        id: String,
        /// Skip the trash and remove the file now.
        #[arg(long)]
        permanent: bool,
    },
    /// Trashed entries: list, restore, empty.
    #[command(subcommand)]
    Trash(TrashCmd),
    /// List templates (files in templates/).
    Templates,
    /// Streak, counts and words.
    Stats,
    /// List tags with counts.
    Tags,
    /// List journals with counts.
    Journals,
    /// Rebuild the search index from files.
    Reindex {
        #[arg(long)]
        full: bool,
    },
    /// Print where things live.
    Info,
    /// Draft a day from your git commits, calendar and photos (see `dayfile sources`).
    Capture {
        /// Day to capture, default today.
        #[arg(long)]
        date: Option<NaiveDate>,
    },
    /// Captured drafts waiting for review.
    Drafts,
    /// Keep a draft as an ordinary entry. Discard one with `dayfile delete`.
    Accept { id: String },
    /// Where capture looks: git folders, calendar feeds, photo folders.
    Sources(SourcesArgs),
    /// Record a fact about your life in memory/<topic>.md.
    Remember(RememberArgs),
    /// Facts that hold on a date (default today), optionally as known at a past moment.
    Recall(RecallArgs),
    /// Every fact of a topic, including ended and retracted ones.
    History { topic: String },
    /// Memory topics with how many facts currently hold.
    Topics,
    /// Mark a fact as never true. It stays in the file for history.
    Retract { id: String },
    /// Import from another app.
    #[command(subcommand)]
    Import(ImportCmd),
    /// Model Context Protocol server for AI agents.
    #[command(subcommand)]
    Mcp(McpCmd),
}

#[derive(Subcommand)]
enum McpCmd {
    /// Serve over stdio. Add to Claude Code with: claude mcp add dayfile -- dayfile mcp serve
    Serve {
        /// Expose only read tools.
        #[arg(long)]
        read_only: bool,
    },
}

#[derive(Subcommand)]
enum TrashCmd {
    /// Show what is in the trash.
    List,
    /// Put an entry back (uuid or prefix).
    Restore { id: String },
    /// Permanently delete everything in the trash.
    Empty,
}

#[derive(Args)]
struct SourcesArgs {
    #[command(subcommand)]
    cmd: Option<SourcesCmd>,
}

#[derive(Subcommand)]
enum SourcesCmd {
    /// Add a source: `git <folder>`, `calendar <.ics file or URL>`, `photos <folder>`.
    Add { kind: SourceKind, value: String },
    /// Remove a source.
    Remove { kind: SourceKind, value: String },
    /// Only capture commits by this email (default: git config user.email).
    Author { email: String },
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum SourceKind {
    Git,
    Calendar,
    Photos,
}

#[derive(Args)]
struct RememberArgs {
    topic: String,
    text: Vec<String>,
    /// Since when it is true (YYYY-MM-DD). Also where replaced facts end.
    #[arg(long)]
    since: Option<NaiveDate>,
    /// Entry the fact comes from (uuid or prefix).
    #[arg(long)]
    source: Option<String>,
    /// Ids of facts this one supersedes.
    #[arg(long)]
    replaces: Vec<String>,
}

#[derive(Args)]
struct RecallArgs {
    query: Vec<String>,
    #[arg(long)]
    topic: Option<String>,
    #[arg(long)]
    as_of: Option<NaiveDate>,
    /// RFC 3339 moment: answer with what the journal knew then.
    #[arg(long)]
    known_at: Option<String>,
}

#[derive(Subcommand)]
enum ImportCmd {
    /// Import a Day One JSON export (.zip or extracted folder).
    Dayone {
        path: PathBuf,
        /// Put everything in this journal instead of one per export file.
        #[arg(long)]
        journal: Option<String>,
        /// Report what would be imported without writing.
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(Args)]
struct AddArgs {
    text: Vec<String>,
    #[arg(short, long)]
    tag: Vec<String>,
    #[arg(short, long)]
    journal: Option<String>,
    /// Override creation time, RFC 3339 (e.g. 2026-10-28T21:00:00+09:00).
    #[arg(long)]
    at: Option<String>,
    #[arg(long)]
    starred: bool,
    /// Start from a template (see `dayfile templates`).
    #[arg(long)]
    template: Option<String>,
}

#[derive(Args)]
struct ListArgs {
    #[arg(short, long)]
    journal: Option<String>,
    #[arg(short, long)]
    tag: Option<String>,
    #[arg(long)]
    from: Option<NaiveDate>,
    #[arg(long)]
    to: Option<NaiveDate>,
    #[arg(long)]
    starred: bool,
    #[arg(short = 'n', long, default_value_t = 20)]
    limit: usize,
    /// Print only the number of matching entries.
    #[arg(long)]
    count: bool,
}

#[derive(Args)]
struct EditArgs {
    id: String,
    #[arg(long)]
    tag: Option<Vec<String>>,
    #[arg(long)]
    journal: Option<String>,
    #[arg(long)]
    star: bool,
    #[arg(long)]
    unstar: bool,
}

/// --root, then $DAYFILE_ROOT (clap), then the folder the desktop app uses,
/// then ~/Journal. Keeps `dayfile` and the app on the same journal by default.
fn default_root() -> PathBuf {
    if let Some(root) = config::root() {
        return root;
    }
    dirs::home_dir().map(|h| h.join("Journal")).unwrap_or_else(|| PathBuf::from("Journal"))
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let root = cli.root.clone().unwrap_or_else(default_root);

    if let Cmd::Init = cli.cmd {
        let store = Store::init(&root)?;
        println!("initialized journal at {}", store.root().display());
        return Ok(());
    }
    if let Cmd::Sources(a) = &cli.cmd {
        return sources(a.cmd.as_ref(), cli.json);
    }

    let mut store = Store::open(&root)
        .with_context(|| format!("no journal at {}. Run `dayfile init` or pass --root.", root.display()))?;

    match cli.cmd {
        Cmd::Init => unreachable!(),
        Cmd::Add(a) => {
            let body = if a.template.is_some() && a.text.is_empty() && std::io::stdin().is_terminal() { String::new() } else { text_or_stdin(a.text)? };
            if body.trim().is_empty() && a.template.is_none() {
                bail!("nothing to add");
            }
            let created = a.at.as_deref().map(chrono::DateTime::parse_from_rfc3339).transpose()?;
            let new = NewEntry { body, created, tags: a.tag, journal: a.journal, starred: a.starred, ..Default::default() };
            let e = match a.template {
                Some(t) => store.create_from_template(&t, new)?,
                None => store.create(new)?,
            };
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&e)?);
            } else {
                println!("{}  {}", short(&e.meta.uuid), store.root().join(&e.path).display());
            }
        }
        Cmd::List(l) => {
            let f = ListFilter {
                journal: l.journal,
                tag: l.tag,
                from: l.from,
                to: l.to,
                starred: if l.starred { Some(true) } else { None },
                limit: if l.count { None } else { Some(l.limit) },
                ..Default::default()
            };
            if l.count {
                println!("{}", store.count(&f)?);
            } else {
                print_summaries(&store.list(&f)?, cli.json)?;
            }
        }
        Cmd::Show { id } => {
            let e = store.get(&id)?;
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&e)?);
            } else {
                let m = &e.meta;
                println!("uuid:     {}", m.uuid);
                println!("created:  {}  ({})", m.created.to_rfc3339(), m.timezone);
                println!("journal:  {}", m.journal);
                if !m.tags.is_empty() {
                    println!("tags:     {}", m.tags.join(", "));
                }
                if m.starred {
                    println!("starred:  yes");
                }
                if m.is_draft() {
                    println!("status:   draft (dayfile accept {} to keep it)", short(&m.uuid));
                }
                println!("file:     {}", store.root().join(&e.path).display());
                println!();
                print!("{}", e.body);
                if !e.body.ends_with('\n') {
                    println!();
                }
            }
        }
        Cmd::Search { query, limit } => {
            let q = query.join(" ");
            print_summaries(&store.search(&q, limit)?, cli.json)?;
        }
        Cmd::Today { date } => {
            let d = date.unwrap_or_else(today);
            print_summaries(&store.on_date(d)?, cli.json)?;
        }
        Cmd::OnThisDay { date } => {
            let d = date.unwrap_or_else(today);
            let rows = store.on_this_day(d)?;
            if rows.is_empty() && !cli.json {
                println!("nothing on {} in other years", d.format("%m-%d"));
            } else {
                print_summaries(&rows, cli.json)?;
            }
        }
        Cmd::Append { id, text } => {
            let t = text_or_stdin(text)?;
            let e = store.append(&id, t.trim_end())?;
            println!("{}  appended", short(&e.meta.uuid));
        }
        Cmd::Edit(a) => {
            let starred = match (a.star, a.unstar) {
                (true, false) => Some(true),
                (false, true) => Some(false),
                (true, true) => bail!("--star and --unstar are exclusive"),
                _ => None,
            };
            let e = store.update(&a.id, EntryPatch { tags: a.tag, journal: a.journal, starred, ..Default::default() })?;
            println!("{}  updated", short(&e.meta.uuid));
        }
        Cmd::Delete { id, permanent } => {
            if permanent {
                let e = store.delete_permanent(&id)?;
                println!("{}  deleted permanently {}", short(&e.meta.uuid), e.path.display());
            } else {
                let e = store.delete(&id)?;
                println!("{}  moved to trash (dayfile trash restore {} to undo)", short(&e.meta.uuid), short(&e.meta.uuid));
            }
        }
        Cmd::Trash(TrashCmd::List) => {
            let rows = store.trash_list()?;
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&rows)?);
            } else if rows.is_empty() {
                println!("trash is empty");
            } else {
                for t in rows {
                    println!("{}  deleted {}  written {}  {}", short(&t.uuid), t.deleted_at.with_timezone(&Local).format("%Y-%m-%d"), t.created.format("%Y-%m-%d"), if t.title.is_empty() { "(untitled)".to_string() } else { t.title });
                }
            }
        }
        Cmd::Trash(TrashCmd::Restore { id }) => {
            let e = store.restore(&id)?;
            println!("{}  restored to {}", short(&e.meta.uuid), e.path.display());
        }
        Cmd::Trash(TrashCmd::Empty) => {
            println!("removed {} files", store.empty_trash()?);
        }
        Cmd::Templates => {
            for t in store.templates()? {
                let extra = [
                    (!t.tags.is_empty()).then(|| format!("tags: {}", t.tags.join(", "))),
                    t.journal.as_ref().map(|j| format!("journal: {j}")),
                ]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join("  ");
                println!("{:<16} {}{}", t.slug, t.name, if extra.is_empty() { String::new() } else { format!("  ({extra})") });
            }
        }
        Cmd::Stats => {
            let s = store.stats(today())?;
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&s)?);
            } else {
                println!("entries: {}  this month: {}  this year: {}", s.total, s.this_month, s.this_year);
                println!("streak: {} days  (longest {})", s.streak_days, s.longest_streak);
                println!("words: {}", s.words);
                if let (Some(f), Some(l)) = (s.first_entry, s.last_entry) {
                    println!("from {f} to {l}");
                }
            }
        }
        Cmd::Tags => {
            for (t, n) in store.tags()? {
                println!("{n:5}  {t}");
            }
        }
        Cmd::Journals => {
            for (j, n) in store.journals()? {
                println!("{n:5}  {j}");
            }
        }
        Cmd::Reindex { full } => {
            let s = store.reindex(full)?;
            println!("added {} updated {} removed {} unchanged {} skipped {}", s.added, s.updated, s.removed, s.unchanged, s.skipped);
            for p in store.problems() {
                println!("  skipped {}: {}", p.path, p.reason);
            }
        }
        Cmd::Import(ImportCmd::Dayone { path, journal, dry_run }) => {
            let r = import_dayone(&mut store, &path, &ImportOptions { journal, dry_run })?;
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&r)?);
            } else {
                let verb = if dry_run { "would import" } else { "imported" };
                println!("{verb} {} of {} entries into {} ({} already present, {} attachments, {} missing media)",
                    r.created, r.entries, r.journals.join(", "), r.already_present, r.attachments, r.missing_media);
            }
        }
        Cmd::Mcp(McpCmd::Serve { read_only }) => {
            tokio::runtime::Runtime::new()?.block_on(mcp::serve(store, read_only, config::capture()))?;
        }
        Cmd::Sources(_) => unreachable!(),
        Cmd::Capture { date } => {
            let cfg = config::capture();
            if cfg.is_empty() {
                bail!("no capture sources yet. Add one, e.g. `dayfile sources add git ~/code`");
            }
            let r = store.capture(date.unwrap_or_else(today), &cfg)?;
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&r)?);
            } else {
                for e in &r.errors {
                    eprintln!("warning: {e}");
                }
                match &r.draft {
                    Some(d) if r.added > 0 => println!("{} {}  {} added  {}", r.date, short(&d.uuid), r.describe(), store.root().join(&d.path).display()),
                    Some(d) => println!("{} {}  nothing new since the last capture", r.date, short(&d.uuid)),
                    None => println!("{}  nothing to capture", r.date),
                }
            }
        }
        Cmd::Drafts => print_summaries(&store.drafts()?, cli.json)?,
        Cmd::Accept { id } => {
            let e = store.accept_draft(&id)?;
            println!("{}  kept", short(&e.meta.uuid));
        }
        Cmd::Remember(a) => {
            let source = a.source.map(|s| store.get(&s).map(|e| e.meta.uuid)).transpose()?;
            let f = store.remember(NewFact { topic: a.topic, text: text_or_stdin(a.text)?, valid_from: a.since, source, replaces: a.replaces })?;
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&f)?);
            } else {
                println!("{}  {}: {}", f.fact.id, f.topic, f.fact.text);
            }
        }
        Cmd::Recall(a) => {
            let known_at = a.known_at.as_deref().map(chrono::DateTime::parse_from_rfc3339).transpose()?;
            let q = (!a.query.is_empty()).then(|| a.query.join(" "));
            print_facts(&store.recall(&Recall { topic: a.topic, query: q, as_of: a.as_of, known_at })?, cli.json)?;
        }
        Cmd::History { topic } => print_facts(&store.fact_history(&topic)?, cli.json)?,
        Cmd::Topics => {
            for (t, n) in store.topics()? {
                println!("{n:5}  {t}");
            }
        }
        Cmd::Retract { id } => {
            let f = store.retract(&id)?;
            println!("{}  retracted: {}", f.fact.id, f.fact.text);
        }
        Cmd::Info => {
            println!("root:   {}", store.root().display());
            println!("index:  {}", Store::index_path(store.root()).display());
            println!("entries: {}", store.count(&ListFilter::default())?);
        }
    }
    Ok(())
}

fn sources(cmd: Option<&SourcesCmd>, json: bool) -> Result<()> {
    let mut cfg: CaptureConfig = config::capture();
    let Some(cmd) = cmd else {
        if json {
            println!("{}", serde_json::to_string_pretty(&cfg)?);
        } else if cfg.is_empty() {
            println!("no sources. Try: dayfile sources add git ~/code");
        } else {
            cfg.git.iter().for_each(|p| println!("git       {}", p.display()));
            if let Some(a) = &cfg.git_author {
                println!("author    {a}");
            }
            cfg.calendars.iter().for_each(|c| println!("calendar  {c}"));
            cfg.photos.iter().for_each(|p| println!("photos    {}", p.display()));
        }
        return Ok(());
    };
    match cmd {
        SourcesCmd::Add { kind, value } => match kind {
            SourceKind::Git if !cfg.git.contains(&PathBuf::from(value)) => cfg.git.push(value.into()),
            SourceKind::Calendar if !cfg.calendars.contains(value) => cfg.calendars.push(value.clone()),
            SourceKind::Photos if !cfg.photos.contains(&PathBuf::from(value)) => cfg.photos.push(value.into()),
            _ => {}
        },
        SourcesCmd::Remove { kind, value } => match kind {
            SourceKind::Git => cfg.git.retain(|p| p != &PathBuf::from(value)),
            SourceKind::Calendar => cfg.calendars.retain(|c| c != value),
            SourceKind::Photos => cfg.photos.retain(|p| p != &PathBuf::from(value)),
        },
        SourcesCmd::Author { email } => cfg.git_author = Some(email.clone()),
    }
    let path = config::save_capture(&cfg)?;
    println!("saved to {}", path.display());
    Ok(())
}

fn print_facts(rows: &[TopicFact], json: bool) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(rows)?);
        return Ok(());
    }
    for r in rows {
        let f = &r.fact;
        let span = match (f.valid_from, f.valid_to) {
            (None, None) => String::new(),
            (Some(a), None) => format!("  [{a} ..]"),
            (a, Some(b)) => format!("  [{} .. {b})", a.map(|d| d.to_string()).unwrap_or_default()),
        };
        let gone = if f.retracted_at.is_some() { "  (retracted)" } else { "" };
        println!("{}  {}: {}{span}{gone}", f.id, r.topic, f.text);
    }
    Ok(())
}

fn text_or_stdin(args: Vec<String>) -> Result<String> {
    if !args.is_empty() {
        return Ok(args.join(" "));
    }
    let mut stdin = std::io::stdin();
    if stdin.is_terminal() {
        bail!("pass text as arguments or pipe it on stdin");
    }
    let mut s = String::new();
    stdin.read_to_string(&mut s)?;
    Ok(s)
}

fn short(uuid: &str) -> String {
    uuid.chars().take(8).collect()
}

fn print_summaries(rows: &[EntrySummary], json: bool) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(rows)?);
        return Ok(());
    }
    for r in rows {
        let star = if r.starred { "*" } else { " " };
        let tags = if r.tags.is_empty() { String::new() } else { format!("  [{}]", r.tags.join(", ")) };
        let title = if r.title.chars().count() > 60 {
            format!("{}…", r.title.chars().take(60).collect::<String>())
        } else {
            r.title.clone()
        };
        println!("{} {}{}  {}{}", r.date, short(&r.uuid), star, title, tags);
    }
    Ok(())
}
