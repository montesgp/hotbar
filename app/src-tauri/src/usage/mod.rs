//! Token-spend usage readers: claude, codex and opencode, each read from the
//! agent's own local files, no external service or program involved. Ported
//! from `legacy/windows-widget/lib/Get-AgentUsage.ps1`
//! (`Get-*AgentHistory`), which is the semantics source of truth for
//! deduplication, cumulative-vs-summed counters and cost estimation.
//!
//! One function, [`collect_usage`], returns a per-agent breakdown for a time
//! window plus a per-project split sorted by output tokens. A broken or
//! missing store on one agent never hides the other two: each carries its own
//! [`AgentStatus`].

pub mod claude;
pub mod codex;
pub mod opencode;
pub mod pricing;

use chrono::{DateTime, Datelike, Duration, Local, TimeZone};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// The time window a snapshot is computed over. `ThisMonth` is the default,
/// matching the legacy widget's month-to-date panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum TimeWindow {
    Today,
    Last7Days,
    Last30Days,
    #[default]
    ThisMonth,
}

/// Resolves a window to a local `[start, end)` instant pair. All windows are
/// full-day boundaries at local midnight, so "last 7 days" always means 7
/// whole days including today, never a rolling 168-hour clock.
pub fn window_range(window: TimeWindow, now: DateTime<Local>) -> (DateTime<Local>, DateTime<Local>) {
    let today_start = now
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .expect("midnight is always a valid time");
    let today_start = Local.from_local_datetime(&today_start).single().unwrap_or(now);
    let tomorrow_start = today_start + Duration::days(1);

    match window {
        TimeWindow::Today => (today_start, tomorrow_start),
        TimeWindow::Last7Days => (today_start - Duration::days(6), tomorrow_start),
        TimeWindow::Last30Days => (today_start - Duration::days(29), tomorrow_start),
        TimeWindow::ThisMonth => {
            let month_start_naive = today_start
                .date_naive()
                .with_day(1)
                .expect("day 1 always exists");
            let month_start = Local
                .from_local_datetime(&month_start_naive.and_hms_opt(0, 0, 0).unwrap())
                .single()
                .unwrap_or(today_start);
            let next_month_start_naive = if month_start_naive.month() == 12 {
                month_start_naive
                    .with_year(month_start_naive.year() + 1)
                    .and_then(|d| d.with_month(1))
            } else {
                month_start_naive.with_month(month_start_naive.month() + 1)
            }
            .expect("adjacent month always exists");
            let next_month_start = Local
                .from_local_datetime(&next_month_start_naive.and_hms_opt(0, 0, 0).unwrap())
                .single()
                .unwrap_or(month_start);
            (month_start, next_month_start)
        }
    }
}

/// Per-agent status: `Ok` even when the window is empty of activity, `NotInstalled`
/// when the store's root directory/file is missing (the agent was never
/// used, or is not installed on this machine), `Error` for anything else
/// (locked file, unreadable database, ...). A broken store never reports
/// zeroes as if that were the truth.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", tag = "state", content = "detail")]
pub enum AgentStatus {
    Ok,
    NotInstalled,
    Error(String),
}

/// Aggregated counters for one agent (or one project inside an agent), over
/// the window. `cost` is `None` when the store carries no cost field and no
/// price row exists for the model(s) involved - never a guessed number.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageTotals {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
    pub reasoning_tokens: u64,
    pub cost: Option<f64>,
    pub entries: u64,
}

/// One project's totals inside an agent's window, keyed by the normalized
/// session `cwd`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectUsage {
    pub path: String,
    pub name: String,
    pub model: String,
    pub totals: UsageTotals,
}

/// One agent's full report: overall totals plus the per-project breakdown,
/// sorted by output tokens descending (busiest project first).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentUsageReport {
    pub agent: String,
    pub status: AgentStatus,
    pub totals: UsageTotals,
    pub projects: Vec<ProjectUsage>,
    pub detail: String,
}

impl AgentUsageReport {
    pub fn not_installed(agent: &str, detail: impl Into<String>) -> Self {
        Self {
            agent: agent.to_string(),
            status: AgentStatus::NotInstalled,
            totals: UsageTotals::default(),
            projects: Vec::new(),
            detail: detail.into(),
        }
    }

    pub fn error(agent: &str, detail: impl Into<String>) -> Self {
        let detail = detail.into();
        Self {
            agent: agent.to_string(),
            status: AgentStatus::Error(detail.clone()),
            totals: UsageTotals::default(),
            projects: Vec::new(),
            detail,
        }
    }

    pub fn ok(agent: &str, totals: UsageTotals, mut projects: Vec<ProjectUsage>, detail: impl Into<String>) -> Self {
        projects.sort_by_key(|p| std::cmp::Reverse(p.totals.output_tokens));
        Self {
            agent: agent.to_string(),
            status: AgentStatus::Ok,
            totals,
            projects,
            detail: detail.into(),
        }
    }
}

/// The three store roots, resolved from a home directory. Readers take their
/// root as a parameter (never resolve it themselves) so tests can point them
/// at fixtures.
#[derive(Debug, Clone)]
pub struct UsagePaths {
    pub claude_dir: PathBuf,
    pub codex_dir: PathBuf,
    pub opencode_db: PathBuf,
}

/// Resolves the three store roots under a home directory. `home_override`
/// lets tests and the parity harness point at a fixture home instead of the
/// real one; the ordinary caller passes `None` and gets `dirs::home_dir()`.
///
/// The layout is home-relative and identical across Windows, macOS and
/// Linux, matching what `legacy/windows-widget/lib/Get-AgentUsage.ps1`
/// verified on disk: opencode is a cross-platform CLI that writes to
/// `<home>/.local/share/opencode/opencode.db` on every OS, not to each
/// platform's "special" data directory, so this mirrors that literal path
/// rather than asking `dirs::data_dir()` for a platform-specific one.
pub fn resolve_paths(home_override: Option<&Path>) -> Option<UsagePaths> {
    let home = match home_override {
        Some(p) => p.to_path_buf(),
        None => dirs::home_dir()?,
    };
    Some(UsagePaths {
        claude_dir: home.join(".claude").join("projects"),
        codex_dir: home.join(".codex").join("sessions"),
        opencode_db: home
            .join(".local")
            .join("share")
            .join("opencode")
            .join("opencode.db"),
    })
}

/// The canonical form of a project path: trimmed, slashes normalized to the
/// platform separator style used by the legacy widget (backslash), trailing
/// separator removed. Ported from `Normalize-AgentProjectPath` so the same
/// cwd string always buckets to the same project key.
pub fn normalize_project_path(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let normalized = trimmed.replace('/', "\\");
    normalized.trim_end_matches(['\\', ' ']).to_string()
}

/// The display name for a project key: its last path segment, falling back
/// to the whole key when there is no separator (e.g. a bare drive root).
pub fn project_display_name(key: &str) -> String {
    key.rsplit(['\\', '/'])
        .find(|s| !s.is_empty())
        .unwrap_or(key)
        .to_string()
}

/// Every `*.jsonl` file under `root`, recursively. A directory that cannot be
/// read (permissions, race with a delete) is skipped rather than failing the
/// whole scan - a store is otherwise honest even if one subfolder is
/// temporarily unreadable.
pub(crate) fn find_jsonl_files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("jsonl")) {
                out.push(path);
            }
        }
    }
    out
}

/// One snapshot of all three agents for a time window - the shape the
/// `get_usage` Tauri command returns.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageSnapshot {
    pub window: TimeWindow,
    pub window_start: String,
    pub window_end: String,
    pub generated_at: String,
    pub agents: Vec<AgentUsageReport>,
}

/// Reads all three agents for `window`, as of `now`. Sequential like the
/// legacy widget: three bounded, independent reads are cheaper than the
/// coordination a parallel version would need, and a broken one never blocks
/// the others because each is wrapped in its own `AgentUsageReport`.
pub fn collect_usage(window: TimeWindow, paths: &UsagePaths, now: DateTime<Local>) -> UsageSnapshot {
    let (start, end) = window_range(window, now);

    let claude = claude::read_usage(&paths.claude_dir, start, end);
    let codex = codex::read_usage(&paths.codex_dir, start, end);
    let opencode = opencode::read_usage(&paths.opencode_db, start, end);

    UsageSnapshot {
        window,
        window_start: start.to_rfc3339(),
        window_end: end.to_rfc3339(),
        generated_at: now.to_rfc3339(),
        agents: vec![claude, codex, opencode],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn local(y: i32, m: u32, d: u32, h: u32, mi: u32) -> DateTime<Local> {
        Local.with_ymd_and_hms(y, m, d, h, mi, 0).unwrap()
    }

    #[test]
    fn today_window_is_local_midnight_to_midnight() {
        let now = local(2026, 9, 28, 15, 30);
        let (start, end) = window_range(TimeWindow::Today, now);
        assert_eq!(start, local(2026, 9, 28, 0, 0));
        assert_eq!(end, local(2026, 9, 29, 0, 0));
    }

    #[test]
    fn last_7_days_includes_today_and_the_previous_6_days() {
        let now = local(2026, 9, 28, 8, 0);
        let (start, end) = window_range(TimeWindow::Last7Days, now);
        assert_eq!(start, local(2026, 9, 22, 0, 0));
        assert_eq!(end, local(2026, 9, 29, 0, 0));
        assert_eq!((end - start).num_days(), 7);
    }

    #[test]
    fn last_30_days_spans_exactly_30_days() {
        let now = local(2026, 9, 28, 8, 0);
        let (start, end) = window_range(TimeWindow::Last30Days, now);
        assert_eq!((end - start).num_days(), 30);
        assert_eq!(end, local(2026, 9, 29, 0, 0));
    }

    #[test]
    fn this_month_spans_the_calendar_month() {
        let now = local(2026, 9, 15, 12, 0);
        let (start, end) = window_range(TimeWindow::ThisMonth, now);
        assert_eq!(start, local(2026, 9, 1, 0, 0));
        assert_eq!(end, local(2026, 10, 1, 0, 0));
    }

    #[test]
    fn this_month_rolls_over_at_a_december_boundary() {
        let now = local(2026, 12, 31, 23, 0);
        let (start, end) = window_range(TimeWindow::ThisMonth, now);
        assert_eq!(start, local(2026, 12, 1, 0, 0));
        assert_eq!(end, local(2027, 1, 1, 0, 0));
    }

    #[test]
    fn default_window_is_this_month() {
        assert_eq!(TimeWindow::default(), TimeWindow::ThisMonth);
    }

    #[test]
    fn normalize_project_path_unifies_separators_and_trims_trailing_slash() {
        assert_eq!(normalize_project_path("C:/repos/orbitbar/"), "C:\\repos\\orbitbar");
        assert_eq!(normalize_project_path("  C:\\repos\\orbitbar\\  "), "C:\\repos\\orbitbar");
        assert_eq!(normalize_project_path(""), "");
    }

    #[test]
    fn project_display_name_is_the_last_segment() {
        assert_eq!(project_display_name("C:\\repos\\orbitbar"), "orbitbar");
        assert_eq!(project_display_name("/home/user/orbitbar"), "orbitbar");
        assert_eq!(project_display_name("orbitbar"), "orbitbar");
    }

    #[test]
    fn resolve_paths_uses_the_override_home_when_given() {
        let home = Path::new("Z:\\fixture-home");
        let paths = resolve_paths(Some(home)).expect("override must resolve");
        assert_eq!(paths.claude_dir, home.join(".claude").join("projects"));
        assert_eq!(paths.codex_dir, home.join(".codex").join("sessions"));
        assert_eq!(
            paths.opencode_db,
            home.join(".local").join("share").join("opencode").join("opencode.db")
        );
    }

    #[test]
    fn agent_report_ok_sorts_projects_by_output_desc() {
        let mk = |name: &str, output: u64| ProjectUsage {
            path: name.into(),
            name: name.into(),
            model: "".into(),
            totals: UsageTotals { output_tokens: output, ..Default::default() },
        };
        let report = AgentUsageReport::ok(
            "claude",
            UsageTotals::default(),
            vec![mk("a", 10), mk("b", 100), mk("c", 50)],
            "",
        );
        let names: Vec<&str> = report.projects.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, vec!["b", "c", "a"]);
    }

    /// Not a unit test: prints the real home directory's ThisMonth snapshot
    /// so it can be compared by hand against the legacy PowerShell reader's
    /// `Get-AgentHistorySnapshot`. `--ignored` because it depends on this
    /// machine's real `.claude` / `.codex` / opencode stores, which do not
    /// exist in CI or on a fresh checkout.
    ///
    /// Run with: `cargo test -- --ignored parity --nocapture`
    #[test]
    #[ignore]
    fn parity_this_month_against_the_real_home() {
        let paths = resolve_paths(None).expect("real home directory must resolve on this machine");
        let snapshot = collect_usage(TimeWindow::ThisMonth, &paths, Local::now());

        println!("\n=== orbitbar Rust reader parity (ThisMonth) ===");
        for agent in &snapshot.agents {
            println!(
                "{:9} status={:?} entries={:<6} output_tokens={:<10} cost={:?} projects={}",
                agent.agent,
                agent.status,
                agent.totals.entries,
                agent.totals.output_tokens,
                agent.totals.cost,
                agent.projects.len()
            );
        }
    }
}
