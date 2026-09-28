<#
.SYNOPSIS
  Windowless external-command helper for the orbitbar widget.

.DESCRIPTION
  Copy of scripts/lib/Invoke-Native.ps1, carried into legacy/windows-widget/lib/ so the widget is
  standalone: the bar must not reach into the Herdr plugin's script tree to read
  data. The contract is identical to the original - System.Diagnostics.Process with
  UseShellExecute = $false and CreateNoWindow = $true - so a child never allocates
  a console host (conhost.exe / OpenConsole.exe) and never flashes a window.

  The widget needs this for two reasons: the `netstat` port check, and the
  `sqlite3.exe` provider inside Read-SqliteQuery.ps1. Both run on the UI thread
  while the user waits for the inline panel, so both must be windowless and both
  must be bounded.

  Windows PowerShell 5.1 note: ProcessStartInfo.ArgumentList does not exist in
  .NET Framework, so the argument string is built here following the
  CommandLineToArgvW quoting rules.

  Dot-source it, do not run it:

    . (Join-Path $PSScriptRoot "Invoke-Native.ps1")
#>

# Quotes a single argument following the CRT/CommandLineToArgvW rules: a quote is
# escaped as \", and backslashes are doubled when they precede a quote or end the
# argument.
function ConvertTo-NativeArgument {
  param([Parameter(Mandatory = $true)][AllowEmptyString()][string]$Argument)

  if ($Argument -notmatch '[\s"]') { return $Argument }

  $escaped = [regex]::Replace($Argument, '(\\*)"', '$1$1\"')
  $escaped = [regex]::Replace($escaped, '(\\+)$', '$1$1')
  return '"' + $escaped + '"'
}

<#
.SYNOPSIS
  Starts an external command without allocating a console window and returns a job.

.PARAMETER FilePath
  Executable path. Bare names such as "netstat" or "sqlite3" resolve through PATH.

.PARAMETER Arguments
  Argument list, passed as an array. Quoted internally.

.NOTES
  Start-NativeProcess and Complete-NativeProcess are split so two independent calls
  can be in flight at once. The inline panel needs a port check and a database read
  at the same time, and paying for them one after the other is pure latency.

  A job with a non-empty StartError means the child never ran; pass it to
  Complete-NativeProcess to get a normal failure result.
#>
function Start-NativeProcess {
  [CmdletBinding()]
  param(
    [Parameter(Mandatory = $true)][string]$FilePath,
    [string[]]$Arguments = @()
  )

  $job = [pscustomobject]@{
    Process    = $null
    StdOutTask = $null
    StdErrTask = $null
    StartError = ""
  }

  $psi = New-Object System.Diagnostics.ProcessStartInfo
  $psi.FileName = $FilePath
  $psi.UseShellExecute = $false
  $psi.CreateNoWindow = $true
  $psi.RedirectStandardOutput = $true
  $psi.RedirectStandardError = $true
  $psi.WindowStyle = [System.Diagnostics.ProcessWindowStyle]::Hidden
  $psi.StandardOutputEncoding = New-Object System.Text.UTF8Encoding
  $psi.StandardErrorEncoding = New-Object System.Text.UTF8Encoding

  $quoted = @()
  foreach ($a in $Arguments) { $quoted += (ConvertTo-NativeArgument $a) }
  $psi.Arguments = ($quoted -join " ")

  try {
    $proc = New-Object System.Diagnostics.Process
    $proc.StartInfo = $psi
    [void]$proc.Start()
  } catch {
    $job.StartError = $_.Exception.Message
    return $job
  }

  # Drain both pipes concurrently. Reading one to completion before the other
  # deadlocks as soon as the child fills the pipe we are not reading.
  $job.Process = $proc
  $job.StdOutTask = $proc.StandardOutput.ReadToEndAsync()
  $job.StdErrTask = $proc.StandardError.ReadToEndAsync()
  return $job
}

<#
.SYNOPSIS
  Waits for a job started by Start-NativeProcess and returns its output.

.PARAMETER Job
  Job object from Start-NativeProcess.

.PARAMETER TimeoutMs
  Bounded wait. The child is killed when it is exceeded, so a hung command can never
  freeze the bar.

.NOTES
  Returns ExitCode, Output (stdout), Error (stderr) and Text (stdout on success,
  stderr on failure, so callers get a usable diagnostic).
#>
function Complete-NativeProcess {
  [CmdletBinding()]
  param(
    [Parameter(Mandatory = $true)]$Job,
    [int]$TimeoutMs = 20000
  )

  if ($Job.StartError) {
    return [pscustomobject]@{
      ExitCode = -1
      Output   = ""
      Error    = $Job.StartError
      Text     = $Job.StartError
    }
  }

  $proc = $Job.Process
  if ($null -eq $proc) {
    return [pscustomobject]@{ ExitCode = -1; Output = ""; Error = "no process"; Text = "no process" }
  }

  $timedOut = -not $proc.WaitForExit($TimeoutMs)
  if ($timedOut) {
    try { $proc.Kill() } catch { }
    [void]$proc.WaitForExit(3000)
  }

  $stdout = ""
  $stderr = ""
  try { if ($Job.StdOutTask.Wait(3000)) { $stdout = $Job.StdOutTask.Result } } catch { }
  try { if ($Job.StdErrTask.Wait(3000)) { $stderr = $Job.StdErrTask.Result } } catch { }

  $code = -1
  try { $code = $proc.ExitCode } catch { }
  try { $proc.Dispose() } catch { }

  if ($timedOut) {
    $msg = "timed out after $TimeoutMs ms"
    if ($stderr) { $msg = $msg + " - " + $stderr }
    return [pscustomobject]@{ ExitCode = -1; Output = $stdout; Error = $msg; Text = $msg }
  }

  $text = $stdout
  if ($code -ne 0) { $text = $stderr }
  return [pscustomobject]@{ ExitCode = $code; Output = $stdout; Error = $stderr; Text = $text }
}

<#
.SYNOPSIS
  Runs an external command without allocating a console window and returns its output.

.PARAMETER FilePath
  Executable path. Bare names such as "netstat" or "sqlite3" resolve through PATH.

.PARAMETER Arguments
  Argument list, passed as an array. Quoted internally.

.PARAMETER TimeoutMs
  Bounded wait. The child is killed when it is exceeded.
#>
function Invoke-NativeText {
  [CmdletBinding()]
  param(
    [Parameter(Mandatory = $true)][string]$FilePath,
    [string[]]$Arguments = @(),
    [int]$TimeoutMs = 20000
  )

  $job = Start-NativeProcess -FilePath $FilePath -Arguments $Arguments
  return (Complete-NativeProcess -Job $job -TimeoutMs $TimeoutMs)
}

# Tells a caller that dot-sourced this file that the helpers are available, so a
# library that needs them can skip a second load. Dot-sourcing re-parses the file,
# which is real time on a script that runs on the UI thread.
$script:InvokeNativeLoaded = $true
