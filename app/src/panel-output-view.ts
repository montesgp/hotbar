/**
 * Pure view-model for a `panel:` action's result: no DOM and no Tauri, so it is
 * unit-testable once a frontend test runner exists (see usage-view.ts).
 */

/** What Rust's `run_panel_command` returns (see src-tauri/src/panel.rs). */
export interface PanelOutput {
  /** stdout, already stripped of escape sequences. */
  text: string;
  stderr: string;
  /** null when the process was killed. */
  exitCode: number | null;
  timedOut: boolean;
  /** Either stream was longer than the cap and was cut. */
  truncated: boolean;
}

/** Mirrors `panel::TIMEOUT` and `panel::MAX_OUTPUT_BYTES` in Rust, for the messages only. */
const TIMEOUT_SECONDS = 10;
const MAX_OUTPUT_KIB = 64;

export interface PanelOutputView {
  /** The preformatted body: stdout, or a placeholder when it is empty. */
  text: string;
  /** True when `text` is the placeholder for empty output, not real output. */
  empty: boolean;
  /** One short line per problem, in the order to show them. */
  notes: string[];
  /** stderr to show under the output, or "" when there is none. */
  stderr: string;
}

export function describePanelOutput(out: PanelOutput): PanelOutputView {
  const notes: string[] = [];
  if (out.timedOut) {
    notes.push(`Timed out after ${TIMEOUT_SECONDS} s and was stopped.`);
  } else if (out.exitCode !== null && out.exitCode !== 0) {
    notes.push(`Exited with code ${out.exitCode}.`);
  }
  if (out.truncated) notes.push(`Output was cut at ${MAX_OUTPUT_KIB} KiB.`);
  const empty = out.text.trim() === "";
  return {
    text: empty ? "(no output)" : out.text,
    empty,
    notes,
    stderr: out.stderr,
  };
}
