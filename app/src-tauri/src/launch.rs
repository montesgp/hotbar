//! `run:<program> [args]` item action: parse the command line and spawn the
//! program directly.
//!
//! There is deliberately no shell in the path. The program is executed with
//! `std::process::Command` and its arguments are passed as a vector, so `&&`,
//! `|`, `>` and `%VAR%` are ordinary characters of an argument, not operators.

use std::ffi::OsStr;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use crate::config::Item;

/// A parsed `run:` command line.
#[derive(Debug, PartialEq, Eq)]
pub struct CommandLine {
    pub program: String,
    pub args: Vec<String>,
}

/// Splits `input` on whitespace into a program and its arguments. A double
/// quote groups characters, whitespace included, into one argument; the quotes
/// themselves are dropped. An empty pair of quotes yields an empty argument.
/// There is no escape syntax: a backslash is an ordinary character, which keeps
/// Windows paths (`C:\Tools\app.exe`) intact.
pub fn parse_command_line(input: &str) -> Result<CommandLine, String> {
    let mut tokens: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut in_token = false;
    let mut in_quotes = false;

    for ch in input.chars() {
        match ch {
            '"' => {
                in_quotes = !in_quotes;
                in_token = true;
            }
            c if c.is_whitespace() && !in_quotes => {
                if in_token {
                    tokens.push(std::mem::take(&mut current));
                    in_token = false;
                }
            }
            c => {
                current.push(c);
                in_token = true;
            }
        }
    }
    if in_quotes {
        return Err("unterminated double quote in command".into());
    }
    if in_token {
        tokens.push(current);
    }

    let mut tokens = tokens.into_iter();
    match tokens.next() {
        Some(program) if !program.is_empty() => Ok(CommandLine {
            program,
            args: tokens.collect(),
        }),
        _ => Err("no program to run".into()),
    }
}

/// Resolves a cell's `run:` action from the configured items. The frontend
/// sends only the item id; the command line always comes from the config the
/// backend loads itself, so the IPC command cannot be used to run an arbitrary
/// string.
pub fn command_for_item(items: &[Item], id: &str) -> Result<CommandLine, String> {
    command_for_item_with_prefix(items, id, "run:")
}

fn command_for_item_with_prefix(items: &[Item], id: &str, prefix: &str) -> Result<CommandLine, String> {
    let item = items
        .iter()
        .find(|item| item.id == id)
        .ok_or_else(|| format!("no item with id \"{id}\""))?;
    let command = item
        .action
        .strip_prefix(prefix)
        .ok_or_else(|| format!("item \"{id}\" is not a {prefix} action"))?;
    parse_command_line(command)
}

/// Resolves a cell's `panel:` action, with exactly the parsing and validation
/// of `run:` (same quoting rules, no shell, id looked up in the backend's own
/// config). The two prefixes never cross: a `run:` item is refused here and a
/// `panel:` item is refused by `command_for_item`.
pub fn panel_command_for_item(items: &[Item], id: &str) -> Result<CommandLine, String> {
    command_for_item_with_prefix(items, id, "panel:")
}

/// PATHEXT fallback when the variable is unset.
const DEFAULT_PATHEXT: &str = ".COM;.EXE;.BAT;.CMD";

/// Finds `program` the way a Windows shell does: for a bare name (no
/// extension, no path separator) it tries every `PATH` directory, in order,
/// with every `PATHEXT` extension, in order, and returns the first regular
/// file. `None` for anything else, so the caller falls back to its own lookup.
///
/// `Command::new("code")` alone does not work for VS Code: std's own `PATH`
/// search appends only `.exe`, and `code` is a `code.cmd` shim. Resolving here
/// and spawning the full path lets std run it. Since Rust 1.77.2 std escapes
/// the arguments of a `.bat`/`.cmd` target itself (and refuses ones it cannot
/// escape safely), so spawning a batch file directly stays shell-free and
/// `&`, `|` and `%VAR%` in an argument are still plain text.
///
/// `path` and `pathext` are parameters so the search is testable; the caller
/// passes the environment. Used on Windows only.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn resolve_program(program: &str, path: Option<&OsStr>, pathext: Option<&OsStr>) -> Option<PathBuf> {
    if program.is_empty() || program.contains(['/', '\\']) || std::path::Path::new(program).extension().is_some() {
        return None;
    }
    let pathext = pathext.and_then(OsStr::to_str).unwrap_or(DEFAULT_PATHEXT);
    let extensions: Vec<&str> = pathext.split(';').filter(|e| !e.is_empty()).collect();
    std::env::split_paths(path?)
        .filter(|dir| !dir.as_os_str().is_empty())
        .flat_map(|dir| extensions.iter().map(move |ext| dir.join(format!("{program}{ext}"))))
        .find(|candidate| candidate.is_file())
}

/// The program to execute for `line`: on Windows a bare name is resolved
/// through `PATH` and `PATHEXT` (see `resolve_program`), anything else is used
/// as written. Shared by `run:` and `panel:` so both find the same program.
pub fn resolved_program(line: &CommandLine) -> std::ffi::OsString {
    #[cfg(windows)]
    {
        resolve_program(
            &line.program,
            std::env::var_os("PATH").as_deref(),
            std::env::var_os("PATHEXT").as_deref(),
        )
        .map(PathBuf::into_os_string)
        .unwrap_or_else(|| line.program.clone().into())
    }
    #[cfg(not(windows))]
    {
        line.program.clone().into()
    }
}

/// Spawns `line` detached from the app: nothing is waited on, and stdio is
/// disconnected so the child never blocks on the app's (absent) console.
///
/// No Windows creation flags are set on purpose. Orbitbar is a GUI-subsystem
/// process without a console, so a console program (a terminal, a script
/// runner) gets its own visible console window, which is what a user launching
/// it wants; a GUI program such as `code` or `notepad.exe` shows no console at
/// all. `CREATE_NO_WINDOW` would hide the console of a program the user
/// launched to see.
pub fn spawn(line: &CommandLine) -> Result<(), String> {
    let mut command = Command::new(resolved_program(line));
    command
        .args(&line.args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // Own process group: a signal sent to the app's group does not reach it.
        command.process_group(0);
    }
    let mut child = command
        .spawn()
        .map_err(|e| format!("cannot run {}: {e}", line.program))?;
    // Reap the child when it exits so it does not linger as a zombie on Unix.
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(input: &str) -> CommandLine {
        parse_command_line(input).unwrap()
    }

    fn args(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn a_bare_program_has_no_arguments() {
        let l = line("notepad.exe");
        assert_eq!(l.program, "notepad.exe");
        assert!(l.args.is_empty());
    }

    #[test]
    fn arguments_split_on_whitespace() {
        let l = line("code ~/projects/my-app");
        assert_eq!(l.program, "code");
        assert_eq!(l.args, args(&["~/projects/my-app"]));
    }

    #[test]
    fn extra_and_mixed_whitespace_is_ignored() {
        let l = line("  open \t -a   Terminal  ");
        assert_eq!(l.program, "open");
        assert_eq!(l.args, args(&["-a", "Terminal"]));
    }

    #[test]
    fn double_quotes_group_an_argument_with_spaces() {
        let l = line(r#"code "C:\My Projects\app" --reuse-window"#);
        assert_eq!(l.program, "code");
        assert_eq!(l.args, args(&[r"C:\My Projects\app", "--reuse-window"]));
    }

    #[test]
    fn the_program_itself_may_be_quoted() {
        let l = line(r#""C:\Program Files\App\app.exe" --flag"#);
        assert_eq!(l.program, r"C:\Program Files\App\app.exe");
        assert_eq!(l.args, args(&["--flag"]));
    }

    #[test]
    fn quotes_inside_a_token_join_the_pieces() {
        assert_eq!(line(r#"tool --path="a b""#).args, args(&["--path=a b"]));
    }

    #[test]
    fn empty_quotes_make_an_empty_argument() {
        assert_eq!(line(r#"tool "" x"#).args, args(&["", "x"]));
    }

    #[test]
    fn shell_operators_are_plain_argument_text() {
        let l = line("echo a && b | c > d");
        assert_eq!(l.program, "echo");
        assert_eq!(l.args, args(&["a", "&&", "b", "|", "c", ">", "d"]));
    }

    #[test]
    fn an_empty_or_blank_command_is_an_error() {
        for input in ["", "   ", "\t"] {
            assert!(parse_command_line(input).is_err(), "{input:?}");
        }
    }

    #[test]
    fn an_empty_quoted_program_is_an_error() {
        assert!(parse_command_line(r#""" arg"#).is_err());
    }

    #[test]
    fn an_unterminated_quote_is_an_error() {
        assert!(parse_command_line(r#"code "C:\My Projects"#).is_err());
    }

    #[test]
    fn spawning_a_missing_program_reports_it() {
        let err = spawn(&line("orbitbar-no-such-program-xyz --flag")).unwrap_err();
        assert!(err.contains("orbitbar-no-such-program-xyz"), "{err}");
    }

    fn item(id: &str, action: &str) -> Item {
        Item {
            id: id.into(),
            label: id.into(),
            glyph: "0x2605".into(),
            action: action.into(),
            tooltip: String::new(),
            example: false,
        }
    }

    #[test]
    fn a_run_item_resolves_to_its_program_and_args() {
        let items = [item("a", "open:https://x.test"), item("editor", r#"run:code "my app" --new"#)];
        let l = command_for_item(&items, "editor").unwrap();
        assert_eq!(l.program, "code");
        assert_eq!(l.args, args(&["my app", "--new"]));
    }

    #[test]
    fn an_unknown_id_is_an_error() {
        let err = command_for_item(&[item("a", "run:code")], "missing").unwrap_err();
        assert!(err.contains("missing"), "{err}");
    }

    #[test]
    fn a_non_run_action_is_refused() {
        for action in ["open:https://x.test", "toggle-autostart", "agent-usage", "runcode", ""] {
            let err = command_for_item(&[item("a", action)], "a").unwrap_err();
            assert!(err.contains("run:"), "{action:?}: {err}");
        }
    }

    #[test]
    fn a_panel_item_resolves_like_a_run_item() {
        let items = [item("status", r#"panel:powershell -NoProfile -File "C:\My Tools\status.ps1" -Once"#)];
        let l = panel_command_for_item(&items, "status").unwrap();
        assert_eq!(l.program, "powershell");
        assert_eq!(l.args, args(&["-NoProfile", "-File", r"C:\My Tools\status.ps1", "-Once"]));
    }

    #[test]
    fn panel_and_run_prefixes_never_cross() {
        let items = [item("r", "run:notepad"), item("p", "panel:notepad")];
        let err = panel_command_for_item(&items, "r").unwrap_err();
        assert!(err.contains("panel:"), "{err}");
        let err = command_for_item(&items, "p").unwrap_err();
        assert!(err.contains("run:"), "{err}");
    }

    #[test]
    fn a_panel_item_needs_a_known_id_and_a_program() {
        assert!(panel_command_for_item(&[item("p", "panel:x")], "other").is_err());
        assert!(panel_command_for_item(&[item("p", "panel:   ")], "p").is_err());
        assert!(panel_command_for_item(&[item("p", "agent-usage")], "p").is_err());
    }

    #[test]
    fn a_run_item_with_no_program_is_an_error() {
        assert!(command_for_item(&[item("a", "run:   ")], "a").is_err());
    }

    /// A directory holding empty files with the given names.
    fn dir_with(names: &[&str]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for name in names {
            std::fs::write(dir.path().join(name), b"").unwrap();
        }
        dir
    }

    fn join(dirs: &[&std::path::Path]) -> std::ffi::OsString {
        std::env::join_paths(dirs).unwrap()
    }

    #[test]
    fn a_bare_name_resolves_through_pathext() {
        let dir = dir_with(&["code.cmd"]);
        let path = join(&[dir.path()]);
        let found = resolve_program("code", Some(&path), Some(OsStr::new(".com;.exe;.bat;.cmd")));
        assert_eq!(found, Some(dir.path().join("code.cmd")));
    }

    #[test]
    fn pathext_order_decides_between_candidates() {
        let dir = dir_with(&["tool.bat", "tool.exe"]);
        let path = join(&[dir.path()]);
        let found = resolve_program("tool", Some(&path), Some(OsStr::new(".exe;.bat")));
        assert_eq!(found, Some(dir.path().join("tool.exe")));
    }

    #[test]
    fn earlier_path_directories_win() {
        let first = dir_with(&["tool.cmd"]);
        let second = dir_with(&["tool.exe"]);
        let path = join(&[first.path(), second.path()]);
        let found = resolve_program("tool", Some(&path), Some(OsStr::new(".exe;.cmd")));
        assert_eq!(found, Some(first.path().join("tool.cmd")));
    }

    #[test]
    fn an_unset_pathext_uses_the_windows_default() {
        // The default extensions are upper case, so the file name matches them
        // exactly on case-sensitive file systems too.
        let dir = dir_with(&["code.CMD"]);
        let path = join(&[dir.path()]);
        assert_eq!(resolve_program("code", Some(&path), None), Some(dir.path().join("code.CMD")));
    }

    #[test]
    fn extensionless_files_and_directories_do_not_match() {
        let dir = dir_with(&["code"]);
        std::fs::create_dir(dir.path().join("code.cmd")).unwrap();
        let path = join(&[dir.path()]);
        assert_eq!(resolve_program("code", Some(&path), Some(OsStr::new(".cmd"))), None);
    }

    #[test]
    fn a_name_with_an_extension_or_a_separator_is_left_alone() {
        let dir = dir_with(&["code.cmd", "app.exe"]);
        let path = join(&[dir.path()]);
        let ext = Some(OsStr::new(".cmd;.exe"));
        assert_eq!(resolve_program("code.cmd", Some(&path), ext), None);
        assert_eq!(resolve_program("sub/code", Some(&path), ext), None);
        assert_eq!(resolve_program(r"sub\code", Some(&path), ext), None);
    }

    #[test]
    fn a_missing_program_or_path_resolves_to_nothing() {
        let dir = dir_with(&[]);
        let path = join(&[dir.path()]);
        assert_eq!(resolve_program("nope", Some(&path), None), None);
        assert_eq!(resolve_program("nope", None, None), None);
    }
}
