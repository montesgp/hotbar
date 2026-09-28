<#
.SYNOPSIS
  Official list prices for the cloud models the agent stores report, used to
  estimate money for claude and codex because neither jsonl store writes a cost
  field on this machine.

.DESCRIPTION
  User decision (2026-09-26): claude/codex money is ESTIMATED from official
  prices times the real tokens read from the store, and clearly marked as an
  estimate in the panel. opencode keeps its real session.cost and never goes
  through this table.

  A model with no row in the table produces NO money at all - "sin datos" - not
  a guess from a similar model. Inventing a rate would be the same lie as the
  zero the readers refuse to print.

  The table is meant to be edited when prices change: add or update a row, keep
  the Source and AsOf for the auditor. Matching is exact id first, then the
  longest prefix, so future provider suffixes (claude-opus-5-5-2026XXXX) cost
  nothing to support.

  Cache-write assumption, stated honestly: the claude store reports
  cache_creation_input_tokens without saying whether the write was 5m or 1h, so
  the 5m price is used and the 1h price is kept on the row for the record.
  Codex reports cache_write_input_tokens; OpenAI bills cache writes at 1.25x
  the input price, which the row carries directly.

  Dot-source it, do not run it:

    . (Join-Path $PSScriptRoot "Get-AgentPricing.ps1")
#>

# Rows are $ per 1M tokens, except CacheWrite which follows the same convention.
# Match is matched against the model id; Source and AsOf document where the
# number came from and when it was verified.
$script:AgentPriceEntries = @(
  @{
    Match    = "claude-opus-5-5"
    Input    = 4.0
    Output   = 20.0
    CacheRead = 0.20
    CacheWrite = 5.0
    CacheWrite1h = 8.0
    CacheWriteNote = "5m write used; the store cannot distinguish 5m from 1h"
    Source   = "https://platform.claude.com/docs/en/models/opus-5-5/overview"
    AsOf     = "2026-09-26"
  },
  @{
    Match    = "gpt-5.6-luna"
    Input    = 0.20
    Output   = 1.20
    CacheRead = 0.02
    CacheWrite = 0.25
    CacheWrite1h = $null
    CacheWriteNote = "cache writes billed at 1.25x input price"
    Source   = "https://developers.openai.com/api/docs/models/gpt-5.6-luna"
    AsOf     = "2026-09-26"
  }
)

<#
.SYNOPSIS
  Finds the price row for a model id, or $null.

.DESCRIPTION
  Exact id match first (case-insensitive), then the longest matching prefix, so
  "claude-opus-5-5-20260926" resolves to the claude-opus-5-5 row while a
  shorter key can never shadow a longer one.
#>
function Find-AgentPriceEntry {
  [CmdletBinding()]
  param([string]$Model)

  $model = ([string]$Model).Trim()
  if (-not $model) { return $null }

  # Exact match wins outright.
  foreach ($entry in $script:AgentPriceEntries) {
    if ([string]::Equals($model, [string]$entry.Match, [System.StringComparison]::OrdinalIgnoreCase)) {
      return $entry
    }
  }

  # Prefix match, longest key first: a model id with a date suffix resolves to
  # the base id, and a longer base id beats a shorter one that happens to be a
  # prefix of it.
  $candidates = @($script:AgentPriceEntries | Sort-Object { ([string]$_.Match).Length } -Descending)
  foreach ($entry in $candidates) {
    if ($model.StartsWith([string]$entry.Match, [System.StringComparison]::OrdinalIgnoreCase)) {
      return $entry
    }
  }

  return $null
}

<#
.SYNOPSIS
  Estimates the dollar cost of one token batch at official list prices.

.DESCRIPTION
  Cost = (input x in) + (output x out) + (cacheRead x read)
         + (cacheWrite x write), all divided by 1M.

  InputTokens for codex must be the UNcached part: OpenAI bills the cached
  portion at the cached price and the rest at the input price, and the caller
  (the codex reader) passes uncached = input - cached there. claude's counters
  are already separate (input_tokens does not include the cache counters), so
  the claude reader passes them as they stand.

  Unknown model: Amount = $null, so the caller keeps reporting "costo: sin
  datos" and never prints a number nobody priced.
#>
function Get-AgentEstimatedCost {
  [CmdletBinding()]
  param(
    [string]$Model,
    [double]$InputTokens = 0,
    [double]$OutputTokens = 0,
    [double]$CacheReadTokens = 0,
    [double]$CacheWriteTokens = 0
  )

  $entry = Find-AgentPriceEntry -Model $Model
  if ($null -eq $entry) {
    $model = ([string]$Model).Trim()
    return [pscustomobject]@{
      Amount    = $null
      Estimated = $false
      Model     = $model
      Source    = ""
      AsOf      = ""
      Detail    = if ($model) { "model without a price row: $model" } else { "no model priced" }
    }
  }

  $cost = (([double]$InputTokens * [double]$entry.Input) +
           ([double]$OutputTokens * [double]$entry.Output) +
           ([double]$CacheReadTokens * [double]$entry.CacheRead) +
           ([double]$CacheWriteTokens * [double]$entry.CacheWrite)) / 1000000.0
  # Four decimals keeps cents exact for display while hiding float noise; the
  # panel rounds to 0.00 anyway.
  $cost = [Math]::Round($cost, 4)

  return [pscustomobject]@{
    Amount    = $cost
    Estimated = $true
    Model     = [string]$entry.Match
    Source    = [string]$entry.Source
    AsOf      = [string]$entry.AsOf
    Detail    = ("estimated with official {0} pricing ({1})" -f $entry.Match, $entry.Source)
  }
}