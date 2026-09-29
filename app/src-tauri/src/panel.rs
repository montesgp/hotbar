//! `panel:<program> [args]` item action: run a program once, capture what it
//! prints and hand it to the panel.
//!
//! The command line is parsed and resolved exactly like `run:` (see
//! `launch.rs`): no shell, arguments as a vector, the frontend sends only the
//! item id. What differs is the run itself: no console window, stdin closed,
//! output captured with a hard size cap, and a hard timeout after which the
//! process is killed. The text is stripped of terminal escape sequences before
//! it reaches the webview, which renders it as plain preformatted text.

use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::launch::CommandLine;

/// A command that has not finished by then is killed.
pub const TIMEOUT: Duration = Duration::from_secs(10);

/// At most this many bytes are kept from each of stdout and stderr, so a
/// runaway program cannot flood the panel.
pub const MAX_OUTPUT_BYTES: usize = 64 * 1024;

/// What the panel shows for one run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PanelOutput {
    /// stdout, cleaned up: no escape sequences, no carriage returns.
    pub text: String,
    /// stderr, cleaned the same way.
    pub stderr: String,
    /// `None` when the process was killed (timeout) or has no exit code.
    pub exit_code: Option<i32>,
    pub timed_out: bool,
    /// Either stream was longer than the cap and was cut.
    pub truncated: bool,
}

/// Removes terminal escape sequences: CSI (`ESC [ ... final`, colors, cursor
/// movement, erase), OSC (`ESC ] ... BEL` or `ESC \`, window titles) and the
/// short two-character `ESC x` forms. An escape cut off by the end of the text
/// is dropped. Everything else, including non-ASCII text, is kept.
pub fn strip_ansi(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\u{1b}' {
            out.push(c);
            continue;
        }
        match chars.next() {
            // A lone ESC at the very end.
            None => {}
            // CSI: parameters and intermediates, then a final byte 0x40-0x7E.
            Some('[') => {
                for next in chars.by_ref() {
                    if ('\u{40}'..='\u{7e}').contains(&next) {
                        break;
                    }
                }
            }
            // OSC: up to BEL or the string terminator `ESC \`.
            Some(']') => {
                while let Some(next) = chars.next() {
                    if next == '\u{7}' {
                        break;
                    }
                    if next == '\u{1b}' {
                        if chars.peek() == Some(&'\\') {
                            chars.next();
                        }
                        break;
                    }
                }
            }
            // `ESC 7`, `ESC 8`, `ESC =` and the like.
            Some(_) => {}
        }
    }
    out
}

/// The first `max` bytes of `bytes` as text, and whether anything was cut. A
/// multi-byte character split by the cut is dropped whole instead of turning
/// into a replacement character.
pub fn cap_bytes(bytes: &[u8], max: usize) -> (String, bool) {
    if bytes.len() <= max {
        return (String::from_utf8_lossy(bytes).into_owned(), false);
    }
    let cut = &bytes[..max];
    let text = match std::str::from_utf8(cut) {
        Ok(text) => text.to_string(),
        // The cut split the last character: drop it whole.
        Err(e) if e.error_len().is_none() => String::from_utf8_lossy(&cut[..e.valid_up_to()]).into_owned(),
        Err(_) => String::from_utf8_lossy(cut).into_owned(),
    };
    (text, true)
}

/// Builds the panel result from the raw captured streams: cap, strip escapes,
/// normalize line endings, trim leading blank lines and trailing whitespace.
pub fn shape_output(stdout: &[u8], stderr: &[u8], exit_code: Option<i32>, timed_out: bool, max: usize) -> PanelOutput {
    let (out, out_cut) = cap_bytes(stdout, max);
    let (err, err_cut) = cap_bytes(stderr, max);
    PanelOutput {
        text: tidy(&out),
        stderr: tidy(&err),
        exit_code,
        timed_out,
        truncated: out_cut || err_cut,
    }
}

/// Escape sequences and carriage returns out, leading blank lines (a frame
/// that starts with an empty line) and trailing whitespace off; the
/// indentation of the first real line stays, since output is often aligned.
fn tidy(text: &str) -> String {
    let clean = strip_ansi(text).replace('\r', "");
    let mut skip = 0;
    for line in clean.split_inclusive('\n') {
        if !line.trim().is_empty() {
            break;
        }
        skip += line.len();
    }
    clean[skip..].trim_end().to_string()
}

/// What a reader thread has collected from one pipe so far, plus a signal for
/// when the pipe closed.
struct Capture {
    bytes: Arc<Mutex<Vec<u8>>>,
    closed: Receiver<()>,
}

impl Capture {
    /// Reads `pipe` on its own thread so a full pipe never stalls the child.
    /// Keeps at most `max + 1` bytes (one over, so the cap can tell the output
    /// was longer) and discards the rest while still draining.
    fn start<R: Read + Send + 'static>(pipe: Option<R>, max: usize) -> Self {
        let bytes = Arc::new(Mutex::new(Vec::new()));
        let (tx, closed) = mpsc::channel();
        let shared = Arc::clone(&bytes);
        std::thread::spawn(move || {
            if let Some(mut pipe) = pipe {
                let mut chunk = [0u8; 8192];
                loop {
                    match pipe.read(&mut chunk) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            let mut kept = shared.lock().unwrap_or_else(|e| e.into_inner());
                            let room = (max + 1).saturating_sub(kept.len());
                            kept.extend_from_slice(&chunk[..n.min(room)]);
                        }
                    }
                }
            }
            let _ = tx.send(());
        });
        Self { bytes, closed }
    }

    /// What was read. Waits a moment for the pipe to close, but not forever: a
    /// grandchild the program left running can hold the pipe open after the
    /// program itself is gone or killed.
    fn finish(self, wait: Duration) -> Vec<u8> {
        let _ = self.closed.recv_timeout(wait);
        let kept = self.bytes.lock().unwrap_or_else(|e| e.into_inner());
        kept.clone()
    }
}

/// How long to wait for the output pipes to close once the process is gone.
const PIPE_GRACE: Duration = Duration::from_secs(1);

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Runs `line` to completion (or `timeout`) and shapes the result. No console
/// window on Windows, stdin is closed, stdout and stderr are captured up to
/// `max` bytes each. A program that cannot be started is an `Err`; one that
/// runs and fails is an `Ok` with its exit code and stderr.
pub fn run_captured(line: &CommandLine, timeout: Duration, max: usize) -> Result<PanelOutput, String> {
    let mut command = Command::new(crate::launch::resolved_program(line));
    command
        .args(&line.args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // Unlike `run:`, nothing here is meant to be seen: no console window,
        // not even for a console program such as powershell.
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = command
        .spawn()
        .map_err(|e| format!("cannot run {}: {e}", line.program))?;

    let stdout = Capture::start(child.stdout.take(), max);
    let stderr = Capture::start(child.stderr.take(), max);

    let deadline = Instant::now() + timeout;
    let (exit_code, timed_out) = loop {
        match child.try_wait() {
            Ok(Some(status)) => break (status.code(), false),
            Ok(None) => {}
            Err(e) => return Err(format!("cannot wait for {}: {e}", line.program)),
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            break (None, true);
        }
        std::thread::sleep(Duration::from_millis(15));
    };

    let out = stdout.finish(PIPE_GRACE);
    let err = stderr.finish(PIPE_GRACE);
    Ok(shape_output(&out, &err, exit_code, timed_out, max))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    // ---- strip_ansi ----

    #[test]
    fn colors_and_resets_are_removed() {
        assert_eq!(strip_ansi("\x1b[31mred\x1b[0m and \x1b[1;32mgreen\x1b[m"), "red and green");
    }

    #[test]
    fn cursor_and_erase_sequences_are_removed() {
        assert_eq!(strip_ansi("\x1b[H\x1b[2Jframe\x1b[K\n\x1b[J"), "frame\n");
        assert_eq!(strip_ansi("a\x1b[10;20Hb\x1b[?25lc"), "abc");
    }

    #[test]
    fn osc_titles_end_at_bel_or_string_terminator() {
        assert_eq!(strip_ansi("\x1b]0;my title\x07text"), "text");
        assert_eq!(strip_ansi("\x1b]0;my title\x1b\\text"), "text");
    }

    #[test]
    fn a_short_escape_and_a_truncated_escape_are_dropped() {
        assert_eq!(strip_ansi("a\x1b7b\x1b8c"), "abc");
        assert_eq!(strip_ansi("text\x1b"), "text");
        assert_eq!(strip_ansi("text\x1b[31"), "text");
    }

    #[test]
    fn plain_text_layout_and_unicode_are_untouched() {
        let frame = "  Estado: UP\n  \u{25cf} combo   [priority]  enabled\n\tcol\n";
        assert_eq!(strip_ansi(frame), frame);
    }

    // ---- cap_bytes ----

    #[test]
    fn output_within_the_cap_is_kept_whole() {
        assert_eq!(cap_bytes(b"hello", 5), ("hello".to_string(), false));
        assert_eq!(cap_bytes(b"", 5), (String::new(), false));
    }

    #[test]
    fn output_over_the_cap_is_cut_and_flagged() {
        assert_eq!(cap_bytes(b"hello world", 5), ("hello".to_string(), true));
    }

    #[test]
    fn a_cut_inside_a_multibyte_character_drops_the_whole_character() {
        // "é" is two bytes; a cap of 3 lands in the middle of the second one.
        let (text, cut) = cap_bytes("éé".as_bytes(), 3);
        assert_eq!(text, "é");
        assert!(cut);
        assert!(!text.contains('\u{fffd}'));
    }

    // ---- shape_output ----

    #[test]
    fn a_clean_run_carries_the_text_and_exit_code() {
        let out = shape_output(b"\x1b[32mUP\x1b[0m\r\nline 2\r\n\r\n", b"", Some(0), false, 1024);
        assert_eq!(
            out,
            PanelOutput {
                text: "UP\nline 2".to_string(),
                stderr: String::new(),
                exit_code: Some(0),
                timed_out: false,
                truncated: false,
            }
        );
    }

    #[test]
    fn leading_blank_lines_go_but_indentation_stays() {
        let out = shape_output(b"\n\n  OmniRoute\n  ====\n", b"", Some(0), false, 1024);
        assert_eq!(out.text, "  OmniRoute\n  ====");
    }

    #[test]
    fn stderr_is_captured_cleaned_and_kept_apart() {
        let out = shape_output(b"out", b"\x1b[31mboom\x1b[0m\r\n", Some(3), false, 1024);
        assert_eq!(out.text, "out");
        assert_eq!(out.stderr, "boom");
        assert_eq!(out.exit_code, Some(3));
    }

    #[test]
    fn either_stream_over_the_cap_sets_truncated() {
        let big = vec![b'x'; 300];
        assert!(shape_output(&big, b"", Some(0), false, 100).truncated);
        assert!(shape_output(b"", &big, Some(0), false, 100).truncated);
        let out = shape_output(&big, b"", Some(0), false, 100);
        assert_eq!(out.text.len(), 100);
    }

    #[test]
    fn a_timeout_keeps_the_partial_output_and_has_no_exit_code() {
        let out = shape_output(b"partial", b"", None, true, 1024);
        assert!(out.timed_out);
        assert_eq!(out.exit_code, None);
        assert_eq!(out.text, "partial");
    }

    // ---- run_captured (a real process) ----

    fn shell(script_windows: &str, script_unix: &str) -> CommandLine {
        if cfg!(windows) {
            CommandLine { program: "cmd".into(), args: vec!["/C".into(), script_windows.into()] }
        } else {
            CommandLine { program: "sh".into(), args: vec!["-c".into(), script_unix.into()] }
        }
    }

    #[test]
    fn a_process_output_exit_code_and_stderr_are_captured() {
        let line = shell("echo out& echo err 1>&2& exit 3", "echo out; echo err >&2; exit 3");
        let out = run_captured(&line, Duration::from_secs(20), 1024).unwrap();
        assert_eq!(out.text, "out");
        assert_eq!(out.stderr, "err");
        assert_eq!(out.exit_code, Some(3));
        assert!(!out.timed_out);
    }

    #[test]
    fn a_slow_process_is_killed_at_the_timeout() {
        let line = shell("echo started& ping -n 30 127.0.0.1 >nul", "echo started; sleep 30");
        let began = Instant::now();
        let out = run_captured(&line, Duration::from_millis(500), 1024).unwrap();
        assert!(out.timed_out);
        assert_eq!(out.exit_code, None);
        assert!(began.elapsed() < Duration::from_secs(10), "took {:?}", began.elapsed());
    }

    #[test]
    fn a_program_that_cannot_start_is_an_error() {
        let line = CommandLine { program: "orbitbar-no-such-program-xyz".into(), args: vec![] };
        let err = run_captured(&line, Duration::from_secs(5), 1024).unwrap_err();
        assert!(err.contains("cannot run"), "{err}");
    }

    #[test]
    fn a_process_writing_more_than_the_cap_is_cut_not_stalled() {
        let line = shell(
            "for /L %i in (1,1,3000) do @echo 0123456789012345678901234567890123456789",
            "yes 0123456789012345678901234567890123456789 | head -n 3000",
        );
        let out = run_captured(&line, Duration::from_secs(20), 1000).unwrap();
        assert!(out.truncated);
        assert!(out.text.len() <= 1000);
        assert!(!out.timed_out);
    }
}
