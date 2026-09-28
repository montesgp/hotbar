<#
.SYNOPSIS
  Live session usage for claude, codex and opencode: bounded, honest, never invented.

.DESCRIPTION
  Three readers, one aggregator. Each one answers "what has the current session
  cost so far" from the store that tool already writes, and each one returns a
  typed result instead of throwing, so the caller can always draw something.

  Stores, verified on this machine on 2026-09-26:

    claude    <USERPROFILE>\.claude\projects\**\*.jsonl, newest file by mtime.
              The live session is that file. Each entry carries
              message.usage{input_tokens, cache_creation_input_tokens,
              cache_read_input_tokens, output_tokens,
              output_tokens_details.thinking_tokens}. Those counters are PER
              CALL, so they are summed.

    codex     <USERPROFILE>\.codex\sessions\**\*.jsonl, newest file by mtime.
              token_usage_record entries carry payload.turn_token_usage
              {input_tokens, cached_input_tokens, cache_write_input_tokens,
              output_tokens, reasoning_output_tokens}. Those counters are
              CUMULATIVE for the session, NOT per turn: measured on the newest
              file, input_tokens grows monotonically across the 21 records
              (30 796 -> 1 356 488) and the last record already equals
              payload.thread_token_usage. Summing them inflated the session by
              an order of magnitude, so the LAST record wins. Same field
              values, honest total.

    opencode  <USERPROFILE>\.local\share\opencode\opencode.db, table `session`,
              the row with MAX(time_updated). That table is already
              consolidated per session, so no arithmetic is needed:
              cost, tokens_input, tokens_output, tokens_reasoning,
              tokens_cache_read, tokens_cache_write, model (JSON text).

  MONEY IS NOT INVENTED... unless the user says a price-card total may stand in
  for a bill, and even then it is flagged. Neither jsonl store carries a cost
  key on this machine (checked the newest claude file deep-walked, and the four
  newest codex files by text search: zero matches for a cost field, only the
  word inside conversation content). So the readers look for a cost field
  through a fixed candidate list; with none, they ask Get-AgentEstimatedCost
  (official prices in Get-AgentPricing.ps1, user decision 2026-09-26) and only
  a model that HAS a price row produces money, marked Estimated = $true so the
  panel can never pass it off as billed. A zero is only ever printed when the
  store itself says 0.0, which is the real value for opencode's local models.

  HISTORY. The same stores also feed the per-agent panel (HB14): month-to-date
  counters plus a per-project split, ranked by spend, over every project cwd
  the agent's own store touched this month. claude sums
  every *.jsonl under .claude\projects (those files ARE billed, subagents and
  all), codex reads the LAST counter record per session file (cumulative, both
  token_usage_record and token_count shapes), opencode runs one SELECT per
  month plus one GROUP BY project.worktree. Bytes are bounded across the whole
  scan, newest files first, and truncation is reported as Approximate = $true.
  opencode money is real (session.cost); claude and codex money is estimated
  with the same Get-AgentEstimatedCost rules as the live panel.

  THREE THINGS THAT ARE EASY TO GET WRONG, all of them verified here:

    1. Duplicate entries. The claude file holds 306 entries carrying usage but
       only 156 distinct requestIds: every assistant message is stored more
       once (original plus snapshot copy). Summing raw doubled the session
       (53 943 031 cache_read instead of 27 189 253), so the claude reader
       de-duplicates by requestId, falling back to message.id and then uuid.
    2. Get-Content -Tail is unusable on this path: it took 6390 ms for a 2.5 MB
       file in PowerShell 5.1, against 16 ms for a full streaming read of the
       same file. The bounded read is [System.IO.File] streaming with a seek to
       the tail, never Get-Content -Tail.
    3. Enumerating with Get-ChildItem -Recurse and building a FileInfo per file
       costs 335 ms for 551 files; [System.IO.Directory]::EnumerateFiles with
       [System.IO.File]::GetLastWriteTimeUtc costs 76 ms for the same 551. The
       second one is what this file uses, and it is the reason the readers stay
       well inside their budget.

  BOUNDS. No reader may take more than ~1.5 s, so:

    * at most UsageMaxBytes (8 MB) are read from a session file, and when the
      file is larger the read starts at its tail;
    * at most UsageMaxUsageLines (10000) usage-bearing lines are retained, and
      the oldest are evicted first, so what survives is the recent part;
    * the SQLite read runs with a busy timeout, because a database locked by the
      live editor must fail fast and say so rather than freeze the bar.

  Dropping lines is never silent: the result carries Approximate = $true and the
  reason in Detail, and the panel marks those numbers with a trailing "~".

  Dot-source it, do not run it:

    . (Join-Path $PSScriptRoot "Get-AgentUsage.ps1")
#>

# Read budget for one session file. Both are deliberately generous: a normal
# claude session is ~2.5 MB / ~1300 lines, so nothing is ever dropped in
# practice and the numbers are exact. They exist so a runaway or
# machine-generated file cannot stall the bar.
$script:UsageMaxBytes = 8MB
$script:UsageMaxUsageLines = 10000

# Cost field candidates, in preference order, probed on the entry root and on the
# usage object of every parsed line. None of them exists on this machine today;
# the list is here so a store that starts writing one is picked up without a
# code change, and so the exact spelling that was found can be reported.
$script:CostFieldCandidates = @("cost", "costUSD", "cost_usd", "total_cost_usd", "totalCost")

# Provider ids that mean "ran on this machine, no provider billed for it". Cost
# 0.0 with one of these is a real zero, not a missing value.
$script:UsageLocalProviders = @("opencode")

# Pulled out of the per-line loop: a [regex] built once and reused beats letting
# -match compile a new one for every candidate line.
#
# All three, because the duplicate rejection has to happen BEFORE the JSON parse
# to be worth anything, and the parse-then-discard fallback only catches the
# requestId case. Half the claude usage lines in a window carry no requestId at
# all: they are identified by message.id or uuid, so without these two they were
# parsed, hashed and thrown away - 74 wasted ConvertFrom-Json calls in a 1 MB
# window, about 75 ms. The msg_ prefix anchors the id pattern so it cannot match
# some other "id" field that happens to be in the line.
$script:UsageRequestIdPattern = New-Object System.Text.RegularExpressions.Regex ('"requestId"\s*:\s*"([^"]*)"')
$script:UsageMessageIdPattern = New-Object System.Text.RegularExpressions.Regex ('"id"\s*:\s*"(msg_[^"]*)"')
$script:UsageUuidPattern = New-Object System.Text.RegularExpressions.Regex ('"uuid"\s*:\s*"([^"]*)"')

<#
.SYNOPSIS
  The home directory the three stores live under.

.NOTES
  USERPROFILE first, HOME second, so the reader also works if the widget is ever
  started from a shell that only sets HOME.
#>
function Get-AgentUsageHome {
  [CmdletBinding()]
  param()

  $home_dir = $env:USERPROFILE
  if (-not $home_dir) { $home_dir = $env:HOME }
  if (-not $home_dir) { return "" }
  return $home_dir
}

<#
.SYNOPSIS
  The three store locations, resolved from the home directory.

.PARAMETER HomeDir
  Overrides the home directory. The name is deliberately NOT $Home: $HOME is a
  read-only automatic variable in PowerShell, so a parameter called $Home throws
  "Cannot overwrite variable Home because it is read-only" the moment it is
  bound. Same trap as the $Port/$GatewayPort one in Get-OmniRouteStatus.ps1.
#>
function Resolve-AgentUsagePaths {
  [CmdletBinding()]
  param([string]$HomeDir)

  if (-not $HomeDir) { $HomeDir = Get-AgentUsageHome }
  if (-not $HomeDir) { return $null }

  return [pscustomobject]@{
    Home          = $HomeDir
    ClaudeDir     = [System.IO.Path]::Combine($HomeDir, ".claude", "projects")
    CodexDir      = [System.IO.Path]::Combine($HomeDir, ".codex", "sessions")
    OpenCodeDb    = [System.IO.Path]::Combine($HomeDir, ".local", "share", "opencode", "opencode.db")
  }
}

<#
.SYNOPSIS
  The newest *.jsonl under a directory tree, or $null.

.DESCRIPTION
  One pass over [System.IO.Directory]::EnumerateFiles plus a
  GetLastWriteTimeUtc per file. Measured 76 ms over the 551 files under
  .claude\projects, which is the whole enumeration cost of the claude reader.
#>
function Get-NewestAgentSessionFile {
  [CmdletBinding()]
  param([string]$Root)

  if (-not $Root) { return $null }
  if (-not [System.IO.Directory]::Exists($Root)) { return $null }

  $newest = $null
  $newestTime = [DateTime]::MinValue
  try {
    foreach ($path in [System.IO.Directory]::EnumerateFiles($Root, "*.jsonl", [System.IO.SearchOption]::AllDirectories)) {
      $stamp = [System.IO.File]::GetLastWriteTimeUtc($path)
      if ($stamp -gt $newestTime) { $newestTime = $stamp; $newest = $path }
    }
  } catch {
    if (-not $newest) { return $null }
  }

  if (-not $newest) { return $null }
  return [pscustomobject]@{ Path = $newest; LastWriteTimeUtc = $newestTime }
}

# The result shape every reader fills in. Cost starts as $null, not 0: "no cost
# field in this store" and "this session cost nothing" are different facts and
# only the store can tell them apart.
function New-AgentUsageResult {
  [CmdletBinding()]
  param(
    [string]$Agent,
    [string]$Format
  )

  return [pscustomobject]@{
    Agent           = $Agent
    Format          = $Format
    Ok              = $false
    Approximate     = $false
    InputTokens     = 0
    OutputTokens    = 0
    CacheTokens     = 0
    ReasoningTokens = 0
    Cost            = $null
    CostField       = ""
    Estimated       = $false
    Model           = ""
    Local           = $false
    Detail          = ""
    Error           = ""
  }
}

<#
.SYNOPSIS
  Streams a session file within the read budget and hands back its usage lines.

.PARAMETER Path
  Session file to read.

.PARAMETER PreFilter
  Literal substrings that a line must contain. String.IndexOf, not a regex: this
  runs on every line of a multi-megabyte file, and matching a plain literal with
  Ordinal comparison is the cheapest test there is.

.DESCRIPTION
  Returns the LAST UsageMaxBytes of the file (a seek, not Get-Content -Tail,
  which measured 6.4 s on a 2.5 MB file) and keeps at most
  UsageMaxUsageLines usage-bearing lines, evicting the oldest first, so the
  caller always parses the recent part of the session.

  The file is opened with FileShare.ReadWrite: claude is appending to it right
  now, and a read that fails because the writer holds the handle would be a
  wrong answer instead of a missing one.

  Returns $null when the file cannot be opened at all, and sets $Dropped when
  lines were left behind - that is what Approximate is made of.
#>
function Read-AgentSessionLines {
  [CmdletBinding()]
  param(
    [Parameter(Mandatory = $true)][string]$Path,
    [Parameter(Mandatory = $true)][string[]]$PreFilter,
    [long]$MaxBytes = 0
  )

  $budget = if ($MaxBytes -gt 0) { $MaxBytes } else { [long]$script:UsageMaxBytes }
  $state = [pscustomobject]@{ Lines = @(); Dropped = 0; TruncatedTail = $false; ScannedBytes = 0 }

  $stream = $null
  $reader = $null
  try {
    $stream = [System.IO.File]::Open($Path, [System.IO.FileMode]::Open, [System.IO.FileAccess]::Read, [System.IO.FileShare]::ReadWrite)
    $length = $stream.Length
    $offset = 0
    if ($length -gt $budget) {
      $offset = $length - $budget
      $state.TruncatedTail = $true
    }
    if ($offset -gt 0) { [void]$stream.Seek($offset, [System.IO.SeekOrigin]::Begin) }
    $state.ScannedBytes = $length - $offset

    # 64 KB of buffer, not StreamReader's 1 KB default: a 1 MB window refilled
    # that buffer 1000 times, which measured 65 ms against 35 ms here.
    $reader = [System.IO.StreamReader]::new($stream, [System.Text.Encoding]::UTF8, $true, 65536)
    if ($offset -gt 0) { $null = $reader.ReadLine() }  # drop the partial first line

    $queue = New-Object System.Collections.Queue
    $keptBytes = 0
    while ($null -ne ($line = $reader.ReadLine())) {
      # Cheap text pre-filter before any JSON parsing: on the claude store only
      # about a quarter of the lines carry usage, and ConvertFrom-Json is the
      # expensive half of the whole read.
      $hit = $false
      foreach ($needle in $PreFilter) {
        if ($line.IndexOf($needle, [System.StringComparison]::Ordinal) -ge 0) { $hit = $true; break }
      }
      if (-not $hit) { continue }

      $queue.Enqueue($line)
      $keptBytes += $line.Length
      if ($queue.Count -gt $script:UsageMaxUsageLines) {
        $old = $queue.Dequeue()
        $keptBytes -= $old.Length
        $state.Dropped++
      }
      while ($keptBytes -gt $budget -and $queue.Count -gt 0) {
        $old = $queue.Dequeue()
        $keptBytes -= $old.Length
        $state.Dropped++
      }
    }
    $state.Lines = $queue.ToArray()
  } finally {
    if ($null -ne $reader) { $reader.Dispose() }
    elseif ($null -ne $stream) { $stream.Dispose() }
  }

  return $state
}

# Reads a candidate cost field off one parsed entry. Returns $null when the
# entry has none of them, so a store without a cost field never produces a zero.
function Get-AgentEntryCost {
  [CmdletBinding()]
  param($Entry, $Usage)

  foreach ($name in $script:CostFieldCandidates) {
    foreach ($holder in @($Entry, $Usage)) {
      if ($null -eq $holder) { continue }
      $property = $holder.PSObject.Properties[$name]
      if ($null -eq $property) { continue }
      if ($null -eq $property.Value) { continue }
      $value = 0.0
      if (-not [double]::TryParse(([string]$property.Value), [System.Globalization.NumberStyles]::Float, [System.Globalization.CultureInfo]::InvariantCulture, [ref]$value)) { continue }
      return [pscustomobject]@{ Field = $name; Value = $value }
    }
  }
  return $null
}

# Number extraction that never invents: a missing or unparsable counter stays
# 0 for that counter, and the caller sees it as a zero because the field was
# genuinely absent from every entry.
function Get-AgentInt64 {
  param($Node, [string]$Name)

  if ($null -eq $Node) { return 0 }
  $property = $Node.PSObject.Properties[$Name]
  if ($null -eq $property -or $null -eq $property.Value) { return 0 }
  $value = 0
  if ([int64]::TryParse(([string]$property.Value), [ref]$value)) { return $value }
  return 0
}

<#
.SYNOPSIS
  Session usage for the newest claude session file.

.DESCRIPTION
  Sums message.usage over the file, but only once per request: the store keeps
  more than one copy of many assistant messages, and summing the copies
  doubled every number measured here.

  Model comes from the last entry that names one; reasoning tokens from
  usage.output_tokens_details.thinking_tokens; cache is cache_read plus
  cache_creation, which is what a claude turn actually pays for.
#>
function Get-ClaudeAgentUsage {
  [CmdletBinding()]
  param(
    [string]$ClaudeDir,
    [int]$BusyTimeoutMs = 1200,
    [long]$MaxBytes = 0
  )

  $budget = if ($MaxBytes -gt 0) { $MaxBytes } else { [long]$script:UsageMaxBytes }
  $result = New-AgentUsageResult -Agent "claude" -Format "claude-jsonl"

  if (-not $ClaudeDir) { $ClaudeDir = (Resolve-AgentUsagePaths).ClaudeDir }
  if (-not $ClaudeDir) { $result.Error = "no home directory"; return $result }
  if (-not [System.IO.Directory]::Exists($ClaudeDir)) { $result.Error = "missing directory: $ClaudeDir"; return $result }

  $newest = Get-NewestAgentSessionFile -Root $ClaudeDir
  if ($null -eq $newest) { $result.Error = "no session file under $ClaudeDir"; return $result }

  $name = [System.IO.Path]::GetFileName($newest.Path)
  $result.Detail = $name

  $state = $null
  try {
    $state = Read-AgentSessionLines -Path $newest.Path -PreFilter @('"usage"') -MaxBytes $MaxBytes
  } catch {
    $result.Error = "cannot read " + $name + ": " + (Get-AgentFirstLine $_.Exception.Message)
    return $result
  }

  $seen = New-Object System.Collections.Generic.HashSet[string]
  $input = 0
  $output = 0
  $cacheRead = 0
  $cacheWrite = 0
  $reasoning = 0
  $entries = 0
  $costSum = 0.0
  $costField = ""

  foreach ($line in $state.Lines) {
    # Cheap duplicate rejection before the JSON parse. The key is the same one
    # the parse would produce, so a line skipped here is a line the parse would
    # have thrown away: measured 265 ms to parse all 308 usage lines against
    # 128 ms to parse only the 158 that survive, with an identical total.
    $textKey = ""
    $keyMatch = $script:UsageRequestIdPattern.Match($line)
    if ($keyMatch.Success) { $textKey = $keyMatch.Groups[1].Value }
    else {
      $idMatch = $script:UsageMessageIdPattern.Match($line)
      if ($idMatch.Success) { $textKey = $idMatch.Groups[1].Value }
      else {
        $uuidMatch = $script:UsageUuidPattern.Match($line)
        if ($uuidMatch.Success) { $textKey = $uuidMatch.Groups[1].Value }
      }
    }
    if ($textKey -and $seen.Contains($textKey)) { continue }

    try { $entry = $line | ConvertFrom-Json } catch { continue }
    if ($null -eq $entry) { continue }

    $usage = $null
    if ($null -ne $entry.message -and $null -ne $entry.message.usage) { $usage = $entry.message.usage }
    elseif ($null -ne $entry.usage) { $usage = $entry.usage }
    if ($null -eq $usage) { continue }

    # One assistant message, one count. requestId is the API request, message.id
    # the message, uuid the log line; the first one present is enough.
    $key = [string]$entry.requestId
    if (-not $key -and $null -ne $entry.message) { $key = [string]$entry.message.id }
    if (-not $key) { $key = [string]$entry.uuid }
    if ($key) { if (-not $seen.Add($key)) { continue } }

    $entries++
    $input += Get-AgentInt64 $usage "input_tokens"
    $output += Get-AgentInt64 $usage "output_tokens"
    $cacheRead += Get-AgentInt64 $usage "cache_read_input_tokens"
    $cacheWrite += Get-AgentInt64 $usage "cache_creation_input_tokens"
    if ($null -ne $usage.output_tokens_details) {
      $reasoning += Get-AgentInt64 $usage.output_tokens_details "thinking_tokens"
    }
    $cost = Get-AgentEntryCost -Entry $entry -Usage $usage
    if ($null -ne $cost) { $costSum += $cost.Value; $costField = $cost.Field }

    # The store writes placeholder models such as "<synthetic>" on entries that
    # never hit a real provider. Reporting one as the session model would be a
    # confident lie, so a bracketed marker never wins the label.
    if ($null -ne $entry.message -and $entry.message.model) {
      $model = ([string]$entry.message.model).Trim()
      if ($model -and -not $model.StartsWith("<")) { $result.Model = $model }
    }
  }

  if ($entries -eq 0) {
    $result.Error = "no usage entries in " + $name
    return $result
  }

  $result.Ok = $true
  $result.InputTokens = $input
  $result.OutputTokens = $output
  $result.CacheTokens = $cacheRead + $cacheWrite
  $result.ReasoningTokens = $reasoning
  $result.CostField = $costField

  # Money from the store wins when the store records it. Otherwise the official
  # price row for the session model estimates it, flagged Estimated so the panel
  # can never pass a price-card total off as a bill; a model with no price row
  # still means no money at all.
  if ($costField) {
    $result.Cost = $costSum
  } else {
    $est = Get-AgentEstimatedCost -Model $result.Model -InputTokens $input -OutputTokens $output -CacheReadTokens $cacheRead -CacheWriteTokens $cacheWrite
    if ($est.Estimated) {
      $result.Cost = $est.Amount
      $result.Estimated = $true
    }
  }

  $result.Detail = ("{0} - {1} entradas - cache = read+creation" -f $name, $entries)
  if ($result.Estimated) { $result.Detail = $result.Detail + (" - est: {0}" -f $result.Model) }
  if ($state.Dropped -gt 0 -or $state.TruncatedTail) {
    $result.Approximate = $true
    $result.Detail = $result.Detail + (" - aproximada: {0} lineas fuera del presupuesto de {1} MB" -f $state.Dropped, [int]($budget / 1MB))
  }
  return $result
}

<#
.SYNOPSIS
  Session usage for the newest codex session file.

.DESCRIPTION
  LAST record wins, not the sum. payload.turn_token_usage is the running session
  total on every token_usage_record (verified: 21 records, input_tokens
  strictly increasing from 30796 to 1356488, last record identical to
  payload.thread_token_usage), so summing would report a 12.9 M input session
  for a 1.36 M one. The counters are read as they stand.

  Reasoning tokens come from reasoning_output_tokens; cache is
  cached_input_tokens plus cache_write_input_tokens, matching what codex
  actually bills as cached.
#>
function Get-CodexAgentUsage {
  [CmdletBinding()]
  param(
    [string]$CodexDir,
    [int]$BusyTimeoutMs = 1200,
    [long]$MaxBytes = 0
  )

  $budget = if ($MaxBytes -gt 0) { $MaxBytes } else { [long]$script:UsageMaxBytes }
  $result = New-AgentUsageResult -Agent "codex" -Format "codex-jsonl"

  if (-not $CodexDir) { $CodexDir = (Resolve-AgentUsagePaths).CodexDir }
  if (-not $CodexDir) { $result.Error = "no home directory"; return $result }
  if (-not [System.IO.Directory]::Exists($CodexDir)) { $result.Error = "missing directory: $CodexDir"; return $result }

  $newest = Get-NewestAgentSessionFile -Root $CodexDir
  if ($null -eq $newest) { $result.Error = "no session file under $CodexDir"; return $result }

  $name = [System.IO.Path]::GetFileName($newest.Path)
  $result.Detail = $name

  $state = $null
  try {
    # Two things are worth a parse here: the token counters, and the model name,
    # which lives on session_meta / turn_context entries that carry no counters
    # at all. Filtering on both keeps the model label on a file where the model
    # is declared once per turn rather than once per token_usage_record.
    $state = Read-AgentSessionLines -Path $newest.Path -PreFilter @("token_usage", '"model"') -MaxBytes $MaxBytes
  } catch {
    $result.Error = "cannot read " + $name + ": " + (Get-AgentFirstLine $_.Exception.Message)
    return $result
  }

  $last = $null
  $records = 0
  $costValue = 0.0
  $costField = ""
  $hasCost = $false

  foreach ($line in $state.Lines) {
    try { $entry = $line | ConvertFrom-Json } catch { continue }
    if ($null -eq $entry -or $null -eq $entry.payload) { continue }
    $payload = $entry.payload

    if ($payload.model) {
      $model = ([string]$payload.model).Trim()
      if ($model -and -not $model.StartsWith("<")) { $result.Model = $model }
    }

    $usage = $null
    if ($null -ne $payload.turn_token_usage) { $usage = $payload.turn_token_usage }
    elseif ($null -ne $payload.usage) { $usage = $payload.usage }
    if ($null -eq $usage) { continue }

    $records++
    $last = $usage

    $cost = Get-AgentEntryCost -Entry $entry -Usage $usage
    if ($null -ne $cost) { $costValue = $cost.Value; $costField = $cost.Field; $hasCost = $true }
  }

  if ($null -eq $last) {
    $result.Error = "no token usage record in " + $name
    return $result
  }

  $result.Ok = $true
  $inputTokens = Get-AgentInt64 $last "input_tokens"
  $cachedInput = Get-AgentInt64 $last "cached_input_tokens"
  $cacheWriteTokens = Get-AgentInt64 $last "cache_write_input_tokens"
  $result.InputTokens = $inputTokens
  $result.OutputTokens = Get-AgentInt64 $last "output_tokens"
  $result.CacheTokens = $cachedInput + $cacheWriteTokens
  $result.ReasoningTokens = Get-AgentInt64 $last "reasoning_output_tokens"
  $result.CostField = $costField

  if ($hasCost) {
    $result.Cost = $costValue
  } else {
    # OpenAI bills the cached portion at the cached price and the rest at the
    # input price, so the estimator gets UNCached input here.
    $uncached = [Math]::Max(0, $inputTokens - $cachedInput)
    $est = Get-AgentEstimatedCost -Model $result.Model -InputTokens $uncached -OutputTokens $result.OutputTokens -CacheReadTokens $cachedInput -CacheWriteTokens $cacheWriteTokens
    if ($est.Estimated) {
      $result.Cost = $est.Amount
      $result.Estimated = $true
    }
  }

  $result.Detail = ("{0} - {1} registros - ultimo acumulado" -f $name, $records)
  if ($result.Estimated) { $result.Detail = $result.Detail + (" - est: {0}" -f $result.Model) }
  if ($state.Dropped -gt 0 -or $state.TruncatedTail) {
    $result.Approximate = $true
    $result.Detail = $result.Detail + (" - aproximada: {0} lineas fuera del presupuesto de {1} MB" -f $state.Dropped, [int]($budget / 1MB))
  }
  return $result
}

# One SELECT, one row: the most recently updated session. Columns come back as
# ASCII unit separated text, so a model JSON with quotes or commas in it can
# never break the column split. There are no double quotes in the statement, so
# it survives PowerShell 5.1 native argument marshalling unchanged; the here
# string is single quoted and flattened to one line for the same reason.
$script:UsageSessionSql = @'
SELECT replace(id, char(31), ' '),
       replace(COALESCE(model, ''), char(31), ' '),
       COALESCE(cost, ''),
       COALESCE(tokens_input, 0),
       COALESCE(tokens_output, 0),
       COALESCE(tokens_reasoning, 0),
       COALESCE(tokens_cache_read, 0),
       COALESCE(tokens_cache_write, 0),
       COALESCE(time_updated, 0)
FROM session ORDER BY time_updated DESC LIMIT 1
'@ -replace "`r?`n", " "

# The model column is JSON text: {"id":"big-pickle","providerID":"opencode"}.
# Parsed with ConvertFrom-Json, and matched against a permissive regex as a
# fallback so a future schema change costs the model label, not the whole read.
function ConvertFrom-AgentModelColumn {
  [CmdletBinding()]
  param([string]$Text)

  $label = [pscustomobject]@{ Id = ""; Provider = "" }

  $text = ([string]$Text).Trim()
  if (-not $text) { return $label }

  try {
    $parsed = $text | ConvertFrom-Json
    if ($null -ne $parsed) {
      if ($parsed.id) { $label.Id = [string]$parsed.id }
      if ($parsed.providerID) { $label.Provider = [string]$parsed.providerID }
    }
  } catch {
    # Not JSON: fall through to the regex below.
  }

  if (-not $label.Id -and $text -match '"id"\s*:\s*"([^"]*)"') { $label.Id = $Matches[1] }
  if (-not $label.Provider -and $text -match '"providerID"\s*:\s*"([^"]*)"') { $label.Provider = $Matches[1] }
  return $label
}

<#
.SYNOPSIS
  Session usage for the live opencode session, out of its own SQLite store.

.DESCRIPTION
  The `session` table is already consolidated per session, so this is one
  indexed read of the row with MAX(time_updated): no arithmetic, no double
  counting. The database is 168 MB and is opened read-only through the widget's
  own Read-SqliteQuery wrapper, with a busy timeout so a database locked by the
  live editor reports a reason instead of freezing the bar.

  cost is taken as stored: 0.0 is a real zero for a model that ran on this
  machine, and it is reported as 0.0. A NULL cost is a missing value and stays
  missing.
#>
function Get-OpenCodeAgentUsage {
  [CmdletBinding()]
  param(
    [string]$DatabasePath,
    [int]$BusyTimeoutMs = 1200
  )

  $result = New-AgentUsageResult -Agent "opencode" -Format "sqlite"

  if (-not $DatabasePath) { $DatabasePath = (Resolve-AgentUsagePaths).OpenCodeDb }
  $result.Detail = [System.IO.Path]::GetFileName($DatabasePath)

  if (-not [System.IO.File]::Exists($DatabasePath)) {
    $result.Error = "database not found: $DatabasePath"
    return $result
  }

  # The reader is dot-sourced here rather than at the top of the widget, exactly
  # like Get-OmniRouteCombos does: a bar that never opens a panel must not pay to
  # parse it. It is dot-sourced on every call rather than cached in a variable,
  # because a function defined by dot-sourcing inside a function dies with that
  # function, and a "cache" that publishes it to a wider scope breaks
  # $PSScriptRoot inside it - both measured, both worse than 60 ms of re-parsing.
  $lib = [System.IO.Path]::Combine($PSScriptRoot, "Read-SqliteQuery.ps1")
  if (-not [System.IO.File]::Exists($lib)) {
    $result.Error = "SQLite reader not found: $lib"
    return $result
  }
  . $lib

  $query = $null
  try {
    $query = Invoke-SqliteQuery -DatabasePath $DatabasePath -Sql $script:UsageSessionSql -BusyTimeoutMs $BusyTimeoutMs
  } catch {
    $result.Error = Get-AgentFirstLine $_.Exception.Message
    return $result
  }

  if (-not $query.Ok) {
    $result.Error = Get-AgentFirstLine $query.Error
    return $result
  }
  if (@($query.Rows).Count -eq 0) {
    $result.Error = "no session row in " + $result.Detail
    return $result
  }

  $row = @($query.Rows)[0]
  $model = ConvertFrom-AgentModelColumn $row[1]

  $result.Ok = $true
  $result.Model = $model.Id
  $result.Local = ($script:UsageLocalProviders -contains $model.Provider)
  $result.InputTokens = [int64]$row[3]
  $result.OutputTokens = [int64]$row[4]
  $result.ReasoningTokens = [int64]$row[5]
  $result.CacheTokens = [int64]$row[6] + [int64]$row[7]

  # An empty cost cell is NULL in the database, which is "not recorded", not
  # free. Only a real number becomes money.
  $costText = ([string]$row[2]).Trim()
  if ($costText) {
    $cost = 0.0
    if ([double]::TryParse($costText, [System.Globalization.NumberStyles]::Float, [System.Globalization.CultureInfo]::InvariantCulture, [ref]$cost)) {
      $result.Cost = $cost
      $result.CostField = "session.cost"
    } else {
      $result.Error = "unreadable cost value: $costText"
    }
  }

  $updated = 0
  $stamp = ""
  if ([int64]::TryParse(([string]$row[8]), [ref]$updated) -and $updated -gt 0) {
    try { $stamp = ([DateTimeOffset]::FromUnixTimeMilliseconds($updated).LocalDateTime.ToString("yyyy-MM-dd HH:mm:ss")) } catch { $stamp = "" }
  }
  $result.Detail = ("{0} - session {1}{2} - via {3}" -f $result.Detail, $row[0], $(if ($stamp) { " @ $stamp" } else { "" }), $query.Provider)
  return $result
}

# First non-blank line of a diagnostic, so a multi-line reason still renders as
# one readable line in the panel.
function Get-AgentFirstLine {
  param([string]$Text)

  if (-not $Text) { return "" }
  foreach ($line in ($Text -split "`r?`n")) {
    $trimmed = $line.Trim()
    if ($trimmed) { return $trimmed }
  }
  return ""
}

<#
.SYNOPSIS
  One sample of all three agents, for the live panel and for the self test.

.DESCRIPTION
  Sequential on purpose. Measured on this machine the three readers together are
  roughly half a second (claude 76 ms to enumerate 551 files plus ~250 ms to
  parse, codex ~90 ms, opencode ~60 ms for a SELECT over a 168 MB database),
  which is a fifth of the refresh period and a fraction of the 1.5 s per-reader
  budget. A runspace would cost more than it saves at that size.

  Every reader is wrapped: a store that is missing, locked or unparsable comes
  back as Ok = $false with the reason in Error, never as zeroes.

.PARAMETER MaxBytes
  Per-reader read budget for this one call. 0 uses UsageMaxBytes (8 MB), which is
  what the live panel always wants. The self test passes a smaller number on
  purpose: a check with a 2 s ceiling that reads a session file of unbounded
  length is a check that starts failing on long sessions, and a self test that
  fails for a reason unrelated to the code is worse than no self test. At 2 MB the
  claude read is ~200 ms, and it exercises the truncation and Approximate paths
  that an 8 MB read on a small file would never reach.
#>
function Get-AgentUsageSnapshot {
  [CmdletBinding()]
  param(
    [string]$HomeDir,
    [int]$BusyTimeoutMs = 1200,
    [long]$MaxBytes = 0
  )

  $paths = Resolve-AgentUsagePaths -HomeDir $HomeDir

  $agents = @()
  $agents += Get-ClaudeAgentUsage -ClaudeDir $(if ($paths) { $paths.ClaudeDir } else { "" }) -BusyTimeoutMs $BusyTimeoutMs -MaxBytes $MaxBytes
  $agents += Get-CodexAgentUsage -CodexDir $(if ($paths) { $paths.CodexDir } else { "" }) -BusyTimeoutMs $BusyTimeoutMs -MaxBytes $MaxBytes
  $agents += Get-OpenCodeAgentUsage -DatabasePath $(if ($paths) { $paths.OpenCodeDb } else { "" }) -BusyTimeoutMs $BusyTimeoutMs

  $takenAt = Get-Date
  foreach ($agent in $agents) { $agent | Add-Member -NotePropertyName TakenAt -NotePropertyValue $takenAt -Force }
  return $agents
}

# ---------------------------------------------------------------------------
# History (HB14): month-to-date counters plus a per-project split, for the
# per-agent panel. The same stores, the same honesty rules as the live readers.
# ---------------------------------------------------------------------------

# Total read budget across ALL history files for one agent scan. The scan is
# newest-file-first and each file only gets the remaining budget, so a month of
# work is exact while a machine-generated runaway still cannot stall the bar.
$script:UsageHistoryMaxBytes = 8MB

# Month labels for the panel. Deliberately ASCII and fixed: the panel text is
# Spanish without accents, and a culture-dependent abbreviated month could
# render "sep" here and "SEP" there.
$script:AgentMonthNames = @("ene", "feb", "mar", "abr", "may", "jun", "jul", "ago", "sep", "oct", "nov", "dic")

<#
.SYNOPSIS
  Local start of the current month, as a [datetime] for comparisons.
#>
function Get-AgentMonthStartLocal {
  [CmdletBinding()]
  param()

  $now = Get-Date
  return $now.Date.AddDays(1 - [int]$now.Day)
}

<#
.SYNOPSIS
  The ASCII-safe Spanish label of the current month ("sep" for September).
#>
function Get-AgentMonthLabel {
  [CmdletBinding()]
  param()

  $now = Get-Date
  return $script:AgentMonthNames[[int]$now.Month - 1]
}

<#
.SYNOPSIS
  Parses an ISO-8601 timestamp (claude and codex both write UTC with Z) to
  local [datetime], or $null when unparsable.
#>
function ConvertFrom-AgentTimestamp {
  [CmdletBinding()]
  param([string]$Text)

  if (-not $Text) { return $null }
  try {
    $dto = [System.DateTimeOffset]::Parse($Text, [System.Globalization.CultureInfo]::InvariantCulture, [System.Globalization.DateTimeStyles]::AssumeUniversal)
    return $dto.LocalDateTime
  } catch { return $null }
}

<#
.SYNOPSIS
  The canonical form of a project path: trimmed, slashes normalized to
  backslashes, trailing separator removed. The agents' own session stores
  spell cwd inconsistently (mixed separators, an occasional trailing one), so
  every cwd goes through here before it is used as a bucket key.
#>
function Normalize-AgentProjectPath {
  [CmdletBinding()]
  param([string]$Path)

  if (-not $Path) { return "" }
  $p = ([string]$Path).Trim()
  if (-not $p) { return "" }
  $p = $p.Replace('/', '\')
  $p = $p.TrimEnd([char[]]('\', ' '))
  return $p
}

<#
.SYNOPSIS
  Every *.jsonl under a tree, newest by last-write first. Same enumeration
  strategy as Get-NewestAgentSessionFile, measured 76 ms over claude's 551.
#>
function Get-AgentHistoryFiles {
  [CmdletBinding()]
  param([string]$Root)

  $files = @()
  if (-not $Root -or -not [System.IO.Directory]::Exists($Root)) { return $files }
  try {
    foreach ($path in [System.IO.Directory]::EnumerateFiles($Root, "*.jsonl", [System.IO.SearchOption]::AllDirectories)) {
      $files += [pscustomobject]@{
        Path             = $path
        LastWriteTimeUtc = [System.IO.File]::GetLastWriteTimeUtc($path)
      }
    }
  } catch { return @() }
  return @($files | Sort-Object LastWriteTimeUtc -Descending)
}

# One counting bucket for month-level or project-level totals. Model keeps the
# LAST real model seen (never a bracketed placeholder), which is the model the
# estimate gets priced at.
function New-AgentHistoryBucket {
  return [pscustomobject]@{
    Input = 0; Output = 0; CacheRead = 0; CacheWrite = 0; Reasoning = 0
    Entries = 0; Model = ""
  }
}

# The result shape every history reader fills in. MonthCost is $null until a
# cost or an estimate exists; MonthEstimated tells the panel to mark it.
function New-AgentHistoryResult {
  [CmdletBinding()]
  param([string]$Agent, [string]$Format)

  return [pscustomobject]@{
    Agent = $Agent; Format = $Format; Ok = $false; Approximate = $false
    Detail = ""; Error = ""
    MonthTokens = 0; MonthCost = $null; MonthEstimated = $false
    MonthEntries = 0; MonthModel = ""
    Projects = @()
  }
}

<#
.SYNOPSIS
  Month-to-date claude history: every *.jsonl under .claude\projects.

.DESCRIPTION
  Subagent files ARE billed, so every file counts, not just the live one. The
  per-file duplicate rejection is the same as the live reader (the store keeps
  more than one copy of many assistant messages). Month attribution uses
  entry.timestamp (UTC); cwd comes from entry.cwd. Money is the official-price
  estimate for the last real model seen in each bucket, never a store cost.
#>
function Get-ClaudeAgentHistory {
  [CmdletBinding()]
  param(
    [string]$ClaudeDir,
    [long]$MaxBytes = 0
  )

  $budget = if ($MaxBytes -gt 0) { $MaxBytes } else { [long]$script:UsageHistoryMaxBytes }
  $result = New-AgentHistoryResult -Agent "claude" -Format "claude-jsonl"

  if (-not $ClaudeDir) { $ClaudeDir = (Resolve-AgentUsagePaths).ClaudeDir }
  if (-not $ClaudeDir) { $result.Error = "no home directory"; return $result }
  if (-not [System.IO.Directory]::Exists($ClaudeDir)) { $result.Error = "missing directory: $ClaudeDir"; return $result }

  $files = Get-AgentHistoryFiles -Root $ClaudeDir
  if (@($files).Count -eq 0) { $result.Error = "no session files under $ClaudeDir"; return $result }

  $monthStart = Get-AgentMonthStartLocal
  $month = New-AgentHistoryBucket
  $projects = @{}
  $dropped = 0
  $remaining = $budget
  $scannedBytes = 0
  $filesScanned = 0

  foreach ($file in @($files)) {
    if ($remaining -le 0) { break }
    $filesScanned++
    $state = $null
    try {
      $state = Read-AgentSessionLines -Path $file.Path -PreFilter @('"usage"') -MaxBytes $remaining
    } catch { continue }
    $dropped += $state.Dropped
    $scannedBytes += $state.ScannedBytes
    $remaining -= $state.ScannedBytes

    # Dedupe is per file: the same requestId can (and does) appear in more than
    # one session file, and those are separate billable events.
    $seen = New-Object System.Collections.Generic.HashSet[string]
    $fileModel = ""
    $touched = @{}
    foreach ($line in $state.Lines) {
      $textKey = ""
      $keyMatch = $script:UsageRequestIdPattern.Match($line)
      if ($keyMatch.Success) { $textKey = $keyMatch.Groups[1].Value }
      else {
        $idMatch = $script:UsageMessageIdPattern.Match($line)
        if ($idMatch.Success) { $textKey = $idMatch.Groups[1].Value }
        else {
          $uuidMatch = $script:UsageUuidPattern.Match($line)
          if ($uuidMatch.Success) { $textKey = $uuidMatch.Groups[1].Value }
        }
      }
      if ($textKey -and $seen.Contains($textKey)) { continue }

      $entry = $null
      try { $entry = $line | ConvertFrom-Json } catch { continue }
      if ($null -eq $entry) { continue }

      $usage = $null
      if ($null -ne $entry.message -and $null -ne $entry.message.usage) { $usage = $entry.message.usage }
      elseif ($null -ne $entry.usage) { $usage = $entry.usage }
      if ($null -eq $usage) { continue }

      $key = [string]$entry.requestId
      if (-not $key -and $null -ne $entry.message) { $key = [string]$entry.message.id }
      if (-not $key) { $key = [string]$entry.uuid }
      if ($key) { if (-not $seen.Add($key)) { continue } }

      $inMonth = $false
      $ts = ConvertFrom-AgentTimestamp ([string]$entry.timestamp)
      if ($null -ne $ts) { $inMonth = ($ts -ge $monthStart) }
      $cwd = Normalize-AgentProjectPath ([string]$entry.cwd)

      $model = ""
      if ($null -ne $entry.message -and $entry.message.model) {
        $model = ([string]$entry.message.model).Trim()
        if ($model.StartsWith("<")) { $model = "" }
      }
      if ($model) { $fileModel = $model }

      $input = Get-AgentInt64 $usage "input_tokens"
      $output = Get-AgentInt64 $usage "output_tokens"
      $cacheRead = Get-AgentInt64 $usage "cache_read_input_tokens"
      $cacheWrite = Get-AgentInt64 $usage "cache_creation_input_tokens"
      $reasoning = 0
      if ($null -ne $usage.output_tokens_details) {
        $reasoning = Get-AgentInt64 $usage.output_tokens_details "thinking_tokens"
      }

      if ($inMonth) {
        $month.Entries++
        $month.Input += $input
        $month.Output += $output
        $month.CacheRead += $cacheRead
        $month.CacheWrite += $cacheWrite
        $month.Reasoning += $reasoning
      }

      if ($cwd) {
        if (-not $projects.ContainsKey($cwd)) { $projects[$cwd] = New-AgentHistoryBucket }
        $bucket = $projects[$cwd]
        $bucket.Entries++
        $bucket.Input += $input
        $bucket.Output += $output
        $bucket.CacheRead += $cacheRead
        $bucket.CacheWrite += $cacheWrite
        $bucket.Reasoning += $reasoning
        $touched[$cwd] = $true
      }
    }

    # Model attribution is per file (last real model seen inside a file wins) and
    # first across files: files are scanned newest first, so the NEWEST file that
    # names a model prices the month and every project this file touched. Without
    # the ordering guard, the oldest file in the window would overwrite the model
    # with a stale (or empty) value.
    if ($fileModel) {
      if (-not $month.Model) { $month.Model = $fileModel }
      foreach ($touchedKey in $touched.Keys) {
        $bucket = $projects[$touchedKey]
        if (-not $bucket.Model) { $bucket.Model = $fileModel }
      }
    }
  }

  if ($filesScanned -eq 0) { $result.Error = "no session files readable under $ClaudeDir"; return $result }

  $result.Ok = $true
  $result.MonthTokens = $month.Output
  $result.MonthEntries = $month.Entries
  if ($month.Entries -gt 0) {
    $est = Get-AgentEstimatedCost -Model $month.Model -InputTokens $month.Input -OutputTokens $month.Output -CacheReadTokens $month.CacheRead -CacheWriteTokens $month.CacheWrite
    if ($est.Estimated) { $result.MonthCost = $est.Amount; $result.MonthEstimated = $true; $result.MonthModel = $est.Model }
  }

  $projectsOut = @()
  foreach ($key in $projects.Keys) {
    $bucket = $projects[$key]
    $est = Get-AgentEstimatedCost -Model $bucket.Model -InputTokens $bucket.Input -OutputTokens $bucket.Output -CacheReadTokens $bucket.CacheRead -CacheWriteTokens $bucket.CacheWrite
    $name = [System.IO.Path]::GetFileName($key)
    if (-not $name) { $name = $key }
    $projectsOut += [pscustomobject]@{
      Path = $key; Name = $name
      Input = $bucket.Input; Output = $bucket.Output
      CacheRead = $bucket.CacheRead; CacheWrite = $bucket.CacheWrite
      Reasoning = $bucket.Reasoning; Entries = $bucket.Entries; Model = $bucket.Model
      Cost = $(if ($est.Estimated) { $est.Amount } else { $null })
      Estimated = $est.Estimated
    }
  }
  $result.Projects = @($projectsOut | Sort-Object Output -Descending)

  $result.Detail = ("mes + {0} proyectos - {1} archivos, {2} bytes" -f @($result.Projects).Count, $filesScanned, $scannedBytes)
  if ($dropped -gt 0 -or $remaining -le 0) {
    $result.Approximate = $true
    $result.Detail = $result.Detail + (" - aproximada: lineas fuera del presupuesto de {0} MB" -f [int]($budget / 1MB))
  }
  return $result
}

<#
.SYNOPSIS
  Month-to-date codex history: every *.jsonl under .codex\sessions.

.DESCRIPTION
  Per session file the LAST counter record wins, exactly like the live reader,
  and both cumulative shapes count: token_usage_record
  (payload.turn_token_usage) and the terminal token_count
  (payload.info.total_token_usage). Month attribution uses the last record's
  root timestamp (file mtime when missing): a session is billed to the month it
  ENDED. cwd comes from the last turn_context record seen. Money is the
  official-price estimate, with OpenAI's cached-subtract rule applied.
#>
function Get-CodexAgentHistory {
  [CmdletBinding()]
  param(
    [string]$CodexDir,
    [long]$MaxBytes = 0
  )

  $budget = if ($MaxBytes -gt 0) { $MaxBytes } else { [long]$script:UsageHistoryMaxBytes }
  $result = New-AgentHistoryResult -Agent "codex" -Format "codex-jsonl"

  if (-not $CodexDir) { $CodexDir = (Resolve-AgentUsagePaths).CodexDir }
  if (-not $CodexDir) { $result.Error = "no home directory"; return $result }
  if (-not [System.IO.Directory]::Exists($CodexDir)) { $result.Error = "missing directory: $CodexDir"; return $result }

  $files = Get-AgentHistoryFiles -Root $CodexDir
  if (@($files).Count -eq 0) { $result.Error = "no session files under $CodexDir"; return $result }

  $monthStart = Get-AgentMonthStartLocal
  $month = New-AgentHistoryBucket
  $projects = @{}
  $dropped = 0
  $remaining = $budget
  $scannedBytes = 0
  $filesScanned = 0
  $noCwd = 0

  foreach ($file in @($files)) {
    if ($remaining -le 0) { break }
    $filesScanned++
    $state = $null
    try {
      # Counters live on token_usage lines; cwd on turn_context lines; the model
      # on session_meta / turn_context lines. All three are worth a parse.
      $state = Read-AgentSessionLines -Path $file.Path -PreFilter @("token_usage", '"cwd"', '"model"') -MaxBytes $remaining
    } catch { continue }
    $dropped += $state.Dropped
    $scannedBytes += $state.ScannedBytes
    $remaining -= $state.ScannedBytes

    $lastUsage = $null
    $lastStamp = $null
    $fileCwd = ""
    $fileModel = ""
    foreach ($line in $state.Lines) {
      $entry = $null
      try { $entry = $line | ConvertFrom-Json } catch { continue }
      if ($null -eq $entry -or $null -eq $entry.payload) { continue }
      $payload = $entry.payload

      if ($payload.cwd) { $fileCwd = Normalize-AgentProjectPath ([string]$payload.cwd) }
      if ($payload.model) {
        $model = ([string]$payload.model).Trim()
        if ($model -and -not $model.StartsWith("<")) { $fileModel = $model }
      }

      $usage = $null
      if ($null -ne $payload.turn_token_usage) { $usage = $payload.turn_token_usage }
      elseif ($null -ne $payload.info -and $null -ne $payload.info.total_token_usage) { $usage = $payload.info.total_token_usage }
      elseif ($null -ne $payload.usage) { $usage = $payload.usage }
      if ($null -eq $usage) { continue }

      $lastUsage = $usage
      $lastStamp = ConvertFrom-AgentTimestamp ([string]$entry.timestamp)
    }

    if ($null -eq $lastUsage) { continue }

    $stamp = $lastStamp
    if ($null -eq $stamp) { $stamp = $file.LastWriteTimeUtc.ToLocalTime() }
    $inMonth = ($stamp -ge $monthStart)

    $input = Get-AgentInt64 $lastUsage "input_tokens"
    $output = Get-AgentInt64 $lastUsage "output_tokens"
    $cachedInput = Get-AgentInt64 $lastUsage "cached_input_tokens"
    $cacheWrite = Get-AgentInt64 $lastUsage "cache_write_input_tokens"
    $reasoning = Get-AgentInt64 $lastUsage "reasoning_output_tokens"

    if ($inMonth) {
      $month.Entries++
      $month.Input += $input
      $month.Output += $output
      $month.CacheRead += $cachedInput
      $month.CacheWrite += $cacheWrite
      $month.Reasoning += $reasoning
      # Files are scanned newest first: the NEWEST file that names a model wins,
      # so an older session cannot overwrite the pricing model.
      if (-not $month.Model -and $fileModel) { $month.Model = $fileModel }
    }

    if ($fileCwd) {
      if (-not $projects.ContainsKey($fileCwd)) { $projects[$fileCwd] = New-AgentHistoryBucket }
      $bucket = $projects[$fileCwd]
      $bucket.Entries++
      $bucket.Input += $input
      $bucket.Output += $output
      $bucket.CacheRead += $cachedInput
      $bucket.CacheWrite += $cacheWrite
      $bucket.Reasoning += $reasoning
      if (-not $bucket.Model -and $fileModel) { $bucket.Model = $fileModel }
    } else {
      $noCwd++
    }
  }

  if ($filesScanned -eq 0) { $result.Error = "no session files readable under $CodexDir"; return $result }

  $result.Ok = $true
  $result.MonthTokens = $month.Output
  $result.MonthEntries = $month.Entries
  if ($month.Entries -gt 0) {
    $uncached = [Math]::Max(0, $month.Input - $month.CacheRead)
    $est = Get-AgentEstimatedCost -Model $month.Model -InputTokens $uncached -OutputTokens $month.Output -CacheReadTokens $month.CacheRead -CacheWriteTokens $month.CacheWrite
    if ($est.Estimated) { $result.MonthCost = $est.Amount; $result.MonthEstimated = $true; $result.MonthModel = $est.Model }
  }

  $projectsOut = @()
  foreach ($key in $projects.Keys) {
    $bucket = $projects[$key]
    $uncached = [Math]::Max(0, $bucket.Input - $bucket.CacheRead)
    $est = Get-AgentEstimatedCost -Model $bucket.Model -InputTokens $uncached -OutputTokens $bucket.Output -CacheReadTokens $bucket.CacheRead -CacheWriteTokens $bucket.CacheWrite
    $name = [System.IO.Path]::GetFileName($key)
    if (-not $name) { $name = $key }
    $projectsOut += [pscustomobject]@{
      Path = $key; Name = $name
      Input = $bucket.Input; Output = $bucket.Output
      CacheRead = $bucket.CacheRead; CacheWrite = $bucket.CacheWrite
      Reasoning = $bucket.Reasoning; Entries = $bucket.Entries; Model = $bucket.Model
      Cost = $(if ($est.Estimated) { $est.Amount } else { $null })
      Estimated = $est.Estimated
    }
  }
  $result.Projects = @($projectsOut | Sort-Object Output -Descending)

  $result.Detail = ("mes + {0} proyectos - {1} archivos, {2} bytes" -f @($result.Projects).Count, $filesScanned, $scannedBytes)
  if ($noCwd -gt 0) { $result.Detail = $result.Detail + (" - {0} sesiones sin cwd atribuible" -f $noCwd) }
  if ($dropped -gt 0 -or $remaining -le 0) {
    $result.Approximate = $true
    $result.Detail = $result.Detail + (" - aproximada: lineas fuera del presupuesto de {0} MB" -f [int]($budget / 1MB))
  }
  return $result
}

# The month SELECT. The start instant is a computed int64 injected by the
# reader; the literal ">= 0" is replaced with ">= <ms>" before running, which
# keeps the statement free of double quotes (PowerShell 5.1 native marshalling)
# and free of untrusted input.
$script:UsageHistoryMonthSql = @'
SELECT COALESCE(SUM(tokens_input), 0),
       COALESCE(SUM(tokens_output), 0),
       COALESCE(SUM(tokens_reasoning), 0),
       COALESCE(SUM(tokens_cache_read), 0),
       COALESCE(SUM(tokens_cache_write), 0),
       COALESCE(SUM(cost), 0),
       COUNT(*)
FROM session WHERE time_updated >= 0
'@ -replace "`r?`n", " "

# The per-project SELECT: one row per canonical project root, this month's
# totals. project.worktree is the canonical repo root opencode itself tracks.
$script:UsageHistoryProjectSql = @'
SELECT p.worktree,
       COALESCE(SUM(s.tokens_input), 0),
       COALESCE(SUM(s.tokens_output), 0),
       COALESCE(SUM(s.cost), 0),
       COUNT(*)
FROM session s JOIN project p ON s.project_id = p.id
WHERE s.time_updated >= 0
GROUP BY p.worktree
ORDER BY 3 DESC
'@ -replace "`r?`n", " "

<#
.SYNOPSIS
  Month-to-date opencode history out of its own SQLite store.

.DESCRIPTION
  Two indexed SELECTs through Read-SqliteQuery: one totals the month (sessions
  with time_updated >= month start) and one splits the same window per
  project.worktree. Money is REAL - session.cost as recorded, 0 is a real zero
  for a local model - so Estimated stays false and the estimator is never asked.
#>
function Get-OpenCodeAgentHistory {
  [CmdletBinding()]
  param(
    [string]$DatabasePath,
    [int]$BusyTimeoutMs = 1200
  )

  $result = New-AgentHistoryResult -Agent "opencode" -Format "sqlite"

  if (-not $DatabasePath) { $DatabasePath = (Resolve-AgentUsagePaths).OpenCodeDb }
  $result.Detail = [System.IO.Path]::GetFileName($DatabasePath)

  if (-not [System.IO.File]::Exists($DatabasePath)) {
    $result.Error = "database not found: $DatabasePath"
    return $result
  }

  $lib = [System.IO.Path]::Combine($PSScriptRoot, "Read-SqliteQuery.ps1")
  if (-not [System.IO.File]::Exists($lib)) {
    $result.Error = "SQLite reader not found: $lib"
    return $result
  }
  . $lib

  $monthStart = Get-AgentMonthStartLocal
  $startMs = 0
  try {
    $startMs = ([DateTimeOffset]::new($monthStart.ToUniversalTime(), [TimeSpan]::Zero)).ToUnixTimeMilliseconds()
  } catch {
    $result.Error = Get-AgentFirstLine $_.Exception.Message
    return $result
  }
  $monthSql = $script:UsageHistoryMonthSql.Replace(">= 0", ">= " + $startMs.ToString([System.Globalization.CultureInfo]::InvariantCulture))
  $projectSql = $script:UsageHistoryProjectSql.Replace(">= 0", ">= " + $startMs.ToString([System.Globalization.CultureInfo]::InvariantCulture))

  $query = $null
  try {
    $query = Invoke-SqliteQuery -DatabasePath $DatabasePath -Sql $monthSql -BusyTimeoutMs $BusyTimeoutMs
  } catch {
    $result.Error = Get-AgentFirstLine $_.Exception.Message
    return $result
  }
  if (-not $query.Ok) { $result.Error = Get-AgentFirstLine $query.Error; return $result }

  $result.Ok = $true
  $rows = @($query.Rows)
  if ($rows.Count -gt 0) {
    $row = $rows[0]
    $sessions = [int64]$row[6]
    $result.MonthTokens = [int64]$row[1]
    $result.MonthEntries = $sessions
    if ($sessions -gt 0) {
      $cost = 0.0
      if ([double]::TryParse(([string]$row[5]), [System.Globalization.NumberStyles]::Float, [System.Globalization.CultureInfo]::InvariantCulture, [ref]$cost)) {
        $result.MonthCost = $cost
        $result.MonthEstimated = $false
      }
    }
  }

  $pquery = $null
  try {
    $pquery = Invoke-SqliteQuery -DatabasePath $DatabasePath -Sql $projectSql -BusyTimeoutMs $BusyTimeoutMs
  } catch {
    $result.Error = Get-AgentFirstLine $_.Exception.Message
    return $result
  }
  if (-not $pquery.Ok) { $result.Error = Get-AgentFirstLine $pquery.Error; return $result }

  $projectsOut = @()
  foreach ($row in @($pquery.Rows)) {
    $key = Normalize-AgentProjectPath ([string]$row[0])
    if (-not $key) { continue }
    $name = [System.IO.Path]::GetFileName($key)
    if (-not $name) { $name = $key }
    $cost = $null
    $costText = ([string]$row[3]).Trim()
    if ($costText) {
      $parsed = 0.0
      if ([double]::TryParse($costText, [System.Globalization.NumberStyles]::Float, [System.Globalization.CultureInfo]::InvariantCulture, [ref]$parsed)) { $cost = $parsed }
    }
    $projectsOut += [pscustomobject]@{
      Path = $key; Name = $name
      Input = [int64]$row[1]; Output = [int64]$row[2]
      CacheRead = 0; CacheWrite = 0
      Reasoning = 0; Entries = [int64]$row[4]; Model = ""
      Cost = $cost
      Estimated = $false
    }
  }
  $result.Projects = @($projectsOut | Sort-Object Output -Descending)

  $result.Detail = ("mes + {0} proyectos - via {1}" -f @($result.Projects).Count, $query.Provider)
  return $result
}

<#
.SYNOPSIS
  One snapshot of all three agents' history, for the per-agent panel and the
  self test.

.DESCRIPTION
  Sequential, like Get-AgentUsageSnapshot: the scans are bounded by
  UsageHistoryMaxBytes (8 MB) across all files per agent, newest first, and the
  SQLite reads run with a busy timeout. A store that is missing, locked or
  unparsable comes back as Ok = $false with the reason in Error, never as
  zeroes; an empty month is "sin actividad", not a failure. Projects come only
  from the agents' own session cwd values (each agent history already carries
  a per-project split, ranked by spend); no external program is consulted.
#>
function Get-AgentHistorySnapshot {
  [CmdletBinding()]
  param(
    [string]$HomeDir,
    [int]$BusyTimeoutMs = 1200,
    [long]$MaxBytes = 0
  )

  $paths = Resolve-AgentUsagePaths -HomeDir $HomeDir

  $agents = @()
  $agents += Get-ClaudeAgentHistory -ClaudeDir $(if ($paths) { $paths.ClaudeDir } else { "" }) -MaxBytes $MaxBytes
  $agents += Get-CodexAgentHistory -CodexDir $(if ($paths) { $paths.CodexDir } else { "" }) -MaxBytes $MaxBytes
  $agents += Get-OpenCodeAgentHistory -DatabasePath $(if ($paths) { $paths.OpenCodeDb } else { "" }) -BusyTimeoutMs $BusyTimeoutMs

  $monthStart = Get-AgentMonthStartLocal

  return [pscustomobject]@{
    TakenAt    = Get-Date
    MonthStart = $monthStart
    MonthLabel = $script:AgentMonthNames[[int]$monthStart.Month - 1]
    Agents     = $agents
  }
}
