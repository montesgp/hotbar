//! Official list prices for the cloud models the agent stores report, used to
//! estimate money for claude and codex because neither jsonl store writes a
//! cost field. Verified against each provider's own pricing page (see the
//! `source`/`as_of` on every row) and extended with a user override file so
//! nobody is stuck waiting on this table for a model they already use.
//!
//! By design, claude/codex money is ESTIMATED from official prices times the
//! real tokens read from the store. opencode keeps its own real
//! `session.cost` and never goes through this table.
//!
//! A model with no row in the table (built-in or user override) produces NO
//! money at all - `None` - never a guess from a similar model. Inventing a
//! rate would be the same lie as printing a zero the readers refuse to print.
//!
//! Matching is exact id first, then the longest prefix, so a future provider
//! suffix (`claude-opus-5-5-20260926`) costs nothing to support. A user
//! override wins a tie against a built-in row with the exact same key; a
//! longer key (built-in or override) always wins over a shorter one
//! regardless of source.
//!
//! Cache pricing: when a model's row does not state an explicit cache rate,
//! this file applies Anthropic's published multipliers - cache read = 0.1x
//! input, cache write = 1.25x input for a 5-minute TTL and 2x input for a
//! 1-hour TTL - to that model's own input price. Claude's `cache_creation`
//! usage field is billed by TTL bucket
//! (`ephemeral_5m_input_tokens` / `ephemeral_1h_input_tokens`); the reader
//! passes each bucket through separately so a 1-hour cache write is never
//! priced as if it were a 5-minute one.

use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

/// One built-in price row. Amounts are $ per 1M tokens. `source` and `as_of`
/// are the auditor trail: not consumed by the estimator itself, but they are
/// why every number in this file can be checked against a real page instead
/// of taken on faith.
#[allow(dead_code)]
pub struct PriceEntry {
    pub id: &'static str,
    pub input: f64,
    pub output: f64,
    pub cache_read: f64,
    pub cache_write_5m: f64,
    pub cache_write_1h: f64,
    pub source: &'static str,
    pub as_of: &'static str,
}

/// Claude rows: Anthropic first-party API, USD per 1M tokens, as of
/// 2026-09-28. Source: https://platform.claude.com/docs/en/about-claude/pricing
/// Cache rates not published per-model use Anthropic's stated multipliers on
/// that row's own input price: cache read = 0.1x input, cache write (5m) =
/// 1.25x input, cache write (1h) = 2x input.
const CLAUDE_SOURCE: &str = "https://platform.claude.com/docs/en/about-claude/pricing";
const CLAUDE_AS_OF: &str = "2026-09-28";

/// OpenAI/codex rows: official API pricing page, USD per 1M tokens, as of
/// 2026-09-28. Source: https://developers.openai.com/api/docs/pricing
/// OpenAI's prompt caching has no separate "cache write" charge - a cache is
/// populated at the ordinary input price and has no TTL tier - so both
/// `cache_write_5m` and `cache_write_1h` below equal that row's `input`.
/// `cache_read` is OpenAI's published "cached input" rate.
const OPENAI_SOURCE: &str = "https://developers.openai.com/api/docs/pricing";
const OPENAI_AS_OF: &str = "2026-09-28";

/// Rows are $ per 1M tokens.
const PRICE_TABLE: &[PriceEntry] = &[
    // --- Claude (Anthropic first-party API) ---
    PriceEntry {
        id: "claude-fable-5-1",
        input: 10.0,
        output: 50.0,
        cache_read: 0.25,
        cache_write_5m: 12.5,
        cache_write_1h: 20.0,
        source: CLAUDE_SOURCE,
        as_of: CLAUDE_AS_OF,
    },
    PriceEntry {
        // Same rate card as fable-5-1, per the pricing page.
        id: "claude-mythos-5-1",
        input: 10.0,
        output: 50.0,
        cache_read: 0.25,
        cache_write_5m: 12.5,
        cache_write_1h: 20.0,
        source: CLAUDE_SOURCE,
        as_of: CLAUDE_AS_OF,
    },
    PriceEntry {
        id: "claude-fable-5",
        input: 10.0,
        output: 50.0,
        cache_read: 1.00,
        cache_write_5m: 12.5,
        cache_write_1h: 20.0,
        source: CLAUDE_SOURCE,
        as_of: CLAUDE_AS_OF,
    },
    PriceEntry {
        id: "claude-opus-5-5",
        input: 4.0,
        output: 20.0,
        cache_read: 0.20,
        cache_write_5m: 5.0,
        cache_write_1h: 8.0,
        source: CLAUDE_SOURCE,
        as_of: CLAUDE_AS_OF,
    },
    PriceEntry {
        id: "claude-opus-5",
        input: 5.0,
        output: 25.0,
        cache_read: 0.5,
        cache_write_5m: 6.25,
        cache_write_1h: 10.0,
        source: CLAUDE_SOURCE,
        as_of: CLAUDE_AS_OF,
    },
    PriceEntry {
        id: "claude-opus-4-8",
        input: 5.0,
        output: 25.0,
        cache_read: 0.5,
        cache_write_5m: 6.25,
        cache_write_1h: 10.0,
        source: CLAUDE_SOURCE,
        as_of: CLAUDE_AS_OF,
    },
    PriceEntry {
        id: "claude-opus-4-7",
        input: 5.0,
        output: 25.0,
        cache_read: 0.5,
        cache_write_5m: 6.25,
        cache_write_1h: 10.0,
        source: CLAUDE_SOURCE,
        as_of: CLAUDE_AS_OF,
    },
    PriceEntry {
        id: "claude-opus-4-6",
        input: 5.0,
        output: 25.0,
        cache_read: 0.5,
        cache_write_5m: 6.25,
        cache_write_1h: 10.0,
        source: CLAUDE_SOURCE,
        as_of: CLAUDE_AS_OF,
    },
    PriceEntry {
        id: "claude-sonnet-5",
        input: 2.0,
        output: 10.0,
        cache_read: 0.2,
        cache_write_5m: 2.5,
        cache_write_1h: 4.0,
        source: CLAUDE_SOURCE,
        as_of: CLAUDE_AS_OF,
    },
    PriceEntry {
        id: "claude-sonnet-4-6",
        input: 3.0,
        output: 15.0,
        cache_read: 0.3,
        cache_write_5m: 3.75,
        cache_write_1h: 6.0,
        source: CLAUDE_SOURCE,
        as_of: CLAUDE_AS_OF,
    },
    PriceEntry {
        id: "claude-haiku-4-5",
        input: 1.0,
        output: 5.0,
        cache_read: 0.1,
        cache_write_5m: 1.25,
        cache_write_1h: 2.0,
        source: CLAUDE_SOURCE,
        as_of: CLAUDE_AS_OF,
    },
    // --- OpenAI / codex (official API pricing page) ---
    PriceEntry {
        id: "gpt-5.6-terra",
        input: 2.0,
        output: 12.0,
        cache_read: 0.20,
        cache_write_5m: 2.0,
        cache_write_1h: 2.0,
        source: OPENAI_SOURCE,
        as_of: OPENAI_AS_OF,
    },
    PriceEntry {
        id: "gpt-5.6-sol",
        input: 4.0,
        output: 20.0,
        cache_read: 0.40,
        cache_write_5m: 4.0,
        cache_write_1h: 4.0,
        source: OPENAI_SOURCE,
        as_of: OPENAI_AS_OF,
    },
    PriceEntry {
        id: "gpt-5.5",
        input: 5.0,
        output: 30.0,
        cache_read: 0.50,
        cache_write_5m: 5.0,
        cache_write_1h: 5.0,
        source: OPENAI_SOURCE,
        as_of: OPENAI_AS_OF,
    },
    PriceEntry {
        id: "gpt-6-astra",
        input: 10.0,
        output: 50.0,
        cache_read: 1.00,
        cache_write_5m: 10.0,
        cache_write_1h: 10.0,
        source: OPENAI_SOURCE,
        as_of: OPENAI_AS_OF,
    },
    PriceEntry {
        id: "gpt-5.6-luna",
        input: 0.20,
        output: 1.20,
        cache_read: 0.02,
        cache_write_5m: 0.20,
        cache_write_1h: 0.20,
        source: OPENAI_SOURCE,
        as_of: OPENAI_AS_OF,
    },
    PriceEntry {
        id: "gpt-5.4-mini",
        input: 0.75,
        output: 4.50,
        cache_read: 0.075,
        cache_write_5m: 0.75,
        cache_write_1h: 0.75,
        source: OPENAI_SOURCE,
        as_of: OPENAI_AS_OF,
    },
    PriceEntry {
        id: "gpt-5.4",
        input: 2.50,
        output: 15.0,
        cache_read: 0.25,
        cache_write_5m: 2.50,
        cache_write_1h: 2.50,
        source: OPENAI_SOURCE,
        as_of: OPENAI_AS_OF,
    },
    PriceEntry {
        id: "gpt-5.3-codex",
        input: 1.75,
        output: 14.0,
        cache_read: 0.175,
        cache_write_5m: 1.75,
        cache_write_1h: 1.75,
        source: OPENAI_SOURCE,
        as_of: OPENAI_AS_OF,
    },
];

/// One user-supplied override row, parsed from `pricing.json`. Missing rates
/// fall back to the same default multipliers the built-in table's unstated
/// rows use, applied to `input`: cache read = 0.1x, cache write 5m = 1.25x,
/// cache write 1h = 2x.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PriceOverrideEntry {
    pub input: f64,
    pub output: f64,
    #[serde(default)]
    pub cache_read: Option<f64>,
    #[serde(default)]
    pub cache_write_5m: Option<f64>,
    #[serde(default)]
    pub cache_write_1h: Option<f64>,
}

/// The raw shape of `pricing.json`: `{ "models": { "<id or prefix>": {...} } }`.
#[derive(Debug, Default, Deserialize)]
struct PricingFile {
    #[serde(default)]
    models: HashMap<String, PriceOverrideEntry>,
}

/// User price overrides, keyed by model id or prefix exactly as written in
/// `pricing.json`. An empty table (the default) means "built-ins only" -
/// `find_price_entry` behaves exactly as before overrides existed.
#[derive(Debug, Default, Clone)]
pub struct PriceOverrides {
    pub(crate) models: HashMap<String, PriceOverrideEntry>,
}

/// Loads `pricing.json` from `path`. Three outcomes, none of them a crash:
/// - the file does not exist: built-ins only, no warning (the normal case
///   for anyone who has not written an override file yet);
/// - the file exists and parses: those overrides, no warning;
/// - the file exists but is malformed (bad JSON, wrong field types): built-
///   ins only, plus a warning string the caller should surface to the user -
///   a typo in a hand-edited price file must not take the whole usage panel
///   down, but it also must not fail silently.
pub fn load_overrides(path: &Path) -> (PriceOverrides, Option<String>) {
    if !path.exists() {
        return (PriceOverrides::default(), None);
    }

    let raw = match std::fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(e) => {
            return (
                PriceOverrides::default(),
                Some(format!("cannot read {}: {e}; using built-in prices only", path.display())),
            )
        }
    };
    let raw = raw.strip_prefix('\u{feff}').unwrap_or(&raw);

    match serde_json::from_str::<PricingFile>(raw) {
        Ok(file) => (PriceOverrides { models: file.models }, None),
        Err(e) => (
            PriceOverrides::default(),
            Some(format!("{} is invalid ({e}); using built-in prices only", path.display())),
        ),
    }
}

/// A resolved price row - built-in or override, defaults already applied -
/// ready for `estimate_cost`. Owned because an override's id is user text,
/// never `'static`.
#[derive(Debug, Clone, PartialEq)]
struct ResolvedPrice {
    id: String,
    input: f64,
    output: f64,
    cache_read: f64,
    cache_write_5m: f64,
    cache_write_1h: f64,
}

fn resolve_builtin(entry: &'static PriceEntry) -> ResolvedPrice {
    ResolvedPrice {
        id: entry.id.to_string(),
        input: entry.input,
        output: entry.output,
        cache_read: entry.cache_read,
        cache_write_5m: entry.cache_write_5m,
        cache_write_1h: entry.cache_write_1h,
    }
}

fn resolve_override(id: &str, entry: &PriceOverrideEntry) -> ResolvedPrice {
    ResolvedPrice {
        id: id.to_string(),
        input: entry.input,
        output: entry.output,
        cache_read: entry.cache_read.unwrap_or(entry.input * 0.1),
        cache_write_5m: entry.cache_write_5m.unwrap_or(entry.input * 1.25),
        cache_write_1h: entry.cache_write_1h.unwrap_or(entry.input * 2.0),
    }
}

/// Finds the price row for a model id: exact match first (case-insensitive,
/// override wins a tie against a built-in with the identical key), then the
/// longest matching prefix across both tables so a shorter key can never
/// shadow a longer one regardless of which table it came from.
fn find_price_entry(model: &str, overrides: &PriceOverrides) -> Option<ResolvedPrice> {
    let model = model.trim();
    if model.is_empty() {
        return None;
    }

    if let Some((key, entry)) = overrides.models.iter().find(|(k, _)| k.eq_ignore_ascii_case(model)) {
        return Some(resolve_override(key, entry));
    }
    if let Some(entry) = PRICE_TABLE.iter().find(|e| e.id.eq_ignore_ascii_case(model)) {
        return Some(resolve_builtin(entry));
    }

    // (key length, is_override, key) - longer keys win; on an equal length a
    // literal same-string tie was already handled above as an exact match,
    // so a length tie here only means two different keys of the same length
    // matched as prefixes, and this ordering breaks that arbitrary tie in
    // the override's favor rather than leaving it to iteration order.
    let mut candidates: Vec<(usize, bool, String)> = Vec::new();
    for key in overrides.models.keys() {
        if model.len() >= key.len() && model[..key.len()].eq_ignore_ascii_case(key) {
            candidates.push((key.len(), true, key.clone()));
        }
    }
    for entry in PRICE_TABLE.iter() {
        if model.len() >= entry.id.len() && model[..entry.id.len()].eq_ignore_ascii_case(entry.id) {
            candidates.push((entry.id.len(), false, entry.id.to_string()));
        }
    }
    candidates.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)));

    let (_, is_override, key) = candidates.into_iter().next()?;
    if is_override {
        overrides.models.get(&key).map(|entry| resolve_override(&key, entry))
    } else {
        PRICE_TABLE.iter().find(|e| e.id == key).map(resolve_builtin)
    }
}

/// The result of one estimate: `amount` is `None` when the model has no price
/// row, never a guessed number. `model` is the canonical priced id (matching
/// the matched row's key) when a row was found, kept for callers that want to
/// show which row was used.
#[allow(dead_code)]
pub struct EstimatedCost {
    pub amount: Option<f64>,
    pub model: String,
}

/// Estimates the dollar cost of one token batch at official list prices (or
/// a user override), applying Claude's TTL-split cache-write pricing when
/// the tokens are known to come from a 5-minute vs. 1-hour cache write.
/// `input_tokens` must already be the uncached portion for providers (codex)
/// that bill cached tokens separately; claude's counters are already
/// disjoint, so its reader passes them as they stand.
pub fn estimate_cost(
    model: &str,
    input_tokens: u64,
    output_tokens: u64,
    cache_read_tokens: u64,
    cache_write_5m_tokens: u64,
    cache_write_1h_tokens: u64,
    overrides: &PriceOverrides,
) -> EstimatedCost {
    let model = model.trim().to_string();
    let Some(entry) = find_price_entry(&model, overrides) else {
        return EstimatedCost { amount: None, model };
    };

    let cost = (input_tokens as f64 * entry.input)
        + (output_tokens as f64 * entry.output)
        + (cache_read_tokens as f64 * entry.cache_read)
        + (cache_write_5m_tokens as f64 * entry.cache_write_5m)
        + (cache_write_1h_tokens as f64 * entry.cache_write_1h);
    let cost = (cost / 1_000_000.0 * 10_000.0).round() / 10_000.0;

    EstimatedCost { amount: Some(cost), model: entry.id }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn no_overrides() -> PriceOverrides {
        PriceOverrides::default()
    }

    #[test]
    fn exact_match_wins() {
        let entry = find_price_entry("claude-opus-5-5", &no_overrides()).expect("row must exist");
        assert_eq!(entry.id, "claude-opus-5-5");
    }

    #[test]
    fn longest_prefix_wins_over_a_shorter_one() {
        let entry =
            find_price_entry("claude-opus-5-5-20260926", &no_overrides()).expect("prefix must match");
        assert_eq!(entry.id, "claude-opus-5-5");
    }

    /// Every verified claude model in the task's table must have a row and
    /// price it correctly, not just the pre-existing claude-opus-5-5 one.
    #[test]
    fn every_verified_claude_model_is_priced() {
        let cases: &[(&str, f64, f64)] = &[
            ("claude-fable-5-1", 10.0, 50.0),
            ("claude-mythos-5-1", 10.0, 50.0),
            ("claude-fable-5", 10.0, 50.0),
            ("claude-opus-5-5", 4.0, 20.0),
            ("claude-opus-5", 5.0, 25.0),
            ("claude-opus-4-8", 5.0, 25.0),
            ("claude-opus-4-7", 5.0, 25.0),
            ("claude-opus-4-6", 5.0, 25.0),
            ("claude-sonnet-5", 2.0, 10.0),
            ("claude-sonnet-4-6", 3.0, 15.0),
            ("claude-haiku-4-5", 1.0, 5.0),
        ];
        for (id, input, output) in cases {
            let entry = find_price_entry(id, &no_overrides())
                .unwrap_or_else(|| panic!("{id} must have a price row"));
            assert_eq!(entry.input, *input, "{id} input price");
            assert_eq!(entry.output, *output, "{id} output price");
        }
    }

    /// A real prefix seen in this machine's claude store never loses its
    /// price row just because a future release appends a date suffix.
    #[test]
    fn claude_model_with_a_date_suffix_still_matches_by_prefix() {
        let entry = find_price_entry("claude-sonnet-5-20260928", &no_overrides())
            .expect("date-suffixed id must still match by prefix");
        assert_eq!(entry.id, "claude-sonnet-5");
    }

    /// Every codex model id actually seen in this machine's `.codex/sessions`
    /// store must be priced from the official OpenAI page.
    #[test]
    fn every_verified_openai_model_is_priced() {
        let ids = [
            "gpt-5.6-terra",
            "gpt-5.6-sol",
            "gpt-5.5",
            "gpt-6-astra",
            "gpt-5.6-luna",
            "gpt-5.4-mini",
            "gpt-5.4",
            "gpt-5.3-codex",
        ];
        for id in ids {
            assert!(find_price_entry(id, &no_overrides()).is_some(), "{id} must have a price row");
        }
    }

    #[test]
    fn cache_write_5m_and_1h_are_priced_at_different_rates() {
        // claude-opus-5: cache_write_5m = 6.25, cache_write_1h = 10.0 per 1M.
        let five_m = estimate_cost("claude-opus-5", 0, 0, 0, 1_000_000, 0, &no_overrides());
        let one_h = estimate_cost("claude-opus-5", 0, 0, 0, 0, 1_000_000, &no_overrides());
        assert_eq!(five_m.amount, Some(6.25));
        assert_eq!(one_h.amount, Some(10.0));
        assert_ne!(five_m.amount, one_h.amount, "a 1h cache write must not be priced as a 5m one");
    }

    #[test]
    fn unpriced_model_returns_none() {
        assert!(find_price_entry("some-unknown-model-xyz", &no_overrides()).is_none());
        let est = estimate_cost("some-unknown-model-xyz", 1000, 1000, 0, 0, 0, &no_overrides());
        assert!(est.amount.is_none(), "an unpriced model must never invent a cost");
    }

    #[test]
    fn empty_model_returns_none() {
        assert!(find_price_entry("", &no_overrides()).is_none());
        assert!(find_price_entry("   ", &no_overrides()).is_none());
    }

    #[test]
    fn estimate_matches_the_priced_formula() {
        let est = estimate_cost(
            "claude-opus-5-5",
            1_000_000,
            1_000_000,
            1_000_000,
            1_000_000,
            0,
            &no_overrides(),
        );
        // (1*4.0)+(1*20.0)+(1*0.20)+(1*5.0) = 29.20
        assert_eq!(est.amount, Some(29.20));
        assert_eq!(est.model, "claude-opus-5-5");
    }

    #[test]
    fn override_beats_a_built_in_row_with_the_same_id() {
        let mut models = HashMap::new();
        models.insert(
            "claude-opus-5-5".to_string(),
            PriceOverrideEntry { input: 1.0, output: 1.0, cache_read: None, cache_write_5m: None, cache_write_1h: None },
        );
        let overrides = PriceOverrides { models };

        let entry = find_price_entry("claude-opus-5-5", &overrides).expect("row must exist");
        assert_eq!(entry.input, 1.0, "the override's price must win over the built-in row");
        assert_eq!(entry.output, 1.0);
    }

    #[test]
    fn override_can_add_an_unknown_model() {
        let mut models = HashMap::new();
        models.insert(
            "my-local-model".to_string(),
            PriceOverrideEntry { input: 0.5, output: 1.5, cache_read: None, cache_write_5m: None, cache_write_1h: None },
        );
        let overrides = PriceOverrides { models };

        let est = estimate_cost("my-local-model", 1_000_000, 1_000_000, 0, 0, 0, &overrides);
        assert_eq!(est.amount, Some(2.0), "a model with no built-in row must still price once overridden");
    }

    #[test]
    fn override_missing_cache_rates_fall_back_to_the_default_multipliers() {
        let mut models = HashMap::new();
        models.insert(
            "my-model".to_string(),
            PriceOverrideEntry { input: 10.0, output: 10.0, cache_read: None, cache_write_5m: None, cache_write_1h: None },
        );
        let overrides = PriceOverrides { models };

        let entry = find_price_entry("my-model", &overrides).expect("row must exist");
        assert_eq!(entry.cache_read, 1.0, "cache read defaults to 0.1x input");
        assert_eq!(entry.cache_write_5m, 12.5, "cache write 5m defaults to 1.25x input");
        assert_eq!(entry.cache_write_1h, 20.0, "cache write 1h defaults to 2x input");
    }

    #[test]
    fn missing_pricing_file_is_built_ins_only_with_no_warning() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("pricing.json");
        let (overrides, warning) = load_overrides(&path);
        assert!(overrides.models.is_empty());
        assert!(warning.is_none());
    }

    #[test]
    fn valid_pricing_file_loads_its_overrides() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("pricing.json");
        fs::write(
            &path,
            r#"{"models":{"my-model":{"input":1.0,"output":2.0}}}"#,
        )
        .unwrap();

        let (overrides, warning) = load_overrides(&path);
        assert!(warning.is_none());
        assert!(overrides.models.contains_key("my-model"));
    }

    #[test]
    fn malformed_pricing_file_falls_back_to_built_ins_with_a_warning() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("pricing.json");
        fs::write(&path, "{ not valid json").unwrap();

        let (overrides, warning) = load_overrides(&path);
        assert!(overrides.models.is_empty(), "a malformed file must never leave partial overrides");
        assert!(warning.is_some(), "a malformed file must produce a visible warning");

        // The estimator must still work off the built-in table alone.
        let est = estimate_cost("claude-opus-5-5", 1_000_000, 0, 0, 0, 0, &overrides);
        assert_eq!(est.amount, Some(4.0));
    }
}
