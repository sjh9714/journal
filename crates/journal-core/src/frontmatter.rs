//! Split and join `---` YAML frontmatter and a Markdown body.

use crate::entry::Frontmatter;
use crate::{Error, Result};
use std::path::Path;

const FENCE: &str = "---";

pub fn parse(path: &Path, text: &str) -> Result<(Frontmatter, String)> {
    let err = |msg: &str| Error::Frontmatter {
        path: path.to_path_buf(),
        msg: msg.to_string(),
    };
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut lines = text.split_inclusive('\n');
    match lines.next().map(|l| l.trim_end()) {
        Some(FENCE) => {}
        _ => return Err(err("missing opening ---")),
    }
    let mut yaml = String::new();
    let mut closed = false;
    for line in lines.by_ref() {
        if line.trim_end() == FENCE {
            closed = true;
            break;
        }
        yaml.push_str(line);
    }
    if !closed {
        return Err(err("missing closing ---"));
    }
    let body: String = lines.collect();
    let body = body.strip_prefix('\n').unwrap_or(&body).to_string();
    let meta: Frontmatter = serde_norway::from_str(&yaml).map_err(|e| err(&e.to_string()))?;
    Ok((meta, body))
}

pub fn render(meta: &Frontmatter, body: &str) -> Result<String> {
    let yaml = serde_norway::to_string(meta)?;
    let mut out = String::with_capacity(yaml.len() + body.len() + 16);
    out.push_str(FENCE);
    out.push('\n');
    out.push_str(&yaml);
    if !yaml.ends_with('\n') {
        out.push('\n');
    }
    out.push_str(FENCE);
    out.push('\n');
    out.push('\n');
    out.push_str(body);
    Ok(out)
}
