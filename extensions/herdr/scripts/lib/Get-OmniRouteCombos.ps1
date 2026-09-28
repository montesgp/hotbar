<#
.SYNOPSIS
  Reads the routing combos straight out of OmniRoute's SQLite store.

.DESCRIPTION
  The status popup used to shell out to `node omniroute.mjs combo list`. That call
  cost 3-9 s per frame (tsx/Commander boot, an isServerUp() health budget that always
  times out, and gateway latency before routing even starts) and it spawned the CLI with
  `shell:true, windowsHide:false`, which allocated a console and flashed windows on the
  Windows Terminal broker. The gateway reads the same SQLite file the CLI reads, so the
  popup reads it directly: a read-only SELECT costs 30-60 ms and spawns no console.

  Storage path: bin/cli/data-dir.mjs::resolveStoragePath -> <dataDir>\storage.sqlite
  Schema: combos(id, name, data, sort_order, created_at, updated_at, ...) plus
  key_value(namespace, key, value). The displayed fields are resolved exactly the way
  bin/cli/commands/combo.mjs::runComboListCommand and
  src/lib/db/repositories/sqliteComboRepository.ts::getCombos resolve them:

    name     -> data.name, falling back to the name column, then the id (CLI: name ?? id ?? "?")
    strategy -> data.strategy, defaulting to "priority"   (CLI: strategy ?? "priority")
    enabled  -> data.enabled, where only an explicit false disables (CLI: enabled !== false)
    active   -> key_value('settings','activeCombo'), compared against name or id
    order    -> ORDER BY sort_order ASC, name COLLATE NOCASE ASC, same as getCombos()

  IMPORTANT SOURCING NOTE: the gateway keeps activeCombo in runtime memory and only
  persists it to key_value if a user ever saves it as a setting. In the install this
  reader targets, key_value has NO activeCombo key, so ActiveComboName is empty and a
  caller must NOT render U+25CF/U+25CB as if it knew. The live value is only reachable through
  GET /api/settings, which requires gateway auth (this install: 401 without a key).

  Dot-source it, do not run it:

    . (Join-Path $PSScriptRoot "lib\Get-OmniRouteCombos.ps1")
#>

# Mirrors bin/cli/data-dir.mjs: DATA_DIR wins, then the legacy ~/.omniroute directory
# if it exists, then %APPDATA%\omniroute on Windows. The directory probe uses
# [System.IO.Directory]::Exists rather than Test-Path: the first Test-Path in a
# process pays ~70 ms to initialise the PowerShell provider machinery, which is a
# large slice of this popup's whole budget.
function Resolve-OmniRouteDataDir {
  $configured = $env:DATA_DIR
  if ($configured -and $configured.Trim()) { return [IO.Path]::GetFullPath($configured.Trim()) }

  $homeDir = $env:USERPROFILE
  if (-not $homeDir) { $homeDir = $env:HOME }
  if ($homeDir) {
    $legacy = [System.IO.Path]::Combine($homeDir, ".omniroute")
    if ([System.IO.Directory]::Exists($legacy)) { return $legacy }
  }

  $appData = $env:APPDATA
  if (-not $appData) { $appData = [System.IO.Path]::Combine($homeDir, "AppData", "Roaming") }
  return [System.IO.Path]::Combine($appData, "omniroute")
}

# bin/cli/data-dir.mjs::resolveStoragePath
function Get-OmniRouteStoragePath {
  return [System.IO.Path]::Combine((Resolve-OmniRouteDataDir), "storage.sqlite")
}

# One SELECT for the four display fields plus the active combo name. The column order
# is the contract the reader below parses positionally: id, name, strategy, enabled,
# activeCombo.
#
# `replace(..., char(31), ' ')` guards the ASCII unit separator the reader splits on,
# so a combo name can never fake a column break. There are no double quotes in the
# statement, so it survives the PS 5.1 native argument marshalling unchanged.
#
# The here-string is single-quoted on purpose: '$' has no special meaning in SQLite's
# JSON paths, and single-quoting means PowerShell cannot touch it.
$script:ComboSql = @'
SELECT replace(id, char(31), ' '),
       replace(COALESCE(json_extract(data, '$.name'), name, id, '?'), char(31), ' '),
       COALESCE(json_extract(data, '$.strategy'), 'priority'),
       COALESCE(json_extract(data, '$.enabled'), 1),
       COALESCE((SELECT value FROM key_value WHERE namespace = 'settings' AND key = 'activeCombo'), '')
FROM combos ORDER BY sort_order ASC, name COLLATE NOCASE ASC
'@ -replace "`r?`n", " "

<#
.SYNOPSIS
  Starts the combo read and returns a job, so it can overlap the port check.

.PARAMETER StoragePath
  SQLite file to read. Defaults to the OmniRoute data directory.

.PARAMETER Provider
  Passed through to Invoke-SqliteQuery. "auto" by default.

.NOTES
  Never throws. A missing database, a missing reader or a locked file all come back as
  a failed job, which the caller renders as "data unavailable".
#>
function Start-OmniRouteComboRead {
  [CmdletBinding()]
  param(
    [string]$StoragePath,
    [ValidateSet("auto", "sqlite3", "winsqlite")][string]$Provider = "auto"
  )

  if (-not $StoragePath) { $StoragePath = Get-OmniRouteStoragePath }
  return [pscustomobject]@{
    StoragePath = $StoragePath
    Provider    = $Provider
  }
}

<#
.SYNOPSIS
  Finishes a job from Start-OmniRouteComboRead.

.NOTES
  Returns Ok, Combos, Provider and Error. On success Combos is a list of records with
  Name, Id, Strategy, Enabled and Active, in the order the CLI would print them.

  The reader is dot-sourced here rather than at the top of the popup so that a script
  which never asks for combos does not pay to parse it.
#>
function Complete-OmniRouteComboRead {
  [CmdletBinding()]
  param([Parameter(Mandatory = $true)]$Job)

  $result = [pscustomobject]@{
    Ok              = $false
    Combos          = @()
    Provider        = ""
    Error           = ""
    ActiveComboName = ""
  }

  $lib = [System.IO.Path]::Combine($PSScriptRoot, "Read-SqliteQuery.ps1")
  if (-not [System.IO.File]::Exists($lib)) {
    $result.Error = "SQLite reader not found: $lib"
    return $result
  }
  . $lib

  $query = Invoke-SqliteQuery -DatabasePath $Job.StoragePath -Sql $script:ComboSql -Provider $Job.Provider
  if (-not $query.Ok) {
    $result.Error = $query.Error
    return $result
  }

  $result.Ok = $true
  $result.Provider = $query.Provider

  $combos = @()
  foreach ($row in $query.Rows) {
    $id = $row[0]
    $name = $row[1]

    # key_value stores settings as JSON.stringify(value), so the quotes are part of the
    # stored value and not part of the name.
    $activeName = $row[$row.Count - 1].Trim()
    if ($activeName.Length -ge 2 -and $activeName.StartsWith('"') -and $activeName.EndsWith('"')) {
      $activeName = $activeName.Substring(1, $activeName.Length - 2)
    }
    if (-not $result.ActiveComboName) { $result.ActiveComboName = $activeName }

    $combos += [pscustomobject]@{
      Name     = $name
      Id       = $id
      Strategy = $row[2]
      # json_extract yields 0 for an explicit false, 1 for true, and COALESCE turns a
      # missing key into 1, which is what the CLI's `enabled !== false` means.
      Enabled  = ($row[3] -ne "0" -and $row[3] -ne "false")
      Active   = ($activeName -ne "" -and ($activeName -eq $name -or $activeName -eq $id))
    }
  }

  $result.Combos = $combos
  return $result
}
