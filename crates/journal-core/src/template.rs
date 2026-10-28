//! Entry templates: `templates/<slug>.md` with frontmatter `name`, optional
//! `tags`/`journal`, and a body that may use `{{date}}`, `{{weekday}}`,
//! `{{time}}`, `{{year}}`.

use crate::store::Store;
use crate::Result;
use chrono::{DateTime, Datelike, FixedOffset};
use serde::{Deserialize, Serialize};

pub const TEMPLATES_DIR: &str = "templates";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Template {
    /// File stem, e.g. `gratitude`.
    pub slug: String,
    pub name: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub journal: Option<String>,
    pub body: String,
}

#[derive(Deserialize)]
struct Head {
    name: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    journal: Option<String>,
}

pub const DEFAULTS: &[(&str, &str)] = &[
    (
        "gratitude",
        "---\nname: Gratitude\ntags: [gratitude]\n---\n# {{weekday}}, {{date}}\n\nThree things I am grateful for today:\n\n1. \n2. \n3. \n\nOne thing I will do better tomorrow:\n\n",
    ),
    (
        "weekly-review",
        "---\nname: Weekly review\ntags: [review]\n---\n# Week of {{date}}\n\n## What went well\n\n- \n\n## What did not\n\n- \n\n## Next week, one thing\n\n- \n",
    ),
    (
        "daily",
        "---\nname: Daily\n---\n# {{weekday}}\n\n**Morning**\n\n\n**Afternoon**\n\n\n**Evening**\n\n",
    ),
];

fn split(text: &str) -> (Option<&str>, &str) {
    let t = text.strip_prefix('\u{feff}').unwrap_or(text);
    let Some(rest) = t.strip_prefix("---\n") else { return (None, t) };
    let Some(end) = rest.find("\n---\n") else { return (None, t) };
    let yaml = &rest[..end];
    let body = &rest[end + 5..];
    (Some(yaml), body.strip_prefix('\n').unwrap_or(body))
}

pub fn render(body: &str, now: DateTime<FixedOffset>) -> String {
    body.replace("{{date}}", &now.format("%Y-%m-%d").to_string())
        .replace("{{weekday}}", &now.format("%A").to_string())
        .replace("{{time}}", &now.format("%H:%M").to_string())
        .replace("{{year}}", &now.year().to_string())
}

impl Store {
    /// Write the default templates if the folder does not exist yet.
    pub fn ensure_default_templates(&self) -> Result<()> {
        let dir = self.root().join(TEMPLATES_DIR);
        if dir.exists() {
            return Ok(());
        }
        std::fs::create_dir_all(&dir)?;
        for (slug, text) in DEFAULTS {
            std::fs::write(dir.join(format!("{slug}.md")), text)?;
        }
        Ok(())
    }

    pub fn templates(&self) -> Result<Vec<Template>> {
        let dir = self.root().join(TEMPLATES_DIR);
        let mut out = Vec::new();
        let Ok(rd) = std::fs::read_dir(&dir) else { return Ok(out) };
        for item in rd.filter_map(|e| e.ok()) {
            let path = item.path();
            if path.extension().and_then(|e| e.to_str()) != Some("md") {
                continue;
            }
            let slug = path.file_stem().and_then(|s| s.to_str()).unwrap_or_default().to_string();
            let Ok(text) = std::fs::read_to_string(&path) else { continue };
            let (yaml, body) = split(&text);
            let head: Head = yaml.and_then(|y| serde_norway::from_str(y).ok()).unwrap_or(Head { name: None, tags: Vec::new(), journal: None });
            out.push(Template {
                name: head.name.unwrap_or_else(|| slug.clone()),
                slug,
                tags: head.tags,
                journal: head.journal,
                body: body.to_string(),
            });
        }
        out.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(out)
    }

    pub fn template(&self, slug: &str) -> Result<Template> {
        self.templates()?
            .into_iter()
            .find(|t| t.slug == slug)
            .ok_or_else(|| crate::Error::NotFound(format!("template {slug}")))
    }
}
