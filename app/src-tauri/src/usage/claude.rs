//! Claude usage reader: every `*.jsonl` under `~/.claude/projects`, de-duped
//! and windowed. Ported from `Get-ClaudeAgentHistory` in
//! `legacy/windows-widget/lib/Get-AgentUsage.ps1`.
//!
//! Subagent files are billed too, so every file counts, not just the newest
//! one. The store keeps more than one copy of many assistant messages
//! (original plus a snapshot copy), so entries are de-duplicated per file by
//! `requestId`, falling back to `message.id` and then `uuid` - summing raw
//! doubles every counter, verified on the legacy widget's own machine.
//!
//! Deviation from the legacy script: the legacy per-project bucket is filled
//! from every entry seen in the read budget regardless of whether that entry
//! falls inside the month, while the month total is filtered. That looks like
//! an oversight rather than a deliberate design (the panel's own docstring
//! says the project split is "over every project cwd the agent's own store
//! touched this month"), so this port applies the window consistently to
//! both the totals and the per-project breakdown.

use crate::usage::{
    find_jsonl_files, normalize_project_path, pricing, project_display_name, AgentUsageReport,
    CostBasis, ProjectUsage, UsageTotals,
};
use chrono::{DateTime, Local};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader};
use std::path::Path;

const AGENT: &str = "claude";

struct Bucket {
    totals: UsageTotals,
    model: String,
    /// Cache-write tokens split by TTL bucket, tracked separately from
    /// `totals.cache_write_tokens` (their sum, used for display) because
    /// Anthropic prices a 5-minute cache write differently from a 1-hour one.
    cache_write_5m: u64,
    cache_write_1h: u64,
}

impl Bucket {
    fn new() -> Self {
        Self { totals: UsageTotals::default(), model: String::new(), cache_write_5m: 0, cache_write_1h: 0 }
    }
}

/// Reads month/window usage from every claude session file under `root`,
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
    let mut projects: HashMap<String, Bucket> = HashMap::new();
    let mut files_scanned = 0u64;

    for file in &files {
        let Ok(handle) = std::fs::File::open(file) else { continue };
        files_scanned += 1;
        let reader = BufReader::new(handle);

        // Dedupe is per file: the same requestId can appear in more than one
        // session file (a resumed session, a subagent transcript), and those
        // are separate billable events.
        let mut seen: HashSet<String> = HashSet::new();

        for line in reader.lines() {
            let Ok(line) = line else { continue };
            if !line.contains("\"usage\"") {
                continue;
            }
            // A malformed line is skipped, never fatal to the file.
            let Ok(entry) = serde_json::from_str::<Value>(&line) else { continue };

            let usage = entry
                .get("message")
                .and_then(|m| m.get("usage"))
                .or_else(|| entry.get("usage"));
            let Some(usage) = usage else { continue };

            let key = entry
                .get("requestId")
                .and_then(Value::as_str)
                .or_else(|| entry.get("message").and_then(|m| m.get("id")).and_then(Value::as_str))
                .or_else(|| entry.get("uuid").and_then(Value::as_str));
            if let Some(key) = key {
                if !seen.insert(key.to_string()) {
                    continue;
                }
            }

            let ts = entry
                .get("timestamp")
                .and_then(Value::as_str)
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                .map(|dt| dt.with_timezone(&Local));
            let in_window = matches!(ts, Some(ts) if ts >= start && ts < end);
            if !in_window {
                continue;
            }

            let cwd = entry.get("cwd").and_then(Value::as_str).unwrap_or("");
            let cwd = normalize_project_path(cwd);

            let model = entry
                .get("message")
                .and_then(|m| m.get("model"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|m| !m.is_empty() && !m.starts_with('<'));

            let input = get_u64(usage, "input_tokens");
            let output = get_u64(usage, "output_tokens");
            let cache_read = get_u64(usage, "cache_read_input_tokens");
            let (cache_write_5m, cache_write_1h) = extract_cache_write(usage);
            let reasoning = usage
                .get("output_tokens_details")
                .map(|d| get_u64(d, "thinking_tokens"))
                .unwrap_or(0);

            add_entry(&mut month, input, output, cache_read, cache_write_5m, cache_write_1h, reasoning, model);

            if !cwd.is_empty() {
                let bucket = projects.entry(cwd).or_insert_with(Bucket::new);
                add_entry(bucket, input, output, cache_read, cache_write_5m, cache_write_1h, reasoning, model);
            }
        }
    }

    if files_scanned == 0 {
        return AgentUsageReport::error(AGENT, format!("no session files readable under {}", root.display()));
    }

    let mut month_totals = month.totals;
    if month_totals.entries > 0 {
        let est = pricing::estimate_cost(
            &month.model,
            month_totals.input_tokens,
            month_totals.output_tokens,
            month_totals.cache_read_tokens,
            month.cache_write_5m,
            month.cache_write_1h,
            overrides,
        );
        month_totals.cost = est.amount;
        month_totals.cost_basis = est.amount.map(|_| CostBasis::ApiEquivalent);
    }

    let mut projects_out = Vec::with_capacity(projects.len());
    for (key, bucket) in projects {
        let mut totals = bucket.totals;
        let est = pricing::estimate_cost(
            &bucket.model,
            totals.input_tokens,
            totals.output_tokens,
            totals.cache_read_tokens,
            bucket.cache_write_5m,
            bucket.cache_write_1h,
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

    let detail = format!("{} projects - {} files", projects_out.len(), files_scanned);
    AgentUsageReport::ok(AGENT, month_totals, projects_out, detail)
}

#[allow(clippy::too_many_arguments)]
fn add_entry(
    bucket: &mut Bucket,
    input: u64,
    output: u64,
    cache_read: u64,
    cache_write_5m: u64,
    cache_write_1h: u64,
    reasoning: u64,
    model: Option<&str>,
) {
    bucket.totals.entries += 1;
    bucket.totals.input_tokens += input;
    bucket.totals.output_tokens += output;
    bucket.totals.cache_read_tokens += cache_read;
    bucket.totals.cache_write_tokens += cache_write_5m + cache_write_1h;
    bucket.cache_write_5m += cache_write_5m;
    bucket.cache_write_1h += cache_write_1h;
    bucket.totals.reasoning_tokens += reasoning;
    if let Some(model) = model {
        bucket.model = model.to_string();
    }
}

fn get_u64(node: &Value, name: &str) -> u64 {
    node.get(name).and_then(Value::as_u64).unwrap_or(0)
}

/// Splits `usage`'s cache-write tokens into (5-minute TTL, 1-hour TTL)
/// buckets. Recent claude records carry `cache_creation.ephemeral_5m_input_tokens`
/// / `ephemeral_1h_input_tokens`; older records only have the flat
/// `cache_creation_input_tokens`, which this treats as a 5-minute write since
/// that was the only TTL that existed before the split field shipped.
fn extract_cache_write(usage: &Value) -> (u64, u64) {
    if let Some(cache_creation) = usage.get("cache_creation") {
        let five_m = get_u64(cache_creation, "ephemeral_5m_input_tokens");
        let one_h = get_u64(cache_creation, "ephemeral_1h_input_tokens");
        return (five_m, one_h);
    }
    (get_u64(usage, "cache_creation_input_tokens"), 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use std::fs;
    use tempfile::tempdir;

    fn line(request_id: &str, ts: &str, cwd: &str, input: u64, output: u64) -> String {
        format!(
            r#"{{"requestId":"{request_id}","timestamp":"{ts}","cwd":"{cwd}","message":{{"id":"msg_{request_id}","model":"claude-opus-5-5","usage":{{"input_tokens":{input},"output_tokens":{output},"cache_read_input_tokens":10,"cache_creation_input_tokens":5,"output_tokens_details":{{"thinking_tokens":2}}}}}}}}"#
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
        assert_eq!(report.status, crate::usage::AgentStatus::NotInstalled);
    }

    #[test]
    fn duplicate_entries_are_deduplicated_by_request_id() {
        let dir = tempdir().unwrap();
        let project = dir.path().join("proj");
        fs::create_dir_all(&project).unwrap();
        let content = format!(
            "{}\n{}\n",
            line("req1", "2026-09-15T10:00:00Z", "C:/repos/orbitbar", 100, 50),
            line("req1", "2026-09-15T10:00:00Z", "C:/repos/orbitbar", 100, 50),
        );
        fs::write(project.join("session.jsonl"), content).unwrap();

        let (start, end) = window();
        let report = read_usage(dir.path(), start, end, &pricing::PriceOverrides::default());
        assert_eq!(report.status, crate::usage::AgentStatus::Ok);
        assert_eq!(report.totals.entries, 1, "the duplicate copy must not be counted twice");
        assert_eq!(report.totals.output_tokens, 50);
    }

    #[test]
    fn window_filtering_excludes_entries_outside_the_range() {
        let dir = tempdir().unwrap();
        let project = dir.path().join("proj");
        fs::create_dir_all(&project).unwrap();
        let (start, end) = window();
        // Timestamps are derived from the window's own local boundaries
        // (converted to the UTC the store actually writes) rather than
        // hardcoded UTC strings, so this test is correct under any local
        // timezone offset, not just UTC.
        let in_window_ts = start.to_utc().to_rfc3339();
        let before_ts = (start - chrono::Duration::seconds(1)).to_utc().to_rfc3339();
        let after_ts = end.to_utc().to_rfc3339();
        let content = format!(
            "{}\n{}\n{}\n",
            line("in-window", &in_window_ts, "C:/repos/orbitbar", 100, 50),
            line("before", &before_ts, "C:/repos/orbitbar", 999, 999),
            line("after", &after_ts, "C:/repos/orbitbar", 999, 999),
        );
        fs::write(project.join("session.jsonl"), content).unwrap();

        let report = read_usage(dir.path(), start, end, &pricing::PriceOverrides::default());
        assert_eq!(report.totals.entries, 1);
        assert_eq!(report.totals.output_tokens, 50);
    }

    #[test]
    fn a_malformed_line_is_skipped_not_fatal() {
        let dir = tempdir().unwrap();
        let project = dir.path().join("proj");
        fs::create_dir_all(&project).unwrap();
        let content = format!(
            "not json at all but has \"usage\" in it\n{}\n",
            line("ok1", "2026-09-15T10:00:00Z", "C:/repos/orbitbar", 100, 50),
        );
        fs::write(project.join("session.jsonl"), content).unwrap();

        let (start, end) = window();
        let report = read_usage(dir.path(), start, end, &pricing::PriceOverrides::default());
        assert_eq!(report.status, crate::usage::AgentStatus::Ok);
        assert_eq!(report.totals.entries, 1);
    }

    #[test]
    fn project_breakdown_sums_per_cwd_and_sorts_by_output_desc() {
        let dir = tempdir().unwrap();
        let project = dir.path().join("proj");
        fs::create_dir_all(&project).unwrap();
        let content = format!(
            "{}\n{}\n",
            line("req-a", "2026-09-15T10:00:00Z", "C:/repos/small", 100, 10),
            line("req-b", "2026-09-15T10:01:00Z", "C:/repos/big", 100, 500),
        );
        fs::write(project.join("session.jsonl"), content).unwrap();

        let (start, end) = window();
        let report = read_usage(dir.path(), start, end, &pricing::PriceOverrides::default());
        assert_eq!(report.projects.len(), 2);
        assert_eq!(report.projects[0].name, "big");
        assert_eq!(report.projects[1].name, "small");
    }

    #[test]
    fn unpriced_model_leaves_cost_none() {
        let dir = tempdir().unwrap();
        let project = dir.path().join("proj");
        fs::create_dir_all(&project).unwrap();
        let content = r#"{"requestId":"req1","timestamp":"2026-09-15T10:00:00Z","cwd":"C:/repos/orbitbar","message":{"id":"msg_req1","model":"some-unknown-model","usage":{"input_tokens":100,"output_tokens":50}}}"#.to_string() + "\n";
        fs::write(project.join("session.jsonl"), content).unwrap();

        let (start, end) = window();
        let report = read_usage(dir.path(), start, end, &pricing::PriceOverrides::default());
        assert_eq!(report.totals.cost, None, "a model without a price row must never invent a cost");
    }

    #[test]
    fn empty_directory_with_no_session_files_is_ok_with_empty_totals() {
        let dir = tempdir().unwrap();
        let (start, end) = window();
        let report = read_usage(dir.path(), start, end, &pricing::PriceOverrides::default());
        assert_eq!(report.status, crate::usage::AgentStatus::Ok);
        assert_eq!(report.totals.entries, 0);
        assert!(report.projects.is_empty());
    }

    #[test]
    fn a_priced_model_carries_the_api_equivalent_cost_basis() {
        let dir = tempdir().unwrap();
        let project = dir.path().join("proj");
        fs::create_dir_all(&project).unwrap();
        let content = line("req1", "2026-09-15T10:00:00Z", "C:/repos/orbitbar", 100, 50) + "\n";
        fs::write(project.join("session.jsonl"), content).unwrap();

        let (start, end) = window();
        let report = read_usage(dir.path(), start, end, &pricing::PriceOverrides::default());
        assert!(report.totals.cost.is_some());
        assert_eq!(report.totals.cost_basis, Some(crate::usage::CostBasis::ApiEquivalent));
    }

    /// A record carrying the TTL-split `cache_creation` object must price its
    /// 1-hour tokens at the 1-hour rate, not fold them into the flat
    /// `cache_creation_input_tokens` 5-minute fallback.
    #[test]
    fn ephemeral_1h_cache_write_is_priced_separately_from_5m() {
        let dir = tempdir().unwrap();
        let project = dir.path().join("proj");
        fs::create_dir_all(&project).unwrap();
        let content = r#"{"requestId":"req1","timestamp":"2026-09-15T10:00:00Z","cwd":"C:/repos/orbitbar","message":{"id":"msg_req1","model":"claude-opus-5","usage":{"input_tokens":0,"output_tokens":0,"cache_read_input_tokens":0,"cache_creation":{"ephemeral_5m_input_tokens":0,"ephemeral_1h_input_tokens":1000000}}}}"#.to_string() + "\n";
        fs::write(project.join("session.jsonl"), content).unwrap();

        let (start, end) = window();
        let report = read_usage(dir.path(), start, end, &pricing::PriceOverrides::default());
        // claude-opus-5 cache_write_1h = 10.0 per 1M, not 6.25 (the 5m rate).
        assert_eq!(report.totals.cost, Some(10.0));
    }
}
