# Coding standards

Read during review. A rule a tool can check belongs in `clippy.toml`, a test or
`scripts/check.sh` instead.

## Text from outside the journal

Calendar titles, commit subjects and feed contents reach Markdown that the app
renders and agents read, so each one is untrusted data.

- Every captured string passes through `plain_words` or `commit_words` in
  `crates/dayfile-core/src/capture.rs` before it enters a draft.
- A sanitizer fix closes the whole class of input, and the reported case joins
  the table test `hostile_text_stays_plain` along with its neighbours.
- Errors name the host of a source, never its path or query. Feed URLs carry
  secret tokens.
- Anything read from outside has a size or count limit.
