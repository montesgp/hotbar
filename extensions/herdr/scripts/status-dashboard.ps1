<#
.SYNOPSIS
  OmniRoute gateway status snapshot, rendered inside a Herdr popup.

.DESCRIPTION
  Prints gateway UP/DOWN, the gateway URL and the configured routing combos ONCE, as a
  single frame taken at open time.

  There is no periodic refresh and no repaint: repainting every couple of seconds lags
  the screen and adds nothing, so reopening the popup is how you get fresh data. The
  script therefore renders one frame and then stays alive without repainting, because a
  Herdr popup is a session modal that closes when its command exits. It stays alive
  waiting for exactly one of two dismiss keys - `q` or Enter - and ignores every other
  key, Escape included. A popup is a session modal, so it consumes every byte the
  terminal sends: an "any key closes" rule was dismissed by ambient Escape bytes from an
  agent, so only a deliberate key closes it now.

  Combo data is read straight out of OmniRoute's SQLite store, read-only. The previous
  `node omniroute.mjs combo list` call cost 3-9 s per frame (tsx/Commander boot, an
  isServerUp() health budget that always times out, and gateway latency before routing
  even starts) and it spawned the CLI with a visible console, which is what flashed
  windows on the Windows Terminal broker. The direct read costs 30-60 ms and starts no
  console. The gateway is not consulted: it reads the same file.

  The only external commands left are that read and the `netstat` port check, and both
  go through the windowless helper in lib\Invoke-Native.ps1
  (System.Diagnostics.Process + CreateNoWindow). They are started back to back and
  collected afterwards, so their cost overlaps instead of adding up.

  The wait is always bounded. `-MaxSeconds` is the hard cap in every code path,
  including the fallback used when the host cannot report console keypresses, so the
  popup can never become an orphan.

.EXAMPLE
  .\status-dashboard.ps1 -Once
#>
[CmdletBinding()]
param(
  [switch]$Once,
  [int]$MaxSeconds = 300,
  [switch]$NoKeyWatch
)

$ErrorActionPreference = "SilentlyContinue"

# The combo markers are U+25CF / U+25CB. A console already on code page 65001 (the
# Windows Terminal default, which is what the popup runs in) renders them as they are.
# A console on an OEM code page such as 850 cannot encode them and turns the icon into
# a tab or a question mark. The probe costs ~3 ms; the switch is only paid in a legacy
# or redirected host. The BOM is suppressed so nothing is prepended to the frame.
try {
  if ([Console]::OutputEncoding.CodePage -ne 65001) {
    [Console]::OutputEncoding = New-Object System.Text.UTF8Encoding $false
  }
} catch { }

$Port = 20128
$ESC = [char]27
$IconActive = [char]0x25CF      # filled dot
$IconInactive = [char]0x25CB    # hollow dot
$NetstatTimeoutMs = 5000

# Paths are joined with [System.IO.Path]::Combine rather than Join-Path throughout this
# popup's load path. The first Join-Path in a process autoloads
# Microsoft.PowerShell.Management, which measures ~95 ms - more than the SQLite read
# and the port check combined. Do not "tidy" these back into Join-Path.
$libRoot = $PSScriptRoot
foreach ($needed in @("lib\Invoke-Native.ps1", "lib\Get-OmniRouteCombos.ps1")) {
  if (-not [System.IO.File]::Exists([System.IO.Path]::Combine($libRoot, $needed))) {
    Write-Output "OmniRoute: missing helper $needed"
    exit 1
  }
}
. ([System.IO.Path]::Combine($libRoot, "lib\Invoke-Native.ps1"))
. ([System.IO.Path]::Combine($libRoot, "lib\Get-OmniRouteCombos.ps1"))

function Home {
  # Put the frame at the top of the popup. No repaint happens after this, so
  # this only positions the single frame.
  try { [Console]::SetCursorPosition(0, 0) } catch { Write-Host "$ESC[H" -NoNewline }
}

function Get-FirstLine([string]$text) {
  if (-not $text) { return "" }
  $line = ""
  foreach ($l in ($text -split "`r?`n")) { if ($l.Trim()) { $line = $l.Trim(); break } }
  return $line
}

# The only two keys that end the wait live in lib\Wait-PopupDismiss.ps1, so the
# dismiss contract can be exercised directly in tests instead of through a popup.

# UP/DOWN from the listening socket, read windowlessly. Takes the job so the port check
# can be in flight while the database read is being collected.
function Test-GatewayUp($Job) {
  $r = Complete-NativeProcess -Job $Job -TimeoutMs $NetstatTimeoutMs
  if (-not $r.Output) { return $false }
  return ($r.Output -split "`r?`n" | Where-Object { $_ -match ":$Port\s+.*LISTENING" }).Count -gt 0
}

# Combo lines in the exact shape `omniroute combo list` printed them: a two-space
# indent, the active/inactive marker, the name padded to 25, the strategy in brackets
# padded to 12, then enabled/disabled.
function Get-ComboLines($Job) {
  $read = Complete-OmniRouteComboRead -Job $Job

  if (-not $read.Ok) {
    $detail = Get-FirstLine $read.Error
    if (-not $detail) { $detail = "read failed" }
    # A failed read is never reported as an empty configuration: a missing or locked
    # database must not read like "no combos configured".
    return @("  (datos de combos no disponibles: $detail)")
  }

  $lines = @()
  # The gateway keeps activeCombo in runtime memory and key_value usually has no such
  # key (this install: none). Rendering U+25CB would claim a definite answer we do not have,
  # so icons only appear when the reader actually found the setting.
  $activeKnown = $read.ActiveComboName -ne ""
  foreach ($c in $read.Combos) {
    $icon = ""
    if ($activeKnown) { $icon = if ($c.Active) { $IconActive } else { $IconInactive } }
    $state = if ($c.Enabled) { "enabled" } else { "disabled" }
    $prefix = if ($activeKnown) { "  " + $icon + " " } else { "  " }
    $lines += ($prefix + $c.Name.PadRight(25) + " [" + $c.Strategy.PadRight(12) + "] " + $state)
  }
  if ($lines.Count -eq 0) { $lines += "  (sin combos)" }
  if (-not $activeKnown) {
    $lines += "  (combo activo: no disponible - el gateway pide login)"
  }
  return $lines
}

function Render {
  Home

  # Two independent external calls, started together and collected after, so the frame
  # costs the slower one instead of their sum.
  $comboJob = Start-OmniRouteComboRead
  $portJob = Start-NativeProcess -FilePath "netstat" -Arguments @("-an")

  $comboLines = Get-ComboLines -Job $comboJob
  $up = Test-GatewayUp -Job $portJob

  Write-Host ""
  Write-Host ("  OmniRoute Gateway" + $ESC + "[K")
  Write-Host ("  =================" + $ESC + "[K")
  if ($up) {
    Write-Host ("  Estado: UP   (localhost:$Port)  " + $ESC + "[K") -ForegroundColor Green
    Write-Host ("  " + $ESC + "[K")   # reserved line, keeps the frame height stable
  } else {
    Write-Host ("  Estado: DOWN (localhost:$Port)  " + $ESC + "[K") -ForegroundColor Red
    Write-Host ("  Arranca con: prefix+o  o  herdr plugin action invoke herdr.omniroute.start" + $ESC + "[K") -ForegroundColor Yellow
  }
  Write-Host ("  URL:    http://localhost:$Port" + $ESC + "[K")

  Write-Host ""
  Write-Host ("  Combos:" + $ESC + "[K")
  foreach ($c in $comboLines) { Write-Host ($c + $ESC + "[K") }
  Write-Host ("  " + $ESC + "[K")
  # Snapshot semantics: this timestamp describes the single sample drawn above.
  Write-Host ("  Instantanea: " + (Get-Date -Format "HH:mm:ss") + "  (una sola muestra; no se refresca)" + $ESC + "[K")
  Write-Host ("  Cerrar: q o Enter cierran este popup. Tope ${MaxSeconds}s." + $ESC + "[K")
  Write-Host ("  " + $ESC + "[K")   # reserved line
  Write-Host ($ESC + "[J") -NoNewline
}

function Write-FatalFrame([string]$message) {
  Home
  Write-Host ""
  Write-Host ("  OmniRoute Gateway" + $ESC + "[K")
  Write-Host ("  =================" + $ESC + "[K")
  Write-Host ("  ERROR: " + $message + $ESC + "[K") -ForegroundColor Red
  Write-Host ("  " + $ESC + "[K")
  Write-Host ("  Revisa la instalacion de OmniRoute en este equipo y vuelve a abrir el popup." + $ESC + "[K")
  Write-Host ""
}

try {
  if ($Once) { Render; exit 0 }

  # Loaded here, not at the top: -Once never waits, so it should not pay to parse the
  # dismiss loop.
  . ([System.IO.Path]::Combine($libRoot, "lib\Wait-PopupDismiss.ps1"))

  # One frame, then wait for a dismiss key without repainting. Both the key path and
  # the unsupported-host fallback exit 0, which is what closes the popup.
  Render
  [void](Wait-ForDismiss -TimeoutMs ([int][Math]::Max(1, $MaxSeconds) * 1000) -KeyWatch (-not $NoKeyWatch))
  exit 0
} catch {
  Write-FatalFrame $_.Exception.Message
  exit 3
}
