$ErrorActionPreference = "Stop"

# scripts/ sits directly under the repo root; load a repo-root .env if
# present (simple KEY=VALUE parser, comments/blank lines ignored, never
# overrides a variable already set in the environment).
$repoRoot = [System.IO.Path]::GetFullPath([System.IO.Path]::Combine($PSScriptRoot, ".."))
$envFile = [System.IO.Path]::Combine($repoRoot, ".env")
if (Test-Path -LiteralPath $envFile) {
  foreach ($line in Get-Content -LiteralPath $envFile) {
    $trimmed = $line.Trim()
    if (-not $trimmed -or $trimmed.StartsWith("#")) { continue }
    $eq = $trimmed.IndexOf("=")
    if ($eq -lt 0) { continue }
    $key = $trimmed.Substring(0, $eq).Trim()
    $value = $trimmed.Substring($eq + 1).Trim()
    if ($key -and -not (Test-Path "env:$key")) { Set-Item -Path "env:$key" -Value $value }
  }
}

$port = 20128
if ((netstat -an | Select-String -Pattern ":$port\s+.*LISTENING").Count -gt 0) { Write-Output "OmniRoute: already UP on $port"; exit 0 }
$node = if ($env:OMNIROUTE_NODE) { $env:OMNIROUTE_NODE } else { "node" }
$entry = if ($env:OMNIROUTE_ENTRY) { $env:OMNIROUTE_ENTRY } else { "omniroute/bin/omniroute.mjs" }
try {
  Start-Process -FilePath $node -ArgumentList "`"$entry`"","serve","--daemon","--no-open" -WindowStyle Hidden
  Write-Output "OmniRoute: starting daemon... (status in ~15s)"
  exit 0
} catch {
  Write-Output "OmniRoute: start FAILED - $($_.Exception.Message)"
  exit 2
}
