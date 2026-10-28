//! journal-core: a Markdown-file journal with a disposable SQLite index.
//!
//! Every entry is one Markdown file with YAML frontmatter under
//! `<root>/entries/YYYY/YYYY-MM-DD-<uuid8>.md`. Attachments live under
//! `<root>/attachments/`. The SQLite index can be rebuilt from the files at
//! any time and lives outside the journal folder so folder-sync tools never
//! touch it.
//!
//! All operations are exposed through [`Store`]; the CLI, the MCP server and
//! the desktop app are thin layers over it.

pub mod dayone;
pub mod entry;
pub mod error;
pub mod frontmatter;
pub mod index;
pub mod stats;
pub mod store;
pub mod template;
pub mod trash;

pub use dayone::{import_dayone, ImportOptions, ImportReport};
pub use entry::{today, Entry, EntrySummary, Location, NewEntry, Weather};
pub use error::{Error, Result};
pub use stats::Stats;
pub use store::{ListFilter, Problem, Store};
pub use template::Template;
pub use trash::TrashedEntry;
