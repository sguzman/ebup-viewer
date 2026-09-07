[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$watcherPath = Join-Path $PSScriptRoot 'codex-goal-notify.ps1'
$signalPath = Join-Path $PSScriptRoot 'signal-codex-goal-terminal.ps1'
$root = Join-Path ([IO.Path]::GetTempPath()) "lanternleaf-goal-notify-$([Guid]::NewGuid())"
New-Item -ItemType Directory -Path (Join-Path $root 'docs\work\done'), (Join-Path $root 'docs\work\blocked'), (Join-Path $root '.git\lanternleaf-goal-state') -Force | Out-Null

function Invoke-WatcherProbe(
    [string] $goalId,
    [string] $stateFile,
    [string] $expected,
    [int] $expectedExit = 0,
    [switch] $AllowRepoStateFallback,
    [switch] $Rearm
) {
    $args = @(
        '-NoProfile',
        '-File', $watcherPath,
        '-GoalId', $goalId,
        '-RepoRoot', $root,
        '-TerminalStateFile', $stateFile,
        '-PollSeconds', '1',
        '-MaxPolls', '1',
        '-TestMode'
    )
    if ($AllowRepoStateFallback) {
        $args += '-AllowRepoStateFallback'
    }
    if ($Rearm) {
        $args += '-Rearm'
    }

    $output = & pwsh @args 2>&1
    if ($LASTEXITCODE -ne $expectedExit) {
        throw "Expected exit $expectedExit, got ${LASTEXITCODE}: $($output -join ' ')"
    }
    if ($expected -and -not (($output -join "`n") -match [regex]::Escape($expected))) {
        throw "Expected '$expected': $($output -join ' ')"
    }
}

$sentinel = Join-Path $root '.git\lanternleaf-goal-state\0005.terminal'
Invoke-WatcherProbe '0005' $sentinel 'WATCH_TIMEOUT' 2

# Real goal filenames are slugged. Fallback detection must understand that shape.
$doneGoal = Join-Path $root 'docs\work\done\0005-tts-correctness-and-goal-notify.md'
Set-Content -LiteralPath $doneGoal -Value '# done'
Invoke-WatcherProbe '0005' $sentinel 'TEST_NOTIFICATION|done' 0 -AllowRepoStateFallback
Invoke-WatcherProbe '0005' $sentinel 'ALREADY_ACKNOWLEDGED|done' 0 -AllowRepoStateFallback
Remove-Item -LiteralPath $doneGoal -Force

# Sentinel is the normal checkout-safe trigger.
$blockedSentinel = Join-Path $root '.git\lanternleaf-goal-state\0006.terminal'
Set-Content -LiteralPath $blockedSentinel -Value 'blocked'
Invoke-WatcherProbe '0006' $blockedSentinel 'TEST_NOTIFICATION|blocked'

$malformed = Join-Path $root '.git\lanternleaf-goal-state\0007.terminal'
Set-Content -LiteralPath $malformed -Value 'not-terminal'
Invoke-WatcherProbe '0007' $malformed 'WATCH_TIMEOUT' 2

# Signal helper writes the terminal sentinel after push. Missing watcher/ack is non-fatal.
$signalOutput = & pwsh -NoProfile -File $signalPath -GoalId 0008 -RepoRoot $root -State done -AckWaitSeconds 0 2>&1
if ($LASTEXITCODE -ne 0) {
    throw "Terminal signal helper must be non-fatal without an ack: $($signalOutput -join ' ')"
}
if (-not (($signalOutput -join "`n") -match 'GOAL_TERMINAL_SIGNALLED\|0008\|done')) {
    throw "Terminal signal helper did not record done state: $($signalOutput -join ' ')"
}
$signalSentinel = Join-Path $root '.git\lanternleaf-goal-state\0008.terminal'
if ((Get-Content -LiteralPath $signalSentinel -Raw).Trim() -ne 'done') {
    throw 'Terminal signal helper wrote the wrong state.'
}

# A director correction may reuse the same repository goal ID in a fresh Codex Goal session.
# Rearm must clear the prior attempt's terminal + ack so stale state cannot immediately retrigger.
$rearmSentinel = Join-Path $root '.git\lanternleaf-goal-state\0009.terminal'
$rearmAck = "$rearmSentinel.ack"
Set-Content -LiteralPath $rearmSentinel -Value 'done'
Set-Content -LiteralPath $rearmAck -Value "done`nprior-attempt"
Invoke-WatcherProbe '0009' $rearmSentinel 'WATCH_TIMEOUT' 2 -Rearm
if ((Test-Path -LiteralPath $rearmSentinel) -or (Test-Path -LiteralPath $rearmAck)) {
    throw 'Rearm must clear stale terminal/ack state from the prior attempt.'
}
Set-Content -LiteralPath $rearmSentinel -Value 'blocked'
Invoke-WatcherProbe '0009' $rearmSentinel 'TEST_NOTIFICATION|blocked'

Write-Output 'codex-goal-notify tests passed'
Remove-Item -LiteralPath $root -Recurse -Force
