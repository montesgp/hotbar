//! `run:<program> [args]` item action: parse the command line and spawn the
//! program directly.
//!
//! There is deliberately no shell in the path. The program is executed with
//! `std::process::Command` and its arguments are passed as a vector, so `&&`,
//! `|`, `>` and `%VAR%` are ordinary characters of an argument, not operators.

use std::process::{Command, Stdio};

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

/// Spawns `input` detached from the app: nothing is waited on, and stdio is
/// disconnected so the child never blocks on the app's (absent) console.
///
/// No Windows creation flags are set on purpose. Orbitbar is a GUI-subsystem
/// process without a console, so a console program (a terminal, a script
/// runner) gets its own visible console window, which is what a user launching
/// it wants; a GUI program such as `code` or `notepad.exe` shows no console at
/// all. `CREATE_NO_WINDOW` would hide the console of a program the user
/// launched to see.
pub fn spawn(input: &str) -> Result<(), String> {
    let line = parse_command_line(input)?;
    let mut command = Command::new(&line.program);
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
        let err = spawn("orbitbar-no-such-program-xyz --flag").unwrap_err();
        assert!(err.contains("orbitbar-no-such-program-xyz"), "{err}");
    }
}
