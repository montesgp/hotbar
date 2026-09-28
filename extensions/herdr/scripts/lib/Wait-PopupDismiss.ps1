<#
.SYNOPSIS
    Bounded wait for the popup dismiss keys.

.DESCRIPTION
    A Herdr popup is a session modal: it receives every byte the terminal sends,
    including the ambient Escape bytes an agent emits between commands. A popup
    that closes on any key therefore disappears on its own.

    This module holds the only wait loop the status popup uses. It ends the wait
    on exactly two keys, `q` (or `Q`) and Enter, and consumes everything else
    without repainting. The wait is always bounded so a popup can never outlive
    its cap, no matter what the keypress report does.

    It lives in a dot-sourceable file rather than inside the dashboard so the
    dismiss contract can be exercised directly in tests.
#>

function Test-DismissKey($key) {
    # Enter and q/Q are the only keys that end the wait. Matching on KeyChar as
    # well as Key covers layouts and hosts where the shift state is not reported.
    if ($null -eq $key) { return $false }
    if ($key.Key -eq [ConsoleKey]::Enter) { return $true }
    if ($key.Key -eq [ConsoleKey]::Q) { return $true }
    if ($key.KeyChar -eq 'q' -or $key.KeyChar -eq 'Q') { return $true }
    return $false
}

function Wait-ForDismiss {
    <#
    .SYNOPSIS
        Waits up to $TimeoutMs for a dismiss key, rendering nothing while it waits.
    .PARAMETER TimeoutMs
        Hard upper bound for the wait, in milliseconds.
    .PARAMETER KeyWatch
        When $false the call degrades to a plain bounded sleep. Used by -NoKeyWatch
        and by hosts that cannot report keypresses.
    .OUTPUTS
        $true when a dismiss key was pressed, $false on timeout.
    #>
    param([int]$TimeoutMs, [bool]$KeyWatch)

    $waitMs = [Math]::Max(0, $TimeoutMs)
    if (-not $KeyWatch) { Start-Sleep -Milliseconds $waitMs; return $false }

    $end = [DateTime]::UtcNow.AddMilliseconds($waitMs)
    $watchable = $true
    while ([DateTime]::UtcNow -lt $end) {
        if ($watchable) {
            try {
                if ([Console]::KeyAvailable) {
                    $key = $null
                    try { $key = [Console]::ReadKey($true) } catch { }
                    if ($null -ne $key -and (Test-DismissKey $key)) { return $true }
                }
            } catch {
                # No usable key report (redirected input, no console). Fall back to
                # the same bounded sleep instead of failing, so the cap still holds.
                $watchable = $false
            }
        }
        Start-Sleep -Milliseconds 50
    }
    return $false
}
