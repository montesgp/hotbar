//! Official list prices for the cloud models the agent stores report, used to
//! estimate money for claude and codex because neither jsonl store writes a
//! cost field. Ported from `legacy/windows-widget/lib/Get-AgentPricing.ps1`.
//!
//! User decision carried over from the legacy widget: claude/codex money is
//! ESTIMATED from official prices times the real tokens read from the store.
//! opencode keeps its own real `session.cost` and never goes through this
//! table.
//!
//! A model with no row in the table produces NO money at all - `None` - never
//! a guess from a similar model. Inventing a rate would be the same lie as
//! printing a zero the readers refuse to print.
//!
//! Matching is exact id first, then the longest prefix, so a future provider
//! suffix (`claude-opus-5-5-20260926`) costs nothing to support.

/// One price row. Amounts are $ per 1M tokens, same convention as the legacy
/// table. `source` and `as_of` are not consumed by the estimator itself; they
/// are the auditor trail the legacy widget carries on every row and stay here
/// so a future panel can surface "why this number" instead of a bare figure.
#[allow(dead_code)]
pub struct PriceEntry {
    pub id: &'static str,
    pub input: f64,
    pub output: f64,
    pub cache_read: f64,
    pub cache_write: f64,
    pub source: &'static str,
    pub as_of: &'static str,
}

/// Rows are $ per 1M tokens. Kept in sync with
/// `legacy/windows-widget/lib/Get-AgentPricing.ps1`; edit both when prices
/// change.
const PRICE_TABLE: &[PriceEntry] = &[
    PriceEntry {
        id: "claude-opus-5-5",
        input: 4.0,
        output: 20.0,
        cache_read: 0.20,
        cache_write: 5.0,
        source: "https://platform.claude.com/docs/en/models/opus-5-5/overview",
        as_of: "2026-09-26",
    },
    PriceEntry {
        id: "gpt-5.6-luna",
        input: 0.20,
        output: 1.20,
        cache_read: 0.02,
        cache_write: 0.25,
        source: "https://developers.openai.com/api/docs/models/gpt-5.6-luna",
        as_of: "2026-09-26",
    },
];

/// Finds the price row for a model id, exact match first (case-insensitive),
/// then the longest matching prefix so a shorter key can never shadow a
/// longer one.
pub fn find_price_entry(model: &str) -> Option<&'static PriceEntry> {
    let model = model.trim();
    if model.is_empty() {
        return None;
    }

    if let Some(entry) = PRICE_TABLE
        .iter()
        .find(|e| e.id.eq_ignore_ascii_case(model))
    {
        return Some(entry);
    }

    let mut candidates: Vec<&PriceEntry> = PRICE_TABLE.iter().collect();
    candidates.sort_by_key(|e| std::cmp::Reverse(e.id.len()));
    candidates
        .into_iter()
        .find(|e| model.len() >= e.id.len() && model[..e.id.len()].eq_ignore_ascii_case(e.id))
}

/// The result of one estimate: `amount` is `None` when the model has no price
/// row, never a guessed number. `model` is the canonical priced id (matching
/// `PriceEntry::id`) when a row was found, kept for callers that want to show
/// which row was used, mirroring the legacy widget's separate `MonthModel`.
#[allow(dead_code)]
pub struct EstimatedCost {
    pub amount: Option<f64>,
    pub model: String,
}

/// Estimates the dollar cost of one token batch at official list prices.
/// `input_tokens` must already be the uncached portion for providers (codex)
/// that bill cached tokens separately; claude's counters are already
/// disjoint, so its reader passes them as they stand.
pub fn estimate_cost(
    model: &str,
    input_tokens: u64,
    output_tokens: u64,
    cache_read_tokens: u64,
    cache_write_tokens: u64,
) -> EstimatedCost {
    let model = model.trim().to_string();
    let Some(entry) = find_price_entry(&model) else {
        return EstimatedCost { amount: None, model };
    };

    let cost = (input_tokens as f64 * entry.input)
        + (output_tokens as f64 * entry.output)
        + (cache_read_tokens as f64 * entry.cache_read)
        + (cache_write_tokens as f64 * entry.cache_write);
    let cost = (cost / 1_000_000.0 * 10_000.0).round() / 10_000.0;

    EstimatedCost {
        amount: Some(cost),
        model: entry.id.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_match_wins() {
        let entry = find_price_entry("claude-opus-5-5").expect("row must exist");
        assert_eq!(entry.id, "claude-opus-5-5");
    }

    #[test]
    fn longest_prefix_wins_over_a_shorter_one() {
        let entry = find_price_entry("claude-opus-5-5-20260926").expect("prefix must match");
        assert_eq!(entry.id, "claude-opus-5-5");
    }

    #[test]
    fn unpriced_model_returns_none() {
        assert!(find_price_entry("some-unknown-model-xyz").is_none());
        let est = estimate_cost("some-unknown-model-xyz", 1000, 1000, 0, 0);
        assert!(est.amount.is_none(), "an unpriced model must never invent a cost");
    }

    #[test]
    fn empty_model_returns_none() {
        assert!(find_price_entry("").is_none());
        assert!(find_price_entry("   ").is_none());
    }

    #[test]
    fn estimate_matches_the_priced_formula() {
        let est = estimate_cost("claude-opus-5-5", 1_000_000, 1_000_000, 1_000_000, 1_000_000);
        // (1*4.0)+(1*20.0)+(1*0.20)+(1*5.0) = 29.20
        assert_eq!(est.amount, Some(29.20));
        assert_eq!(est.model, "claude-opus-5-5");
    }
}
