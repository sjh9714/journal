//! Small numbers for the home screen: streak, counts, words.

use crate::store::Store;
use crate::Result;
use chrono::{Duration, NaiveDate};
use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq, Eq, Default)]
pub struct Stats {
    pub total: usize,
    pub this_month: usize,
    pub this_year: usize,
    /// Consecutive days with at least one entry, ending today or yesterday.
    pub streak_days: usize,
    pub longest_streak: usize,
    pub words: usize,
    pub first_entry: Option<NaiveDate>,
    pub last_entry: Option<NaiveDate>,
}

/// Whitespace tokens, plus one per CJK character (which has no spaces).
pub fn count_words(text: &str) -> usize {
    let mut n = 0;
    let mut in_word = false;
    for c in text.chars() {
        let cjk = matches!(c as u32, 0x3040..=0x30FF | 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xAC00..=0xD7AF | 0xF900..=0xFAFF);
        if cjk {
            n += 1;
            in_word = false;
        } else if c.is_whitespace() {
            in_word = false;
        } else if !in_word {
            in_word = true;
            n += 1;
        }
    }
    n
}

fn streaks(dates: &[NaiveDate], today: NaiveDate) -> (usize, usize) {
    // dates: distinct, descending
    let mut longest = 0;
    let mut run = 0;
    let mut prev: Option<NaiveDate> = None;
    for &d in dates {
        run = match prev {
            Some(p) if p - d == Duration::days(1) => run + 1,
            _ => 1,
        };
        longest = longest.max(run);
        prev = Some(d);
    }
    let current = match dates.first() {
        Some(&d) if d == today || d == today - Duration::days(1) => {
            let mut c = 1;
            for w in dates.windows(2) {
                if w[0] - w[1] == Duration::days(1) {
                    c += 1;
                } else {
                    break;
                }
            }
            c
        }
        _ => 0,
    };
    (current, longest)
}

impl Store {
    pub fn stats(&self, today: NaiveDate) -> Result<Stats> {
        let (dates, words, total) = self.index_ref().dates_and_words()?;
        let (streak_days, longest_streak) = streaks(&dates, today);
        let month = today.format("%Y-%m").to_string();
        let year = today.format("%Y").to_string();
        Ok(Stats {
            total,
            this_month: self.index_ref().count_prefix(&month)?,
            this_year: self.index_ref().count_prefix(&year)?,
            streak_days,
            longest_streak,
            words,
            first_entry: dates.last().copied(),
            last_entry: dates.first().copied(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_count_cjk_per_char() {
        assert_eq!(count_words("hello world"), 2);
        assert_eq!(count_words("오늘은 좋은 날"), 6);
        assert_eq!(count_words("mix 한글 and 日本語"), 7);
        assert_eq!(count_words(""), 0);
    }

    #[test]
    fn streak_math() {
        let d = |s: &str| NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap();
        let today = d("2026-10-29");
        assert_eq!(streaks(&[d("2026-10-29"), d("2026-10-28"), d("2026-10-27"), d("2026-10-20")], today), (3, 3));
        assert_eq!(streaks(&[d("2026-10-28"), d("2026-10-27")], today), (2, 2), "yesterday keeps the streak alive");
        assert_eq!(streaks(&[d("2026-10-26")], today), (0, 1));
        assert_eq!(streaks(&[], today), (0, 0));
    }
}
