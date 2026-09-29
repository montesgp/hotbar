//! Codex usage reader: every `*.jsonl` under `~/.codex/sessions`, last record
//! per session wins.
//!
//! `payload.turn_token_usage` (and its terminal sibling
//! `payload.info.total_token_usage`) is the running SESSION total on every
//! record, not a per-turn delta: summing them inflates a session by an order
//! of magnitude (verified on a real session: 21 records, input_tokens
//! strictly increasing from 30 796 to 1 356 488, the last record already
//! equal to the thread total). So only the LAST usage-bearing record in each
//! file is kept.
//!
//! Same windowing rule as the claude reader: the month total and the
//! per-project bucket are both gated by the session's last timestamp, so an
//! out-of-window session never leaks into either.

use crate::usage::{
    find_jsonl_files, normalize_project_path, pricing, project_display_name, AgentUsageReport,
    CostBasis, ProjectUsage, UsageTotals,
};
use chrono::{DateTime, Local};
use serde_json::Value;
use std::io::{BufRead, BufReader};
use std::path::Path;

const AGENT: &str = "codex";

struct Bucket {
    totals: UsageTotals,
    model: String,
}

impl Bucket {
    fn new() -> Self {
        Self { totals: UsageTotals::default(), model: String::new() }
    }
}

/// Reads month/window usage from every codex session file under `root`,
/// pricing against `overrides` layered on top of the built-in table.
pub fn read_usage(
    root: &Path,
    start: DateTime<Local>,
    end: DateTime<Local>,
    overrides: &pricing::PriceOverrides,
) -> AgentUsageReport {
    if !root.exists() {
        return AgentUsageReport::not_installed(AGENT, format!("missing directory: {}", root.display()));
    }

    let files = find_jsonl_files(root);
    if files.is_empty() {
        return AgentUsageReport::ok(AGENT, UsageTotals::default(), Vec::new(), "no session files yet");
    }

    let mut month = Bucket::new();
    let mut projects: std::collections::HashMap<String, Bucket> = std::collections::HashMap::new();
    let mut files_scanned = 0u64;
    let mut no_cwd = 0u64;

    for file in &files {
        let Ok(handle) = std::fs::File::open(file) else { continue };
        files_scanned += 1;
        let reader = BufReader::new(handle);

        let mut last_usage: Option<Value> = None;
        let mut last_ts: Option<DateTime<Local>> = None;
        let mut file_cwd = String::new();
        let mut file_model = String::new();

        for line in reader.lines() {
            let Ok(line) = line else { continue };
            if !line.contains("token_usage") && !line.contains("\"cwd\"") && !line.contains("\"model\"") {
                continue;
            }
            // A malformed line is skipped, never fatal to the file.
            let Ok(entry) = serde_json::from_str::<Value>(&line) else { continue };
            let Some(payload) = entry.get("payload") else { continue };

            if let Some(cwd) = payload.get("cwd").and_then(Value::as_str) {
                file_cwd = normalize_project_path(cwd);
            }
            if let Some(model) = payload.get("model").and_then(Value::as_str) {
                let model = model.trim();
                if !model.is_empty() && !model.starts_with('<') {
                    file_model = model.to_string();
                }
            }

            let usage = payload
                .get("turn_token_usage")
                .or_else(|| payload.get("info").and_then(|i| i.get("total_token_usage")))
                .or_else(|| payload.get("usage"));
            let Some(usage) = usage else { continue };

            last_usage = Some(usage.clone());
            last_ts = entry
                .get("timestamp")
                .and_then(Value::as_str)
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                .map(|dt| dt.with_timezone(&Local));
        }

        let Some(usage) = last_usage else { continue };

        let stamp = match last_ts {
            Some(ts) => ts,
            None => {
                let mtime = std::fs::metadata(file).and_then(|m| m.modified()).ok();
                match mtime {
                    Some(mtime) => DateTime::<Local>::from(mtime),
                    None => continue,
                }
            }
        };
        if stamp < start || stamp >= end {
            continue;
        }

        let input = get_u64(&usage, "input_tokens");
        let output = get_u64(&usage, "output_tokens");
        let cached = get_u64(&usage, "cached_input_tokens");
        let cache_write = get_u64(&usage, "cache_write_input_tokens");
        let reasoning = get_u64(&usage, "reasoning_output_tokens");
        let model = if file_model.is_empty() { None } else { Some(file_model.as_str()) };

        add_entry(&mut month, input, output, cached, cache_write, reasoning, model);

        if !file_cwd.is_empty() {
            let bucket = projects.entry(file_cwd).or_insert_with(Bucket::new);
            add_entry(bucket, input, output, cached, cache_write, reasoning, model);
        } else {
            no_cwd += 1;
        }
    }

    if files_scanned == 0 {
        return AgentUsageReport::error(AGENT, format!("no session files readable under {}", root.display()));
    }

    let mut month_totals = month.totals;
    if month_totals.entries > 0 {
        let uncached = month_totals.input_tokens.saturating_sub(month_totals.cache_read_tokens);
        // Codex's store carries one flat cache-write counter with no TTL
        // split, unlike claude's `cache_creation`; it is priced as a 5-minute
        // write, and OpenAI's rows price both TTL buckets identically anyway
        // (see pricing.rs), so this never under- or over-charges it.
        let est = pricing::estimate_cost(
            &month.model,
            uncached,
            month_totals.output_tokens,
            month_totals.cache_read_tokens,
            month_totals.cache_write_tokens,
            0,
            overrides,
        );
        month_totals.cost = est.amount;
        month_totals.cost_basis = est.amount.map(|_| CostBasis::ApiEquivalent);
    }

    let mut projects_out = Vec::with_capacity(projects.len());
    for (key, bucket) in projects {
        let mut totals = bucket.totals;
        let uncached = totals.input_tokens.saturating_sub(totals.cache_read_tokens);
        let est = pricing::estimate_cost(
            &bucket.model,
            uncached,
            totals.output_tokens,
            totals.cache_read_tokens,
            totals.cache_write_tokens,
            0,
            overrides,
        );
        totals.cost = est.amount;
        totals.cost_basis = est.amount.map(|_| CostBasis::ApiEquivalent);
        projects_out.push(ProjectUsage {
            name: project_display_name(&key),
            path: key,
            model: bucket.model,
            totals,
        });
    }

    let mut detail = format!("{} projects - {} files", projects_out.len(), files_scanned);
    if no_cwd > 0 {
        detail.push_str(&format!(" - {no_cwd} sesiones sin cwd atribuible"));
    }
    AgentUsageReport::ok(AGENT, month_totals, projects_out, detail)
}

fn add_entry(
    bucket: &mut Bucket,
    input: u64,
    output: u64,
    cache_read: u64,
    cache_write: u64,
    reasoning: u64,
    model: Option<&str>,
) {
    bucket.totals.entries += 1;
    bucket.totals.input_tokens += input;
    bucket.totals.output_tokens += output;
    bucket.totals.cache_read_tokens += cache_read;
    bucket.totals.cache_write_tokens += cache_write;
    bucket.totals.reasoning_tokens += reasoning;
    if let Some(model) = model {
        if !model.is_empty() {
            bucket.model = model.to_string();
        }
    }
}

fn get_u64(node: &Value, name: &str) -> u64 {
    node.get(name).and_then(Value::as_u64).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::usage::AgentStatus;
    use chrono::TimeZone;
    use std::fs;
    use tempfile::tempdir;

    fn record(ts: &str, cwd: &str, input: u64, output: u64) -> String {
        format!(
            r#"{{"timestamp":"{ts}","payload":{{"cwd":"{cwd}","model":"gpt-5.6-luna","turn_token_usage":{{"input_tokens":{input},"output_tokens":{output},"cached_input_tokens":1000,"cache_write_input_tokens":100,"reasoning_output_tokens":20}}}}}}"#
        )
    }

    fn window() -> (DateTime<Local>, DateTime<Local>) {
        (
            Local.with_ymd_and_hms(2026, 9, 1, 0, 0, 0).unwrap(),
            Local.with_ymd_and_hms(2026, 10, 1, 0, 0, 0).unwrap(),
        )
    }

    #[test]
    fn missing_directory_is_not_installed() {
        let dir = tempdir().unwrap();
        let missing = dir.path().join("nope");
        let (start, end) = window();
        let report = read_usage(&missing, start, end, &pricing::PriceOverrides::default());
        assert_eq!(report.status, AgentStatus::NotInstalled);
    }

    #[test]
    fn last_record_wins_not_the_sum() {
        let dir = tempdir().unwrap();
        let content = format!(
            "{}\n{}\n{}\n",
            record("2026-09-15T10:00:00Z", "C:/repos/orbitbar", 30_796, 500),
            record("2026-09-15T10:05:00Z", "C:/repos/orbitbar", 500_000, 5_000),
            record("2026-09-15T10:10:00Z", "C:/repos/orbitbar", 1_356_488, 9_000),
        );
        fs::write(dir.path().join("session.jsonl"), content).unwrap();

        let (start, end) = window();
        let report = read_usage(dir.path(), start, end, &pricing::PriceOverrides::default());
        assert_eq!(report.status, AgentStatus::Ok);
        assert_eq!(report.totals.entries, 1, "one session file must contribute exactly one record");
        assert_eq!(report.totals.input_tokens, 1_356_488, "the LAST record's cumulative total must win, not a sum");
        assert_eq!(report.totals.output_tokens, 9_000);
    }

    #[test]
    fn window_filtering_excludes_sessions_outside_the_range() {
        let dir = tempdir().unwrap();
        fs::write(
            dir.path().join("in.jsonl"),
            record("2026-09-15T10:00:00Z", "C:/repos/orbitbar", 100, 50),
        )
        .unwrap();
        fs::write(
            dir.path().join("out.jsonl"),
            record("2026-08-31T23:59:59Z", "C:/repos/orbitbar", 999, 999),
        )
        .unwrap();

        let (start, end) = window();
        let report = read_usage(dir.path(), start, end, &pricing::PriceOverrides::default());
        assert_eq!(report.totals.entries, 1);
        assert_eq!(report.totals.output_tokens, 50);
    }

    #[test]
    fn a_malformed_line_is_skipped_not_fatal() {
        let dir = tempdir().unwrap();
        let content = format!(
            "garbage with token_usage in it but not json\n{}\n",
            record("2026-09-15T10:00:00Z", "C:/repos/orbitbar", 100, 50),
        );
        fs::write(dir.path().join("session.jsonl"), content).unwrap();

        let (start, end) = window();
        let report = read_usage(dir.path(), start, end, &pricing::PriceOverrides::default());
        assert_eq!(report.status, AgentStatus::Ok);
        assert_eq!(report.totals.entries, 1);
    }

    #[test]
    fn unpriced_model_leaves_cost_none() {
        let dir = tempdir().unwrap();
        let content = r#"{"timestamp":"2026-09-15T10:00:00Z","payload":{"cwd":"C:/repos/orbitbar","model":"some-unknown-codex-model","turn_token_usage":{"input_tokens":100,"output_tokens":50}}}"#.to_string() + "\n";
        fs::write(dir.path().join("session.jsonl"), content).unwrap();

        let (start, end) = window();
        let report = read_usage(dir.path(), start, end, &pricing::PriceOverrides::default());
        assert_eq!(report.totals.cost, None);
    }

    #[test]
    fn empty_directory_with_no_session_files_is_ok_with_empty_totals() {
        let dir = tempdir().unwrap();
        let (start, end) = window();
        let report = read_usage(dir.path(), start, end, &pricing::PriceOverrides::default());
        assert_eq!(report.status, AgentStatus::Ok);
        assert_eq!(report.totals.entries, 0);
    }

    #[test]
    fn a_priced_model_carries_the_api_equivalent_cost_basis() {
        let dir = tempdir().unwrap();
        fs::write(
            dir.path().join("session.jsonl"),
            record("2026-09-15T10:00:00Z", "C:/repos/orbitbar", 100, 50),
        )
        .unwrap();

        let (start, end) = window();
        let report = read_usage(dir.path(), start, end, &pricing::PriceOverrides::default());
        assert!(report.totals.cost.is_some());
        assert_eq!(
            report.totals.cost_basis,
            Some(CostBasis::ApiEquivalent),
            "codex money is an API-equivalent estimate, never a reported bill"
        );
    }

    #[test]
    fn an_override_wins_over_the_built_in_gpt_5_6_luna_row() {
        let dir = tempdir().unwrap();
        fs::write(
            dir.path().join("session.jsonl"),
            record("2026-09-15T10:00:00Z", "C:/repos/orbitbar", 1_000_000, 1_000_000),
        )
        .unwrap();

        let mut models = std::collections::HashMap::new();
        models.insert(
            "gpt-5.6-luna".to_string(),
            pricing::PriceOverrideEntry {
                input: 1.0,
                output: 1.0,
                cache_read: None,
                cache_write_5m: None,
                cache_write_1h: None,
            },
        );
        let overrides = pricing::PriceOverrides { models };

        let (start, end) = window();
        let report = read_usage(dir.path(), start, end, &overrides);
        // record() also carries a fixed 1000 cached + 100 cache-write tokens
        // priced at the override's default multipliers (0.1x / 1.25x input),
        // so the total is not an even 2.0 but is far from the built-in
        // gpt-5.6-luna rate's result (0.20 in / 1.20 out) either way.
        assert_eq!(report.totals.cost, Some(1.9992));
    }
}
