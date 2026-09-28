<#
.SYNOPSIS
  Windowless invocation of external commands, for PowerShell 5.1 on Windows.

.DESCRIPTION
  Invokes an external program through System.Diagnostics.Process with
  UseShellExecute = $false and CreateNoWindow = $true, so the child never gets a
  console window of its own. Launching a console child with the `&` call operator
  (or Start-Process without -WindowStyle Hidden) can allocate a new console
  (conhost.exe / OpenConsole.exe), which shows up as a flashing terminal window
  when the parent runs inside a modal popup.

  This file must stay compatible with Windows PowerShell 5.1 (.NET Framework):
  ProcessStartInfo.ArgumentList does not exist there, so the argument string is
  built here following the CommandLineToArgvW quoting rules.

  Dot-source it, do not run it:

    . (Join-Path $PSScriptRoot "lib\Invoke-Native.ps1")
#>

# Quotes a single argument following the CRT/CommandLineToArgvW rules:
# a quote is escaped as \", and backslashes are doubled when they precede a
# quote or end the argument.
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
  Executable path. Bare names such as "netstat" or "herdr" resolve through PATH.

.PARAMETER Arguments
  Argument list, passed as an array. Quoted internally.

.NOTES
  Split from Complete-NativeProcess so two independent calls can be in flight at
  the same time: start both, then complete both. The popup needs a port check and
  a database read, and paying for them one after the other is pure latency.

  Returns a job object. A job with a non-empty StartError means the child never
  ran; pass it to Complete-NativeProcess to get a normal failure result.
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
  Bounded wait. The child is killed when it is exceeded, so a hung command can
  never freeze the caller.

.NOTES
  Returns an object with ExitCode, Output (stdout), Error (stderr) and Text
  (stdout on success, stderr on failure, so callers get a usable diagnostic).
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
  Executable path. Bare names such as "netstat" or "herdr" resolve through PATH.

.PARAMETER Arguments
  Argument list, passed as an array. Quoted internally.

.PARAMETER TimeoutMs
  Bounded wait. The child is killed when it is exceeded, so a hung command can
  never freeze the caller.
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
# library that needs them can skip a second load. Dot-sourcing a file re-parses it,
# which is real time in a script that has a sub-second budget.
$script:InvokeNativeLoaded = $true
