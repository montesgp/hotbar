$ErrorActionPreference = "SilentlyContinue"
$port = 20128
$listening = (netstat -an | Select-String -Pattern ":$port\s+.*LISTENING").Count -gt 0
if (-not $listening) { Write-Output "OmniRoute: DOWN (port $port not listening)"; exit 1 }
Write-Output "OmniRoute: UP (port $port listening)"
exit 0
