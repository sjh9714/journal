//! Just enough iCalendar to answer "what was on my calendar on this day":
//! VEVENT with DTSTART/DTEND, TZID, all-day dates, RRULE (DAILY, WEEKLY,
//! MONTHLY, YEARLY with INTERVAL, COUNT, UNTIL, BYDAY, BYMONTHDAY), EXDATE,
//! RECURRENCE-ID overrides and cancelled events.

use chrono::{DateTime, Datelike, Duration, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc, Weekday};
use chrono_tz::Tz;
use std::collections::{HashMap, HashSet};

/// One occurrence that falls on the asked-for day, in the journal's zone.
#[derive(Debug, Clone, PartialEq)]
pub struct Occurrence {
    pub uid: String,
    pub summary: String,
    pub location: Option<String>,
    /// None for all-day events.
    pub start: Option<DateTime<Tz>>,
    pub end: Option<DateTime<Tz>>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum When {
    Date(NaiveDate),
    /// Local wall time plus the zone it is in (None = floating, use the journal zone).
    Local(NaiveDateTime, Option<Tz>),
    Utc(NaiveDateTime),
}

impl When {
    fn to_zone(self, tz: Tz) -> Option<DateTime<Tz>> {
        match self {
            When::Date(_) => None,
            When::Utc(n) => Some(Utc.from_utc_datetime(&n).with_timezone(&tz)),
            When::Local(n, z) => z.unwrap_or(tz).from_local_datetime(&n).earliest().map(|d| d.with_timezone(&tz)),
        }
    }
    fn date(self) -> NaiveDate {
        match self {
            When::Date(d) => d,
            When::Local(n, _) | When::Utc(n) => n.date(),
        }
    }
    /// Same wall-clock time on another date (recurrences keep local time across DST).
    fn on(self, d: NaiveDate) -> When {
        match self {
            When::Date(_) => When::Date(d),
            When::Local(n, z) => When::Local(d.and_time(n.time()), z),
            When::Utc(n) => When::Utc(d.and_time(n.time())),
        }
    }
}

#[derive(Debug, Default)]
struct Event {
    uid: String,
    summary: String,
    location: Option<String>,
    start: Option<When>,
    end: Option<When>,
    rrule: Option<String>,
    exdates: Vec<When>,
    recurrence_id: Option<When>,
    cancelled: bool,
}

/// Events of `ics` that occur on `day` in zone `tz`, sorted by start (all-day first).
pub fn occurrences_on(ics: &str, day: NaiveDate, tz: Tz) -> Vec<Occurrence> {
    let events = parse(ics);
    // Overridden instances: (uid, original start) -> replaced by its own VEVENT.
    let overridden: HashSet<(String, NaiveDateTime)> = events
        .iter()
        .filter_map(|e| e.recurrence_id.map(|r| (e.uid.clone(), naive(r))))
        .collect();
    let mut out = Vec::new();
    for e in &events {
        let Some(start) = e.start else { continue };
        if e.cancelled {
            continue;
        }
        let length = e.end.and_then(|end| duration(start, end)).unwrap_or_else(|| match start {
            When::Date(_) => Duration::days(1),
            _ => Duration::zero(),
        });
        let candidates: Vec<When> = match (&e.rrule, e.recurrence_id) {
            (Some(rule), None) => {
                let ex: HashSet<NaiveDateTime> = e.exdates.iter().map(|w| naive(*w)).collect();
                // Local dates around `day` cover zone offsets of up to a day either way.
                expand(rule, start, day + Duration::days(1))
                    .into_iter()
                    .filter(|d| (*d - day).num_days().abs() <= 1)
                    .map(|d| start.on(d))
                    .filter(|w| !ex.contains(&naive(*w)) && !overridden.contains(&(e.uid.clone(), naive(*w))))
                    .collect()
            }
            _ => vec![start],
        };
        for w in candidates {
            let occ = match w {
                When::Date(d) => {
                    let days = length.num_days().max(1);
                    if day < d || day >= d + Duration::days(days) {
                        continue;
                    }
                    Occurrence { uid: e.uid.clone(), summary: e.summary.clone(), location: e.location.clone(), start: None, end: None }
                }
                _ => {
                    let Some(s) = w.to_zone(tz) else { continue };
                    if s.date_naive() != day {
                        continue;
                    }
                    let end = (length > Duration::zero()).then(|| s + length);
                    Occurrence { uid: e.uid.clone(), summary: e.summary.clone(), location: e.location.clone(), start: Some(s), end }
                }
            };
            out.push(occ);
        }
    }
    out.sort_by_key(|o| o.start.map(|s| s.timestamp()).unwrap_or(i64::MIN));
    out
}

fn naive(w: When) -> NaiveDateTime {
    match w {
        When::Date(d) => d.and_time(NaiveTime::MIN),
        When::Local(n, _) | When::Utc(n) => n,
    }
}

fn duration(a: When, b: When) -> Option<Duration> {
    match (a, b) {
        (When::Date(x), When::Date(y)) => Some(y - x),
        _ => Some(naive(b) - naive(a)),
    }
}

fn parse(ics: &str) -> Vec<Event> {
    // Unfold: a line starting with space or tab continues the previous one.
    let mut lines: Vec<String> = Vec::new();
    for raw in ics.lines() {
        let raw = raw.trim_end_matches('\r');
        if let Some(rest) = raw.strip_prefix([' ', '\t']) {
            if let Some(last) = lines.last_mut() {
                last.push_str(rest);
                continue;
            }
        }
        lines.push(raw.to_string());
    }
    let mut events = Vec::new();
    let mut cur: Option<Event> = None;
    for line in &lines {
        match line.as_str() {
            "BEGIN:VEVENT" => cur = Some(Event::default()),
            "END:VEVENT" => events.extend(cur.take()),
            _ => {
                let Some(e) = cur.as_mut() else { continue };
                let Some((head, value)) = line.split_once(':') else { continue };
                let mut parts = head.split(';');
                let name = parts.next().unwrap_or("").to_ascii_uppercase();
                let params: HashMap<String, String> = parts
                    .filter_map(|p| p.split_once('='))
                    .map(|(k, v)| (k.to_ascii_uppercase(), v.trim_matches('"').to_string()))
                    .collect();
                match name.as_str() {
                    "UID" => e.uid = value.to_string(),
                    "SUMMARY" => e.summary = unescape(value),
                    "LOCATION" => e.location = Some(unescape(value)).filter(|s| !s.is_empty()),
                    "DTSTART" => e.start = when(value, &params),
                    "DTEND" => e.end = when(value, &params),
                    "RRULE" => e.rrule = Some(value.to_string()),
                    "EXDATE" => e.exdates.extend(value.split(',').filter_map(|v| when(v, &params))),
                    "RECURRENCE-ID" => e.recurrence_id = when(value, &params),
                    "STATUS" => e.cancelled = value.eq_ignore_ascii_case("CANCELLED"),
                    _ => {}
                }
            }
        }
    }
    events
}

fn when(value: &str, params: &HashMap<String, String>) -> Option<When> {
    if params.get("VALUE").map(String::as_str) == Some("DATE") || value.len() == 8 {
        return NaiveDate::parse_from_str(value, "%Y%m%d").ok().map(When::Date);
    }
    if let Some(v) = value.strip_suffix('Z') {
        return NaiveDateTime::parse_from_str(v, "%Y%m%dT%H%M%S").ok().map(When::Utc);
    }
    let n = NaiveDateTime::parse_from_str(value, "%Y%m%dT%H%M%S").ok()?;
    let tz = params.get("TZID").and_then(|t| t.parse::<Tz>().ok());
    Some(When::Local(n, tz))
}

fn unescape(s: &str) -> String {
    s.replace("\\n", " ").replace("\\N", " ").replace("\\,", ",").replace("\\;", ";").replace("\\\\", "\\")
}

/// Local start dates of a recurring event, from its first one up to `until` inclusive.
fn expand(rule: &str, start: When, until: NaiveDate) -> Vec<NaiveDate> {
    let r: HashMap<String, String> = rule
        .split(';')
        .filter_map(|p| p.split_once('='))
        .map(|(k, v)| (k.to_ascii_uppercase(), v.to_string()))
        .collect();
    let first = start.date();
    let interval = r.get("INTERVAL").and_then(|v| v.parse::<i64>().ok()).unwrap_or(1).max(1);
    let count = r.get("COUNT").and_then(|v| v.parse::<usize>().ok());
    let end = r.get("UNTIL").and_then(|v| when(v, &HashMap::new())).map(|w| w.date()).unwrap_or(until).min(until);
    let byday: Vec<(Option<i32>, Weekday)> = r.get("BYDAY").map(|v| v.split(',').filter_map(parse_byday).collect()).unwrap_or_default();
    let bymonthday: Vec<i32> = r.get("BYMONTHDAY").map(|v| v.split(',').filter_map(|d| d.parse().ok()).collect()).unwrap_or_default();

    let mut out = Vec::new();
    let mut d = first;
    // Bounded: a daily rule over decades is still only a few thousand steps.
    for _ in 0..20_000 {
        if d > end || count.is_some_and(|c| out.len() >= c) {
            break;
        }
        if matches(r.get("FREQ").map(String::as_str).unwrap_or(""), interval, first, d, &byday, &bymonthday) {
            out.push(d);
        }
        d += Duration::days(1);
    }
    out
}

fn parse_byday(s: &str) -> Option<(Option<i32>, Weekday)> {
    let s = s.trim();
    let (n, wd) = s.split_at(s.len().checked_sub(2)?);
    let wd = match wd {
        "MO" => Weekday::Mon,
        "TU" => Weekday::Tue,
        "WE" => Weekday::Wed,
        "TH" => Weekday::Thu,
        "FR" => Weekday::Fri,
        "SA" => Weekday::Sat,
        "SU" => Weekday::Sun,
        _ => return None,
    };
    Some((if n.is_empty() { None } else { n.parse().ok() }, wd))
}

fn matches(freq: &str, interval: i64, first: NaiveDate, d: NaiveDate, byday: &[(Option<i32>, Weekday)], bymonthday: &[i32]) -> bool {
    let months = |a: NaiveDate, b: NaiveDate| (b.year() - a.year()) as i64 * 12 + b.month() as i64 - a.month() as i64;
    match freq {
        "DAILY" => (d - first).num_days() % interval == 0 && (byday.is_empty() || byday.iter().any(|(_, w)| *w == d.weekday())),
        "WEEKLY" => {
            let week = |x: NaiveDate| (x - Duration::days(x.weekday().num_days_from_monday() as i64)).num_days_from_ce() / 7;
            let on_day = if byday.is_empty() { d.weekday() == first.weekday() } else { byday.iter().any(|(_, w)| *w == d.weekday()) };
            on_day && (week(d) - week(first)) as i64 % interval == 0
        }
        "MONTHLY" => {
            if months(first, d) % interval != 0 {
                return false;
            }
            if !bymonthday.is_empty() {
                let last = last_day(d);
                return bymonthday.iter().any(|&m| if m > 0 { d.day() as i32 == m } else { d.day() as i32 == last as i32 + 1 + m });
            }
            if !byday.is_empty() {
                return byday.iter().any(|(n, w)| *w == d.weekday() && n.is_none_or(|n| nth_weekday_matches(d, n)));
            }
            d.day() == first.day()
        }
        "YEARLY" => (d.year() - first.year()) as i64 % interval == 0 && d.month() == first.month() && d.day() == first.day(),
        _ => d == first,
    }
}

fn last_day(d: NaiveDate) -> u32 {
    let (y, m) = if d.month() == 12 { (d.year() + 1, 1) } else { (d.year(), d.month() + 1) };
    NaiveDate::from_ymd_opt(y, m, 1).map(|n| (n - Duration::days(1)).day()).unwrap_or(28)
}

/// 2TU = second Tuesday, -1FR = last Friday of the month.
fn nth_weekday_matches(d: NaiveDate, n: i32) -> bool {
    if n > 0 {
        ((d.day() - 1) / 7 + 1) as i32 == n
    } else {
        ((last_day(d) - d.day()) / 7 + 1) as i32 == -n
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ICS: &str = "BEGIN:VCALENDAR\r
BEGIN:VEVENT\r
UID:standup\r
SUMMARY:Standup\r
DTSTART;TZID=Asia/Seoul:20261001T093000\r
DTEND;TZID=Asia/Seoul:20261001T094500\r
RRULE:FREQ=WEEKLY;BYDAY=MO,WE,FR\r
EXDATE;TZID=Asia/Seoul:20261030T093000\r
END:VEVENT\r
BEGIN:VEVENT\r
UID:standup\r
RECURRENCE-ID;TZID=Asia/Seoul:20261028T093000\r
SUMMARY:Standup (moved)\r
DTSTART;TZID=Asia/Seoul:20261028T110000\r
DTEND;TZID=Asia/Seoul:20261028T111500\r
END:VEVENT\r
BEGIN:VEVENT\r
UID:trip\r
SUMMARY:Busan trip\r
DTSTART;VALUE=DATE:20261027\r
DTEND;VALUE=DATE:20261029\r
END:VEVENT\r
BEGIN:VEVENT\r
UID:utc\r
SUMMARY:Call with\r
  London\r
DTSTART:20261028T010000Z\r
END:VEVENT\r
BEGIN:VEVENT\r
UID:review\r
SUMMARY:Monthly review\r
DTSTART;TZID=Asia/Seoul:20260101T170000\r
RRULE:FREQ=MONTHLY;BYDAY=-1WE;COUNT=12\r
END:VEVENT\r
BEGIN:VEVENT\r
UID:gone\r
SUMMARY:Cancelled thing\r
STATUS:CANCELLED\r
DTSTART;TZID=Asia/Seoul:20261028T150000\r
END:VEVENT\r
END:VCALENDAR\r
";

    fn names(day: (i32, u32, u32)) -> Vec<String> {
        let tz: Tz = "Asia/Seoul".parse().unwrap();
        occurrences_on(ICS, NaiveDate::from_ymd_opt(day.0, day.1, day.2).unwrap(), tz).into_iter().map(|o| o.summary).collect()
    }

    #[test]
    fn recurring_overrides_all_day_utc_and_cancelled() {
        // Wed 2026-10-28: trip day 2 of 2, the moved standup, the UTC call at 10:00 KST, last-Wednesday review.
        assert_eq!(names((2026, 10, 28)), ["Busan trip", "Call with London", "Standup (moved)", "Monthly review"]);
        // Fri 2026-10-30 is an EXDATE; Mon 2026-10-26 is a normal standup.
        assert_eq!(names((2026, 10, 30)), Vec::<String>::new());
        assert_eq!(names((2026, 10, 26)), ["Standup"]);
        // The all-day trip ends before its DTEND date.
        assert_eq!(names((2026, 10, 29)), Vec::<String>::new());
    }

    #[test]
    fn count_and_nth_weekday() {
        // Last Wednesday of each month, 12 times from January 2026: December 2026 is the 12th.
        assert!(names((2026, 12, 30)).contains(&"Monthly review".to_string()));
        assert!(!names((2027, 1, 27)).contains(&"Monthly review".to_string()));
    }
}
