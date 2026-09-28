<#
.SYNOPSIS
  Is the OmniRoute gateway listening? Windowless, bounded, no fabricated answer.

.DESCRIPTION
  One question, one source of truth: `netstat -an` and a search for a LISTENING
  socket on the gateway port. The widget has no business asking the gateway's HTTP
  API for this, because every /api route is authenticated and the bar must never
  read, print or store a key.

  The check runs through the windowless helper (legacy\windows-widget\lib\Invoke-Native.ps1 -
  shared with the core usage reader, so it stays in the core tree even though
  this reader is an optional extension), so opening the inline panel never
  flashes a console window on top of the desktop.

  NAMING RULE (do not "simplify" this): PowerShell variables are case-insensitive,
  so a variable called $Port that holds a Process object silently overwrites a
  $Port constant living in the same scope. The original bug was exactly that: the
  process landed in $Port, the port constant was gone, and the regex matched
  Process.ToString() instead of a TCP port - so a running gateway reported DOWN.
  The port therefore lives in $GatewayPort and the probe result in $portRun, and
  neither name is ever reused for the other.

  Dot-source it, do not run it:

    . (Join-Path $PSScriptRoot "Get-OmniRouteStatus.ps1")
#>

# The OmniRoute gateway's fixed port. Named with a $Gateway prefix precisely so no
# other variable in this scope can collide with it.
$script:GatewayPort = 20128

<#
.SYNOPSIS
  Starts the listening-socket probe and returns a job, so the database read can be
  in flight at the same time.

.NOTES
  Call Complete-OrbitbarGatewayProbe (or Get-OrbitbarGatewayUp) with the result. The
  two checks are independent, and the panel pays for the slower one instead of
  their sum.
#>
function Start-OrbitbarGatewayProbe {
  [CmdletBinding()]
  param()

  if (-not $script:InvokeNativeLoaded) {
    # Shared helper, stays in the core tree: extensions/omniroute/legacy-widget/ ->
    # extensions/omniroute -> extensions -> repo root -> legacy/windows-widget/lib.
    $widgetLib = [System.IO.Path]::GetFullPath([System.IO.Path]::Combine($PSScriptRoot, "..", "..", "..", "legacy", "windows-widget", "lib"))
    $lib = [System.IO.Path]::Combine($widgetLib, "Invoke-Native.ps1")
    if (-not [System.IO.File]::Exists($lib)) {
      return [pscustomobject]@{ Process = $null; StdOutTask = $null; StdErrTask = $null; StartError = "windowless helper not found: $lib" }
    }
    . $lib
  }

  return (Start-NativeProcess -FilePath "netstat" -Arguments @("-an"))
}

<#
.SYNOPSIS
  Finishes a probe job and reports UP or DOWN.

.PARAMETER Job
  Job from Start-OrbitbarGatewayProbe. A null job means the helper was missing, which
  is DOWN-with-a-reason, never UP by default.

.PARAMETER TimeoutMs
  Bounded wait. netstat on a busy machine can take a second; 3 s is generous and
  still short enough for a click.

.NOTES
  Returns an object with Up (boolean), Port (the port actually checked) and Detail
  (a short reason, always populated when Up is false). Never throws: a missing
  netstat, a timeout or an unreadable answer all come back as Down with the reason.
#>
function Complete-OrbitbarGatewayProbe {
  [CmdletBinding()]
  param(
    [Parameter(Mandatory = $true)]$Job,
    [int]$TimeoutMs = 3000
  )

  $result = [pscustomobject]@{
    Up     = $false
    Port   = $script:GatewayPort
    Detail = ""
  }

  $portRun = Complete-NativeProcess -Job $Job -TimeoutMs $TimeoutMs

  if ($portRun.StartError) {
    $result.Detail = "netstat unavailable"
    return $result
  }
  if ($portRun.ExitCode -ne 0) {
    $result.Detail = "netstat exit $($portRun.ExitCode)"
    return $result
  }
  if (-not $portRun.Output) {
    $result.Detail = "netstat returned nothing"
    return $result
  }

  # The port constant is interpolated into the pattern, never the process object.
  $pattern = ":" + $script:GatewayPort + "\s+.*LISTENING"
  foreach ($line in ($portRun.Output -split "`r?`n")) {
    if ($line -match $pattern) {
      $result.Up = $true
      $result.Detail = "listening"
      return $result
    }
  }

  $result.Detail = "port $($script:GatewayPort) not listening"
  return $result
}
