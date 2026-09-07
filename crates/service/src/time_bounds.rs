use chrono::{Duration, Local, LocalResult, TimeZone};

pub(crate) const DAY_SECONDS: i64 = 24 * 60 * 60;

pub(crate) fn local_day_bounds_ts() -> Result<(i64, i64), String> {
    let now = Local::now();
    let today = now.date_naive();
    let start_naive = today
        .and_hms_opt(0, 0, 0)
        .ok_or_else(|| "build local start-of-day failed".to_string())?;
    let tomorrow_naive = (today + Duration::days(1))
        .and_hms_opt(0, 0, 0)
        .ok_or_else(|| "build local end-of-day failed".to_string())?;
    let start = match Local.from_local_datetime(&start_naive) {
        LocalResult::Single(value) => value.timestamp(),
        LocalResult::Ambiguous(a, b) => a.timestamp().min(b.timestamp()),
        LocalResult::None => now.timestamp(),
    };
    let end = match Local.from_local_datetime(&tomorrow_naive) {
        LocalResult::Single(value) => value.timestamp(),
        LocalResult::Ambiguous(a, b) => a.timestamp().max(b.timestamp()),
        LocalResult::None => start + DAY_SECONDS,
    };
    Ok((start, end.max(start)))
}

pub(crate) fn resolve_optional_utc_day_bounds_ts(
    day_start_ts: Option<i64>,
    day_end_ts: Option<i64>,
    now_ts: i64,
) -> (i64, i64) {
    match (
        day_start_ts.filter(|value| *value > 0),
        day_end_ts.filter(|value| *value > 0),
    ) {
        (Some(start), Some(end)) if end > start => (start, end),
        _ => {
            let start = now_ts - now_ts.rem_euclid(DAY_SECONDS);
            (start, start + DAY_SECONDS)
        }
    }
}

pub(crate) fn resolve_bounded_local_day_bounds_ts(
    day_start_ts: Option<i64>,
    day_end_ts: Option<i64>,
    max_range_seconds: i64,
) -> Result<(i64, i64), String> {
    match (day_start_ts, day_end_ts) {
        (Some(start), Some(end)) => {
            if end <= start {
                return Err("dayEndTs must be greater than dayStartTs".to_string());
            }
            if end - start > max_range_seconds {
                return Err("requested day range is too large".to_string());
            }
            Ok((start, end))
        }
        (None, None) => local_day_bounds_ts(),
        _ => Err("dayStartTs and dayEndTs must be provided together".to_string()),
    }
}
