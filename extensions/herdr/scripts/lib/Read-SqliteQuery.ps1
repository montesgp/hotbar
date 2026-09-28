<#
.SYNOPSIS
  Read-only SQLite query with no console window and no runtime dependency.

.DESCRIPTION
  Runs one SELECT and returns its rows as string arrays. Two providers, in
  preference order:

    1. sqlite3 (default)
       An `sqlite3.exe` binary invoked through lib\Invoke-Native.ps1, opened with
       `-readonly` and a bounded `.timeout`. Proven windowless: the same helper
       launching `netstat` creates zero console windows.

    2. winsqlite
       A P/Invoke binding to Windows' own `winsqlite3.dll` (3.51.1 on this machine),
       compiled on demand with `Add-Type`. Used when no `sqlite3.exe` is on PATH, or
       when the sqlite3 binary fails the read. The handle is opened with
       SQLITE_OPEN_READONLY, a short busy timeout is set, and nothing is ever written,
       so the gateway's WAL database is never modified.

  The sqlite3 path is preferred not only because it is the documented first choice: it
  is also the faster of the two here. Measured in a cold `powershell -NoProfile`
  process, the first spawn costs ~160 ms (of which ~120 ms is initialising
  System.Diagnostics.Process at all), while the winsqlite path costs ~310 ms because
  Add-Type has to run the CodeDom C# 5 compiler.

  Both providers resolve json_extract(), so the statement needs no portable variant.
  A read that fails is reported as a failure, never as an empty result set.

  Columns come back as UTF-8 text, which is what both providers store.

  Dot-source it, do not run it:

    . (Join-Path $PSScriptRoot "lib\Read-SqliteQuery.ps1")
#>

# The C# type is compiled once per PowerShell process. Re-dot-sourcing the file
# must not try to add it again, hence the guard.
$script:ReadOnlySqliteTypeName = "HerdrOmniRoute.ReadOnlySqlite"

$script:ReadOnlySqliteSource = @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;

namespace HerdrOmniRoute
{
    // Minimal read-only binding over winsqlite3.dll. C# 5 syntax on purpose:
    // Windows PowerShell 5.1 compiles Add-Type with the CodeDom C# 5 compiler.
    public static class ReadOnlySqlite
    {
        const string Dll = "winsqlite3.dll";
        const int SqliteOk = 0;
        const int SqliteRow = 100;
        const int SqliteOpenReadonly = 0x00000001;

        [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
        static extern int sqlite3_open_v2(byte[] filename, out IntPtr db, int flags, IntPtr vfs);

        [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
        static extern int sqlite3_close(IntPtr db);

        [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
        static extern int sqlite3_busy_timeout(IntPtr db, int ms);

        [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
        static extern int sqlite3_prepare_v2(IntPtr db, byte[] sql, int nBytes, out IntPtr stmt, out IntPtr tail);

        [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
        static extern int sqlite3_step(IntPtr stmt);

        [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
        static extern int sqlite3_finalize(IntPtr stmt);

        [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
        static extern int sqlite3_column_count(IntPtr stmt);

        [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
        static extern int sqlite3_column_type(IntPtr stmt, int col);

        [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
        static extern IntPtr sqlite3_column_text(IntPtr stmt, int col);

        [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
        static extern IntPtr sqlite3_errmsg(IntPtr db);

        // sqlite3 takes UTF-8, never UTF-16, so every string crosses the boundary
        // as a NUL-terminated UTF-8 buffer.
        static byte[] ToUtf8(string value)
        {
            byte[] raw = Encoding.UTF8.GetBytes(value);
            byte[] buffer = new byte[raw.Length + 1];
            Buffer.BlockCopy(raw, 0, buffer, 0, raw.Length);
            buffer[raw.Length] = 0;
            return buffer;
        }

        static string ReadUtf8(IntPtr pointer)
        {
            if (pointer == IntPtr.Zero) return null;
            int length = 0;
            while (Marshal.ReadByte(pointer, length) != 0) length++;
            byte[] buffer = new byte[length];
            Marshal.Copy(pointer, buffer, 0, length);
            return Encoding.UTF8.GetString(buffer);
        }

        // Returns the rows, or null with `error` set. A SQL NULL column comes back
        // as an empty string, so PowerShell never has to test two shapes.
        public static string[][] Query(string databasePath, string sql, int busyTimeoutMs, out string error)
        {
            error = null;
            IntPtr db = IntPtr.Zero;
            IntPtr stmt = IntPtr.Zero;
            try
            {
                int rc = sqlite3_open_v2(ToUtf8(databasePath), out db, SqliteOpenReadonly, IntPtr.Zero);
                if (rc != SqliteOk)
                {
                    error = "sqlite3_open_v2 rc=" + rc + (db == IntPtr.Zero ? "" : " " + ReadUtf8(sqlite3_errmsg(db)));
                    if (db != IntPtr.Zero) { sqlite3_close(db); db = IntPtr.Zero; }
                    return null;
                }

                if (busyTimeoutMs > 0) sqlite3_busy_timeout(db, busyTimeoutMs);

                byte[] statement = ToUtf8(sql);
                IntPtr tail = IntPtr.Zero;
                rc = sqlite3_prepare_v2(db, statement, statement.Length, out stmt, out tail);
                if (rc != SqliteOk)
                {
                    error = "sqlite3_prepare_v2 rc=" + rc + " " + ReadUtf8(sqlite3_errmsg(db));
                    return null;
                }

                int columns = sqlite3_column_count(stmt);
                List<string[]> rows = new List<string[]>();
                while ((rc = sqlite3_step(stmt)) == SqliteRow)
                {
                    string[] row = new string[columns];
                    for (int i = 0; i < columns; i++)
                    {
                        row[i] = sqlite3_column_type(stmt, i) == 5 /* NULL */
                            ? ""
                            : (ReadUtf8(sqlite3_column_text(stmt, i)) ?? "");
                    }
                    rows.Add(row);
                }
                if (rc != 101 /* SQLITE_DONE */)
                {
                    error = "sqlite3_step rc=" + rc + " " + ReadUtf8(sqlite3_errmsg(db));
                    return null;
                }
                return rows.ToArray();
            }
            catch (Exception ex)
            {
                error = ex.Message;
                return null;
            }
            finally
            {
                if (stmt != IntPtr.Zero) sqlite3_finalize(stmt);
                if (db != IntPtr.Zero) sqlite3_close(db);
            }
        }
    }
}
'@

# Compiles the binding once per process. Returns $null on success, or the reason
# it could not be compiled.
function Initialize-WinSqlite {
  if ($null -ne ("HerdrOmniRoute.ReadOnlySqlite" -as [type])) { return $null }

  try {
    Add-Type -TypeDefinition $script:ReadOnlySqliteSource -ErrorAction Stop | Out-Null
  } catch {
    return $_.Exception.Message
  }
  return $null
}

# Row separator for the sqlite3 provider. The ASCII unit separator cannot appear in
# combo names in practice, and the caller's SQL strips it defensively anyway, so a
# value can never fake a column break.
$script:SqliteFieldSeparator = [string][char]31

# Resolves the sqlite3 binary, or $null when there is none on PATH.
# The PATH is scanned by hand instead of with Get-Command: the popup is on a sub-second
# budget and Get-Command's first call in a process costs ~60 ms warming up the command
# discovery machinery, which is a sixth of the whole budget for a lookup of one file.
function Get-SqliteExecutable {
  $exe = "sqlite3.exe"
  foreach ($dir in $env:PATH.Split(";")) {
    if (-not $dir) { continue }
    try {
      if ([System.IO.File]::Exists([System.IO.Path]::Combine($dir, $exe))) {
        return [System.IO.Path]::Combine($dir, $exe)
      }
    } catch {
      # Unreadable PATH entry: keep scanning, the next one may hold the binary.
    }
  }
  return $null
}

<#
.SYNOPSIS
  Runs one read-only SELECT and returns the rows.

.PARAMETER DatabasePath
  Absolute path to the SQLite file. It must already exist; this never creates one.

.PARAMETER Sql
  A single SELECT statement. It is never parameterized, so it must be a literal
  built by the caller, never user input.

.PARAMETER BusyTimeoutMs
  Busy timeout handed to SQLite. Bounded on purpose: a locked database must fail
  fast so the caller can show "data unavailable" instead of hanging the popup.

.PARAMETER Provider
    "auto" (default) tries sqlite3 and falls back to winsqlite. "sqlite3" and
    "winsqlite" force one of them, so a verification run can exercise each path
    independently.

.NOTES
  Returns an object with Ok, Rows (a string[][]), Provider ("sqlite3" or
  "winsqlite") and Error. Ok is $false whenever the database is missing, locked or
  unreadable; nothing in here throws at the caller.
#>
function Invoke-SqliteQuery {
  [CmdletBinding()]
  param(
    [Parameter(Mandatory = $true)][string]$DatabasePath,
    [Parameter(Mandatory = $true)][string]$Sql,
    [int]$BusyTimeoutMs = 1500,
    [ValidateSet("auto", "sqlite3", "winsqlite")][string]$Provider = "auto"
  )

  $result = [pscustomobject]@{
    Ok       = $false
    Rows     = @()
    Provider = ""
    Error    = ""
  }

  # [System.IO.File]::Exists rather than Test-Path: the first Test-Path in a process
  # costs ~70 ms of PowerShell provider initialisation, and this popup has a budget
  # measured in tens of milliseconds.
  if (-not [System.IO.File]::Exists($DatabasePath)) {
    $result.Error = "database not found: $DatabasePath"
    return $result
  }

  $wantsSqlite = ($Provider -eq "auto" -or $Provider -eq "sqlite3")
  $wantsWinSqlite = ($Provider -eq "auto" -or $Provider -eq "winsqlite")

  if ($wantsSqlite) {
    $exe = Get-SqliteExecutable
    if ($exe) {
      $out = Invoke-SqliteWithExecutable -DatabasePath $DatabasePath -Sql $Sql -Executable $exe -BusyTimeoutMs $BusyTimeoutMs
      if ($out.Ok) { return $out }
      $result.Error = $out.Error
      if ($Provider -eq "sqlite3") { return $result }
    } elseif ($Provider -eq "sqlite3") {
      $result.Error = "sqlite3.exe is not on PATH"
      return $result
    }
  }

  if (-not $wantsWinSqlite) { return $result }

  # The sqlite3 build was missing or rejected the statement. Windows ships its own
  # SQLite, so the same read is still possible in-process.
  $compileError = Initialize-WinSqlite
  if ($compileError) {
    if (-not $result.Error) { $result.Error = "winsqlite3.dll unavailable: $compileError" }
    return $result
  }

  $error = $null
  $rows = $null
  try {
    $rows = [HerdrOmniRoute.ReadOnlySqlite]::Query($DatabasePath, $Sql, $BusyTimeoutMs, [ref]$error)
  } catch {
    $rows = $null
    $error = $_.Exception.Message
  }

  if ($null -eq $rows) {
    if (-not $result.Error) { $result.Error = "winsqlite: $error" }
    return $result
  }

  $result.Ok = $true
  $result.Rows = $rows
  $result.Provider = "winsqlite"
  return $result
}

# sqlite3 provider. Kept private to this file: the provider choice belongs here.
function Invoke-SqliteWithExecutable {
  param(
    [string]$DatabasePath,
    [string]$Sql,
    [string]$Executable,
    [int]$BusyTimeoutMs
  )

  $result = [pscustomobject]@{
    Ok       = $false
    Rows     = @()
    Provider = "sqlite3"
    Error    = ""
  }

  # The windowless helper is dot-sourced by the caller in the popup path; loading it
  # again would re-parse the file for nothing. Only load it when used on its own.
  if (-not $script:InvokeNativeLoaded) {
    $lib = [System.IO.Path]::Combine($PSScriptRoot, "Invoke-Native.ps1")
    if (-not [System.IO.File]::Exists($lib)) {
      $result.Error = "windowless helper not found: $lib"
      return $result
    }
    . $lib
  }

  $run = Invoke-NativeText -FilePath $Executable -Arguments @(
    "-readonly",
    "-cmd", ".timeout $BusyTimeoutMs",
    "-separator", $script:SqliteFieldSeparator,
    $DatabasePath,
    $Sql
  ) -TimeoutMs ($BusyTimeoutMs + 3000)

  if ($run.ExitCode -ne 0) {
    $detail = (Get-FirstNonEmptyLine $run.Text)
    if (-not $detail) { $detail = "exit $($run.ExitCode)" }
    $result.Error = $detail
    return $result
  }

  $rows = @()
  foreach ($line in ($run.Output -split "`r?`n")) {
    if (-not $line.Trim()) { continue }
    $rows += , ($line -split [regex]::Escape($script:SqliteFieldSeparator))
  }

  $result.Ok = $true
  $result.Rows = $rows
  return $result
}

# First non-blank line of a diagnostic, so a multi-line stderr still shows up as one
# readable reason in the frame.
function Get-FirstNonEmptyLine([string]$text) {
  if (-not $text) { return "" }
  foreach ($line in ($text -split "`r?`n")) {
    $trimmed = $line.Trim()
    if ($trimmed) { return $trimmed }
  }
  return ""
}
