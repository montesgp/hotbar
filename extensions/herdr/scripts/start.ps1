$ErrorActionPreference = "Stop"

# scripts/ now sits three levels under the repo root (extensions/herdr/scripts/,
# moved out of core in O3); load a repo-root .env if present (simple KEY=VALUE
# parser, comments/blank lines ignored, never overrides a variable already set
# in the environment). A value may be wrapped in one pair of matching quotes
# ("..." or '...'), which is stripped - Windows paths with spaces are commonly
# quoted in a .env file.
$repoRoot = [System.IO.Path]::GetFullPath([System.IO.Path]::Combine($PSScriptRoot, "..", "..", ".."))
$envFile = [System.IO.Path]::Combine($repoRoot, ".env")
if (Test-Path -LiteralPath $envFile) {
  foreach ($line in Get-Content -LiteralPath $envFile) {
    $trimmed = $line.Trim()
    if (-not $trimmed -or $trimmed.StartsWith("#")) { continue }
    $eq = $trimmed.IndexOf("=")
    if ($eq -lt 0) { continue }
    $key = $trimmed.Substring(0, $eq).Trim()
    $value = $trimmed.Substring($eq + 1).Trim()
    if ($value.Length -ge 2) {
      $first = $value.Substring(0, 1)
      $last = $value.Substring($value.Length - 1, 1)
      if (($first -eq '"' -and $last -eq '"') -or ($first -eq "'" -and $last -eq "'")) {
        $value = $value.Substring(1, $value.Length - 2)
      }
    }
    if ($key -and -not (Test-Path "env:$key")) { Set-Item -Path "env:$key" -Value $value }
  }
}

$port = 20128
if ((netstat -an | Select-String -Pattern ":$port\s+.*LISTENING").Count -gt 0) { Write-Output "OmniRoute: already UP on $port"; exit 0 }
$node = if ($env:OMNIROUTE_NODE) { $env:OMNIROUTE_NODE } else { "node" }
if (-not $env:OMNIROUTE_ENTRY) {
  Write-Output "OmniRoute: start FAILED - OMNIROUTE_ENTRY is not set (see .env.example)"
  exit 1
}
$entry = $env:OMNIROUTE_ENTRY
try {
  Start-Process -FilePath $node -ArgumentList "`"$entry`"","serve","--daemon","--no-open" -WindowStyle Hidden
  Write-Output "OmniRoute: starting daemon... (status in ~15s)"
  exit 0
} catch {
  Write-Output "OmniRoute: start FAILED - $($_.Exception.Message)"
  exit 2
}
