//! Opencode usage reader: SQLite `session` table at
//! `~/.local/share/opencode/opencode.db`. Ported from `Get-OpenCodeAgentHistory`
//! in `legacy/windows-widget/lib/Get-AgentUsage.ps1`.
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

use crate::usage::{
    normalize_project_path, project_display_name, AgentUsageReport, CostBasis, ProjectUsage, UsageTotals,
};
use chrono::{DateTime, Local};
use rusqlite::{Connection, OpenFlags};
use std::path::Path;

const AGENT: &str = "opencode";

/// Reads window usage from the opencode SQLite store at `db_path`.
pub fn read_usage(db_path: &Path, start: DateTime<Local>, end: DateTime<Local>) -> AgentUsageReport {
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

    let totals = match read_totals(&conn, start_ms, end_ms) {
        Ok(t) => t,
        Err(e) => return AgentUsageReport::error(AGENT, format!("cannot read session totals: {e}")),
    };

    let projects = match read_projects(&conn, start_ms, end_ms) {
        Ok(p) => p,
        Err(e) => return AgentUsageReport::error(AGENT, format!("cannot read per-project totals: {e}")),
    };

    let detail = format!("{} projects", projects.len());
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

fn read_projects(conn: &Connection, start_ms: i64, end_ms: i64) -> rusqlite::Result<Vec<ProjectUsage>> {
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

    let mut out = Vec::new();
    for row in rows {
        let (worktree, totals) = row?;
        let key = normalize_project_path(&worktree);
        if key.is_empty() {
            continue;
        }
        out.push(ProjectUsage {
            name: project_display_name(&key),
            path: key,
            model: String::new(),
            totals,
        });
    }
    Ok(out)
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
        let report = read_usage(&missing, start, end);
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
        let report = read_usage(&path, start, end);
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
        let report = read_usage(&path, start, end);
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
        let report = read_usage(&path, start, end);
        assert_eq!(report.totals.entries, 1);
        assert_eq!(report.totals.output_tokens, 50);
    }

    #[test]
    fn empty_database_is_ok_with_empty_totals() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("opencode.db");
        make_db(&path);

        let (start, end) = window();
        let report = read_usage(&path, start, end);
        assert_eq!(report.status, AgentStatus::Ok);
        assert_eq!(report.totals.entries, 0);
        assert_eq!(report.totals.cost, None);
        assert!(report.projects.is_empty());
    }
}
