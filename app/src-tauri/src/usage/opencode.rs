//! Opencode usage reader: SQLite `session` table at
//! `~/.local/share/opencode/opencode.db`.
//!
//! The `session` table is already consolidated per session (unlike the
//! claude/codex jsonl stores), so this is two indexed `SELECT`s: one totals
//! the window, one splits the same window per `project.worktree`. Money is
//! REAL - `session.cost` as recorded, not estimated - so opencode never goes
//! through the pricing table and a `0.0` is a genuine zero for a local model.
//!
//! Opened read-only (`rusqlite` with the `bundled` feature, no system SQLite
//! dependency) so the widget can never write to a database a live opencode
//! process might also have open.
//!
//! Sessions outside any project (a cwd under the OS temp dir, or an existing
//! folder that is not inside a repository; see `resolve_project_root`) are
//! left out of the totals and the project rows alike, so that invariant holds.

use crate::usage::{
    normalize_project_path, project_display_name, resolve_project_root, AgentUsageReport, CostBasis,
    ProjectRootCache, ProjectUsage, UsageTotals,
};
use chrono::{DateTime, Local};
use rusqlite::{Connection, OpenFlags};
use std::collections::HashMap;
use std::path::Path;

const AGENT: &str = "opencode";

/// Reads window usage from the opencode SQLite store at `db_path`.
/// Per-project totals are keyed by resolved project root (see
/// `resolve_project_root`), not the raw `project.worktree` path, so two
/// worktrees inside the same repository merge into one row.
pub fn read_usage(
    db_path: &Path,
    start: DateTime<Local>,
    end: DateTime<Local>,
    root_cache: &mut ProjectRootCache,
) -> AgentUsageReport {
    if !db_path.exists() {
        return AgentUsageReport::not_installed(AGENT, format!("database not found: {}", db_path.display()));
    }

    let conn = match Connection::open_with_flags(
        db_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ) {
        Ok(c) => c,
        Err(e) => return AgentUsageReport::error(AGENT, format!("cannot open opencode.db: {e}")),
    };
    if let Err(e) = conn.busy_timeout(std::time::Duration::from_millis(1200)) {
        return AgentUsageReport::error(AGENT, format!("cannot set busy timeout: {e}"));
    }

    let start_ms = start.timestamp_millis();
    let end_ms = end.timestamp_millis();

    let mut totals = match read_totals(&conn, start_ms, end_ms) {
        Ok(t) => t,
        Err(e) => return AgentUsageReport::error(AGENT, format!("cannot read session totals: {e}")),
    };

    let (projects, excluded) = match read_projects(&conn, start_ms, end_ms, root_cache) {
        Ok(p) => p,
        Err(e) => return AgentUsageReport::error(AGENT, format!("cannot read per-project totals: {e}")),
    };

    // Sessions whose worktree is excluded (see `resolve_project_root`) are
    // left out of the totals as well as the project rows, so the per-project
    // split still sums to the totals.
    subtract_excluded(&mut totals, &excluded);

    let mut detail = format!("{} projects", projects.len());
    if excluded.entries > 0 {
        detail.push_str(&format!(" - {} sessions outside any project not counted", excluded.entries));
    }
    AgentUsageReport::ok(AGENT, totals, projects, detail)
}

fn read_totals(conn: &Connection, start_ms: i64, end_ms: i64) -> rusqlite::Result<UsageTotals> {
    let mut stmt = conn.prepare(
        "SELECT COALESCE(SUM(tokens_input), 0),
                COALESCE(SUM(tokens_output), 0),
                COALESCE(SUM(tokens_reasoning), 0),
                COALESCE(SUM(tokens_cache_read), 0),
                COALESCE(SUM(tokens_cache_write), 0),
                COALESCE(SUM(cost), 0.0),
                COUNT(*)
         FROM session WHERE time_updated >= ?1 AND time_updated < ?2",
    )?;
    stmt.query_row([start_ms, end_ms], |row| {
        let entries: i64 = row.get(6)?;
        let cost: f64 = row.get(5)?;
        Ok(UsageTotals {
            input_tokens: row.get::<_, i64>(0)? as u64,
            output_tokens: row.get::<_, i64>(1)? as u64,
            reasoning_tokens: row.get::<_, i64>(2)? as u64,
            cache_read_tokens: row.get::<_, i64>(3)? as u64,
            cache_write_tokens: row.get::<_, i64>(4)? as u64,
            // Real money, never estimated: a session with entries but a null
            // sum is 0.0, a real zero for a local model. Zero sessions means
            // no cost value can even be claimed.
            cost: if entries > 0 { Some(cost) } else { None },
            cost_basis: if entries > 0 { Some(CostBasis::Reported) } else { None },
            entries: entries as u64,
        })
    })
}

/// Takes the excluded sessions' counters back out of the SQL totals. With no
/// session left there is no cost to claim, so the cost goes back to `None`
/// exactly as `read_totals` reports an empty window.
fn subtract_excluded(totals: &mut UsageTotals, excluded: &UsageTotals) {
    if excluded.entries == 0 {
        return;
    }
    totals.input_tokens = totals.input_tokens.saturating_sub(excluded.input_tokens);
    totals.output_tokens = totals.output_tokens.saturating_sub(excluded.output_tokens);
    totals.reasoning_tokens = totals.reasoning_tokens.saturating_sub(excluded.reasoning_tokens);
    totals.cache_read_tokens = totals.cache_read_tokens.saturating_sub(excluded.cache_read_tokens);
    totals.cache_write_tokens = totals.cache_write_tokens.saturating_sub(excluded.cache_write_tokens);
    totals.entries = totals.entries.saturating_sub(excluded.entries);
    if totals.entries == 0 {
        totals.cost = None;
        totals.cost_basis = None;
    } else {
        totals.cost = Some((totals.cost.unwrap_or(0.0) - excluded.cost.unwrap_or(0.0)).max(0.0));
    }
}

/// The per-project rows, plus the totals of the sessions whose worktree was
/// excluded (zero entries when none was).
fn read_projects(
    conn: &Connection,
    start_ms: i64,
    end_ms: i64,
    root_cache: &mut ProjectRootCache,
) -> rusqlite::Result<(Vec<ProjectUsage>, UsageTotals)> {
    let mut stmt = conn.prepare(
        "SELECT p.worktree,
                COALESCE(SUM(s.tokens_input), 0),
                COALESCE(SUM(s.tokens_output), 0),
                COALESCE(SUM(s.tokens_reasoning), 0),
                COALESCE(SUM(s.tokens_cache_read), 0),
                COALESCE(SUM(s.tokens_cache_write), 0),
                COALESCE(SUM(s.cost), 0.0),
                COUNT(*)
         FROM session s JOIN project p ON s.project_id = p.id
         WHERE s.time_updated >= ?1 AND s.time_updated < ?2
         GROUP BY p.worktree",
    )?;
    let rows = stmt.query_map([start_ms, end_ms], |row| {
        let worktree: String = row.get(0)?;
        let entries: i64 = row.get(7)?;
        let cost: f64 = row.get(6)?;
        Ok((
            worktree,
            UsageTotals {
                input_tokens: row.get::<_, i64>(1)? as u64,
                output_tokens: row.get::<_, i64>(2)? as u64,
                reasoning_tokens: row.get::<_, i64>(3)? as u64,
                cache_read_tokens: row.get::<_, i64>(4)? as u64,
                cache_write_tokens: row.get::<_, i64>(5)? as u64,
                cost: if entries > 0 { Some(cost) } else { None },
                cost_basis: if entries > 0 { Some(CostBasis::Reported) } else { None },
                entries: entries as u64,
            },
        ))
    })?;

    // Several worktrees can resolve to the same project root (a subfolder,
    // another checkout of the same repo), so this aggregates by resolved
    // root rather than pushing one row per SQL group.
    let mut aggregated: HashMap<String, UsageTotals> = HashMap::new();
    let mut excluded = UsageTotals::default();
    for row in rows {
        let (worktree, totals) = row?;
        let key = normalize_project_path(&worktree);
        if key.is_empty() {
            continue;
        }
        let acc = match resolve_project_root(&key, root_cache) {
            Some(root) => aggregated.entry(root).or_default(),
            None => &mut excluded,
        };
        acc.input_tokens += totals.input_tokens;
        acc.output_tokens += totals.output_tokens;
        acc.reasoning_tokens += totals.reasoning_tokens;
        acc.cache_read_tokens += totals.cache_read_tokens;
        acc.cache_write_tokens += totals.cache_write_tokens;
        acc.entries += totals.entries;
        // Every row here came from a matched session, so `cost`/`cost_basis`
        // are always `Some` (see the query above) - summing the amounts and
        // re-asserting `Reported` keeps that same guarantee on the merged row.
        acc.cost = Some(acc.cost.unwrap_or(0.0) + totals.cost.unwrap_or(0.0));
        acc.cost_basis = Some(CostBasis::Reported);
    }

    let mut out = Vec::with_capacity(aggregated.len());
    for (root, totals) in aggregated {
        out.push(ProjectUsage {
            name: project_display_name(&root),
            path: root,
            model: String::new(),
            totals,
        });
    }
    Ok((out, excluded))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::usage::AgentStatus;
    use chrono::TimeZone;
    use tempfile::tempdir;

    fn window() -> (DateTime<Local>, DateTime<Local>) {
        (
            Local.with_ymd_and_hms(2026, 9, 1, 0, 0, 0).unwrap(),
            Local.with_ymd_and_hms(2026, 10, 1, 0, 0, 0).unwrap(),
        )
    }

    fn make_db(path: &Path) -> Connection {
        let conn = Connection::open(path).unwrap();
        conn.execute_batch(
            "CREATE TABLE project (id TEXT PRIMARY KEY, worktree TEXT);
             CREATE TABLE session (
                 id TEXT PRIMARY KEY,
                 project_id TEXT,
                 time_updated INTEGER,
                 cost REAL,
                 tokens_input INTEGER,
                 tokens_output INTEGER,
                 tokens_reasoning INTEGER,
                 tokens_cache_read INTEGER,
                 tokens_cache_write INTEGER
             );",
        )
        .unwrap();
        conn
    }

    fn ms(y: i32, m: u32, d: u32) -> i64 {
        Local.with_ymd_and_hms(y, m, d, 0, 0, 0).unwrap().timestamp_millis()
    }

    #[test]
    fn missing_database_is_not_installed() {
        let dir = tempdir().unwrap();
        let missing = dir.path().join("opencode.db");
        let (start, end) = window();
        let report = read_usage(&missing, start, end, &mut ProjectRootCache::new());
        assert_eq!(report.status, AgentStatus::NotInstalled);
    }

    #[test]
    fn reads_the_real_cost_column_not_an_estimate() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("opencode.db");
        let conn = make_db(&path);
        conn.execute(
            "INSERT INTO project (id, worktree) VALUES ('p1', 'C:/repos/orbitbar')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO session (id, project_id, time_updated, cost, tokens_input, tokens_output, tokens_reasoning, tokens_cache_read, tokens_cache_write)
             VALUES ('s1', 'p1', ?1, 1.2345, 1000, 500, 10, 20, 30)",
            [ms(2026, 9, 15)],
        )
        .unwrap();
        drop(conn);

        let (start, end) = window();
        let report = read_usage(&path, start, end, &mut ProjectRootCache::new());
        assert_eq!(report.status, AgentStatus::Ok);
        assert_eq!(report.totals.cost, Some(1.2345));
        assert_eq!(report.totals.output_tokens, 500);
        assert_eq!(report.projects.len(), 1);
        assert_eq!(report.projects[0].totals.cost, Some(1.2345));
        assert_eq!(
            report.totals.cost_basis,
            Some(CostBasis::Reported),
            "opencode's own cost column is a reported bill, never an estimate"
        );
    }

    /// Two different worktrees inside the same repository (the repo root and
    /// a subfolder) must aggregate into one project row instead of two, once
    /// the resolved project root is used as the bucket key.
    #[test]
    fn two_worktrees_in_the_same_repo_merge_into_one_project_row() {
        let dir = tempdir().unwrap();
        let repo_root = dir.path().join("repo");
        std::fs::create_dir_all(repo_root.join(".git")).unwrap();
        let subfolder = repo_root.join("app").join("src-tauri");
        std::fs::create_dir_all(&subfolder).unwrap();
        let root_worktree = repo_root.to_string_lossy().replace('\\', "/");
        let sub_worktree = subfolder.to_string_lossy().replace('\\', "/");

        let db_path = dir.path().join("opencode.db");
        let conn = make_db(&db_path);
        conn.execute(
            "INSERT INTO project (id, worktree) VALUES ('p1', ?1), ('p2', ?2)",
            rusqlite::params![root_worktree, sub_worktree],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO session (id, project_id, time_updated, cost, tokens_input, tokens_output, tokens_reasoning, tokens_cache_read, tokens_cache_write)
             VALUES ('s1', 'p1', ?1, 1.0, 100, 10, 0, 0, 0),
                    ('s2', 'p2', ?1, 2.0, 100, 20, 0, 0, 0)",
            [ms(2026, 9, 15)],
        )
        .unwrap();
        drop(conn);

        let (start, end) = window();
        let mut cache = ProjectRootCache::with_policy(crate::usage::RootPolicy {
            temp_dirs: Vec::new(),
            home: Some(dir.path().to_path_buf()),
        });
        let report = read_usage(&db_path, start, end, &mut cache);
        assert_eq!(report.projects.len(), 1, "both worktrees share the same repo root");
        assert_eq!(report.projects[0].totals.output_tokens, 30, "tokens from both worktrees must sum");
        assert_eq!(report.projects[0].totals.cost, Some(3.0), "cost from both worktrees must sum");
        assert_eq!(report.projects[0].name, "repo");
    }

    #[test]
    fn a_real_zero_cost_stays_zero_not_missing() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("opencode.db");
        let conn = make_db(&path);
        conn.execute(
            "INSERT INTO project (id, worktree) VALUES ('p1', 'C:/repos/local-model')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO session (id, project_id, time_updated, cost, tokens_input, tokens_output, tokens_reasoning, tokens_cache_read, tokens_cache_write)
             VALUES ('s1', 'p1', ?1, 0.0, 100, 50, 0, 0, 0)",
            [ms(2026, 9, 15)],
        )
        .unwrap();
        drop(conn);

        let (start, end) = window();
        let report = read_usage(&path, start, end, &mut ProjectRootCache::new());
        assert_eq!(report.totals.cost, Some(0.0), "a real zero must be reported, not treated as missing");
    }

    #[test]
    fn window_filtering_excludes_sessions_outside_the_range() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("opencode.db");
        let conn = make_db(&path);
        conn.execute(
            "INSERT INTO project (id, worktree) VALUES ('p1', 'C:/repos/orbitbar')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO session (id, project_id, time_updated, cost, tokens_input, tokens_output, tokens_reasoning, tokens_cache_read, tokens_cache_write)
             VALUES ('in', 'p1', ?1, 1.0, 100, 50, 0, 0, 0),
                    ('out', 'p1', ?2, 999.0, 999, 999, 0, 0, 0)",
            rusqlite::params![ms(2026, 9, 15), ms(2026, 8, 31)],
        )
        .unwrap();
        drop(conn);

        let (start, end) = window();
        let report = read_usage(&path, start, end, &mut ProjectRootCache::new());
        assert_eq!(report.totals.entries, 1);
        assert_eq!(report.totals.output_tokens, 50);
    }

    #[test]
    fn empty_database_is_ok_with_empty_totals() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("opencode.db");
        make_db(&path);

        let (start, end) = window();
        let report = read_usage(&path, start, end, &mut ProjectRootCache::new());
        assert_eq!(report.status, AgentStatus::Ok);
        assert_eq!(report.totals.entries, 0);
        assert_eq!(report.totals.cost, None);
        assert!(report.projects.is_empty());
    }

    /// Sessions whose worktree is scratch (under the temp dir) or a folder
    /// outside any repository are not counted anywhere: not in the project
    /// rows and not in the totals, so the split still sums to the totals.
    #[test]
    fn sessions_outside_any_project_are_left_out_of_totals_and_rows() {
        use crate::usage::RootPolicy;
        let dir = tempdir().unwrap();
        let temp = dir.path().join("Temp");
        let plain = dir.path().join("plain");
        std::fs::create_dir_all(&plain).unwrap();
        let kept = dir.path().join("kept-app").join("app");

        let path = dir.path().join("opencode.db");
        let conn = make_db(&path);
        conn.execute(
            "INSERT INTO project (id, worktree) VALUES ('p1', ?1), ('p2', ?2), ('p3', ?3)",
            rusqlite::params![
                kept.to_string_lossy(),
                temp.join("run-1").to_string_lossy(),
                plain.to_string_lossy()
            ],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO session (id, project_id, time_updated, cost, tokens_input, tokens_output, tokens_reasoning, tokens_cache_read, tokens_cache_write)
             VALUES ('s1', 'p1', ?1, 1.0, 100, 10, 0, 0, 0),
                    ('s2', 'p2', ?1, 20.0, 100, 500, 0, 0, 0),
                    ('s3', 'p3', ?1, 30.0, 100, 700, 0, 0, 0)",
            [ms(2026, 9, 15)],
        )
        .unwrap();
        drop(conn);

        let mut cache = ProjectRootCache::with_policy(RootPolicy {
            temp_dirs: vec![temp],
            home: Some(dir.path().to_path_buf()),
        });
        let (start, end) = window();
        let report = read_usage(&path, start, end, &mut cache);

        assert_eq!(report.totals.entries, 1);
        assert_eq!(report.totals.output_tokens, 10);
        assert_eq!(report.totals.cost, Some(1.0));
        assert_eq!(report.projects.len(), 1);
        assert_eq!(report.projects[0].name, "kept-app");
        assert!(report.detail.contains("2 sessions outside any project not counted"), "{}", report.detail);
    }

    /// When every session is excluded there is nothing left to bill: the cost
    /// goes back to "no value", not a leftover 0.0.
    #[test]
    fn when_everything_is_excluded_the_totals_are_empty_with_no_cost() {
        use crate::usage::RootPolicy;
        let dir = tempdir().unwrap();
        let temp = dir.path().join("Temp");
        let path = dir.path().join("opencode.db");
        let conn = make_db(&path);
        conn.execute(
            "INSERT INTO project (id, worktree) VALUES ('p1', ?1)",
            [temp.join("run-1").to_string_lossy()],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO session (id, project_id, time_updated, cost, tokens_input, tokens_output, tokens_reasoning, tokens_cache_read, tokens_cache_write)
             VALUES ('s1', 'p1', ?1, 2.5, 100, 50, 0, 0, 0)",
            [ms(2026, 9, 15)],
        )
        .unwrap();
        drop(conn);

        let mut cache = ProjectRootCache::with_policy(RootPolicy {
            temp_dirs: vec![temp],
            home: Some(dir.path().to_path_buf()),
        });
        let (start, end) = window();
        let report = read_usage(&path, start, end, &mut cache);

        assert_eq!(report.totals.entries, 0);
        assert_eq!(report.totals.cost, None);
        assert_eq!(report.totals.cost_basis, None);
        assert!(report.projects.is_empty());
    }
}
