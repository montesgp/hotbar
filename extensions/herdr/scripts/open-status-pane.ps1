<#
.SYNOPSIS
  Opens the OmniRoute gateway status popup, once, in the current context.

.DESCRIPTION
  The status pane is declared with placement = "popup", which is a session-modal
  terminal: it takes all terminal input and closes on its own when the command
  exits. The running command is therefore the state, so there is nothing to probe
  and no existing-pane detection to keep in sync. The dashboard closes itself on
  `q`, on Enter, or on the -MaxSeconds cap; every other key, Escape included, is
  ignored, because a session modal swallows the whole byte stream and an any-key
  rule lets ambient input dismiss the popup.

  The Herdr call goes through the windowless helper in lib\Invoke-Native.ps1, so
  opening the popup never flashes a console window.
#>
$ErrorActionPreference = "SilentlyContinue"

$herdr = if ($env:HERDR_BIN_PATH) { $env:HERDR_BIN_PATH } else { "herdr" }

$lib = Join-Path $PSScriptRoot "lib\Invoke-Native.ps1"
if (-not (Test-Path -LiteralPath $lib)) {
  Write-Output "OmniRoute: missing helper $lib"
  exit 1
}
. $lib

try {
  $r = Invoke-NativeText -FilePath $herdr -Arguments @(
    "plugin", "pane", "open",
    "--plugin", "herdr.omniroute",
    "--entrypoint", "status"
  ) -TimeoutMs 20000

  $text = ""
  if ($r.Text) { $text = ($r.Text -replace "\s+", " ").Trim() }

  if ($text -match "ui_busy") {
    Write-Output "OmniRoute: another Herdr modal is active, status popup not opened. Try again once it closes."
    exit 0
  }
  if ($r.ExitCode -ne 0) {
    Write-Output ("OmniRoute: popup open failed - " + $text)
    exit 1
  }

  Write-Output "OmniRoute: status popup opened (q or Enter closes it)."
  exit 0
} catch {
  Write-Output ("OmniRoute: popup open failed - " + $_.Exception.Message)
  exit 1
}
