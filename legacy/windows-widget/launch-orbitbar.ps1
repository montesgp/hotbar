#Requires -Version 5.1
<#
.SYNOPSIS
  Starts the orbitbar widget in its own hidden STA process.

.DESCRIPTION
  WPF needs a single-threaded apartment, and a hidden window should not drag a
  console behind it. Both of those are host-level concerns, so they live here
  and nowhere else: this script is the only entry point a human runs, and
  orbitbar.ps1 only ever has to assume it is already on a pumped STA thread.

  A second instance is refused, not stacked. The authoritative guard is the
  mutex inside orbitbar.ps1, which is the only place that lives for the whole
  life of the widget. The probe below is a courtesy: it saves spawning a
  process that would only print ORBITBAR_ALREADY_RUNNING and exit, so the user
  gets a readable message instead of a silent no-op. If two launchers race
  between the probe and the child taking the mutex, the child still wins the
  argument and the loser exits 3.

.PARAMETER SelfTest
  Run the in-widget self test in the foreground and pass its exit code through.

.PARAMETER SelfTestMs
  How long the self test shows the window before closing it.

.PARAMETER Wait
  Stay in the foreground until the widget exits. Useful when debugging; the
  default returns as soon as the widget process has started.

.EXAMPLE
  .\launch-orbitbar.ps1
  Starts the widget, hidden, and returns immediately.

.EXAMPLE
  .\launch-orbitbar.ps1 -SelfTest
  Runs the self test here and now and returns its exit code.
#>
[CmdletBinding()]
param(
  [switch]$SelfTest,
  [int]$SelfTestMs = 400,
  [switch]$Wait
)

$ErrorActionPreference = "Stop"

# The name is duplicated from orbitbar.ps1 on purpose. Reading it from a file at
# runtime would mean parsing a script to start a script, and a two-line constant
# duplicated in a launcher is cheaper to keep honest than that is.
$script:OrbitbarMutexName = "Local\orbitbar.widget.v1"

$bar = Join-Path $PSScriptRoot "orbitbar.ps1"
if (-not (Test-Path -LiteralPath $bar)) {
  Write-Output ("orbitbar: cannot find " + $bar)
  exit 2
}

# System32 explicitly: a 32-bit launcher must not hand a 32-bit host to a WPF
# widget, and the PATH-resolved powershell.exe may be either one.
$shell = Join-Path $env:SystemRoot "System32\WindowsPowerShell\v1.0\powershell.exe"
if (-not (Test-Path -LiteralPath $shell)) { $shell = "powershell.exe" }

# The self test has to be observable, so it runs in this console, in the
# foreground, and its output and exit code are the whole point of running it.
if ($SelfTest) {
  $selfArgs = @(
    "-NoProfile", "-STA", "-ExecutionPolicy", "Bypass",
    "-File", ('"{0}"' -f $bar),
    "-SelfTest", "-SelfTestMs", $SelfTestMs
  )
  & $shell @selfArgs
  exit $LASTEXITCODE
}

# Best-effort "is one already up" probe. Mutex(initiallyOwned: true) hands us
# ownership only when we are the creator; if the name already exists, createdNew
# is false and we never owned it, so there is nothing to release.
$createdNew = $false
$probe = New-Object System.Threading.Mutex($true, $script:OrbitbarMutexName, [ref]$createdNew)
try {
  if (-not $createdNew) {
    Write-Output "orbitbar: already running"
    exit 0
  }
  $probe.ReleaseMutex()
} finally {
  $probe.Dispose()
}

$barArgs = @(
  "-NoProfile", "-STA", "-WindowStyle", "Hidden", "-ExecutionPolicy", "Bypass",
  "-File", ('"{0}"' -f $bar)
)

$process = Start-Process -FilePath $shell -ArgumentList $barArgs -WindowStyle Hidden -PassThru
if ($null -eq $process) {
  Write-Output "orbitbar: failed to start the widget process"
  exit 1
}

if ($Wait) {
  $process.WaitForExit()
  exit $process.ExitCode
}

Write-Output ("orbitbar: started (pid " + $process.Id + ")")
exit 0
