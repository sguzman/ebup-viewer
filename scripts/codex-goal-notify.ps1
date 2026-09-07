[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)] [string] $GoalId,
    [Parameter(Mandatory = $true)] [string] $RepoRoot,
    [int] $PollSeconds = 5,
    [int] $MaxPolls = 0,
    [switch] $TestMode,
    [string] $TerminalStateFile,
    [switch] $AllowRepoStateFallback,
    [switch] $Rearm
)

$ErrorActionPreference = 'Stop'
$repo = [IO.Path]::GetFullPath($RepoRoot)
$stateRoot = Join-Path $repo '.git\lanternleaf-goal-state'
if ([string]::IsNullOrWhiteSpace($TerminalStateFile)) {
    $TerminalStateFile = Join-Path $stateRoot "$GoalId.terminal"
}
$ackFile = "$TerminalStateFile.ack"

if ($Rearm) {
    $stateParent = Split-Path -Parent $TerminalStateFile
    New-Item -ItemType Directory -Path $stateParent -Force | Out-Null
    foreach ($stalePath in @($TerminalStateFile, $ackFile)) {
        if (Test-Path -LiteralPath $stalePath) {
            Remove-Item -LiteralPath $stalePath -Force
        }
    }
    Write-Output "WATCH_REARMED|$GoalId"
}

function Get-RepoTerminalState {
    $doneDir = Join-Path $repo 'docs\work\done'
    $blockedDir = Join-Path $repo 'docs\work\blocked'

    $doneMatches = @(
        Get-ChildItem -LiteralPath $doneDir -Filter "$GoalId*.md" -File -ErrorAction SilentlyContinue |
            Where-Object { $_.BaseName -eq $GoalId -or $_.BaseName.StartsWith("$GoalId-") }
    )
    $blockedMatches = @(
        Get-ChildItem -LiteralPath $blockedDir -Filter "$GoalId*.md" -File -ErrorAction SilentlyContinue |
            Where-Object { $_.BaseName -eq $GoalId -or $_.BaseName.StartsWith("$GoalId-") }
    )

    if ($doneMatches.Count -gt 0 -and $blockedMatches.Count -gt 0) {
        throw "Goal $GoalId exists in both done and blocked."
    }
    if ($doneMatches.Count -gt 0) { return 'done' }
    if ($blockedMatches.Count -gt 0) { return 'blocked' }
    return $null
}

function Get-TerminalState {
    if (Test-Path -LiteralPath $TerminalStateFile) {
        $value = (Get-Content -LiteralPath $TerminalStateFile -Raw).Trim().ToLowerInvariant()
        if ($value -in @('done', 'blocked')) { return $value }
        if ($value) { Write-Warning "Ignoring malformed terminal state '$value'." }
    }

    if ($AllowRepoStateFallback) {
        return Get-RepoTerminalState
    }

    return $null
}

function Write-TerminalAck([string] $state) {
    $parent = Split-Path -Parent $ackFile
    New-Item -ItemType Directory -Path $parent -Force | Out-Null
    if (-not (Test-Path -LiteralPath $ackFile)) {
        Set-Content -LiteralPath $ackFile -Value "$state`n$([DateTime]::UtcNow.ToString('o'))" -Encoding utf8
        return $true
    }
    $existing = (Get-Content -LiteralPath $ackFile -TotalCount 1).Trim().ToLowerInvariant()
    if ($existing -eq $state) { return $false }
    throw "Terminal acknowledgment already records '$existing', cannot change it to '$state'."
}

function Send-TerminalNotification([string] $state) {
    $label = if ($state -eq 'done') { 'Completed' } else { 'Blocked' }
    $title = "LanternLeaf goal $GoalId $label"
    $body = "LanternLeaf macro-goal ${GoalId}: $label."

    if ($TestMode) {
        Write-Output "TEST_NOTIFICATION|$state|$title|$body"
        return
    }

    $toastAttempted = $false
    try {
        Add-Type -AssemblyName System.Runtime.WindowsRuntime -ErrorAction SilentlyContinue
        $null = [Windows.UI.Notifications.ToastNotificationManager, Windows.UI.Notifications, ContentType = WindowsRuntime]
        $xml = New-Object Windows.Data.Xml.Dom.XmlDocument
        $escapedTitle = [Security.SecurityElement]::Escape($title)
        $escapedBody = [Security.SecurityElement]::Escape($body)
        $xml.LoadXml("<toast><visual><binding template='ToastGeneric'><text>$escapedTitle</text><text>$escapedBody</text></binding></visual><audio src='ms-winsoundevent:Notification.Default'/></toast>")
        $toast = [Windows.UI.Notifications.ToastNotification]::new($xml)
        [Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier('LanternLeaf').Show($toast)
        $toastAttempted = $true
    } catch {
        Write-Warning "Windows toast delivery failed (non-fatal): $($_.Exception.Message)"
    }

    if (-not $toastAttempted) {
        try {
            Add-Type -AssemblyName System.Windows.Forms
            Add-Type -AssemblyName System.Drawing
            $notify = New-Object System.Windows.Forms.NotifyIcon
            $notify.Icon = if ($state -eq 'done') {
                [System.Drawing.SystemIcons]::Information
            } else {
                [System.Drawing.SystemIcons]::Warning
            }
            $notify.BalloonTipTitle = $title
            $notify.BalloonTipText = $body
            $notify.Visible = $true
            $notify.ShowBalloonTip(5000)
            if ($state -eq 'done') {
                [System.Media.SystemSounds]::Asterisk.Play()
            } else {
                [System.Media.SystemSounds]::Exclamation.Play()
            }
            Start-Sleep -Seconds 6
            $notify.Dispose()
        } catch {
            Write-Warning "Windows notification-area fallback failed (non-fatal): $($_.Exception.Message)"
        }
    }
}

$poll = 0
while ($true) {
    $state = Get-TerminalState
    if ($state) {
        if (Write-TerminalAck $state) {
            Send-TerminalNotification $state
            exit 0
        }
        Write-Output "ALREADY_ACKNOWLEDGED|$state"
        exit 0
    }

    if ($MaxPolls -gt 0 -and $poll -ge $MaxPolls) {
        Write-Output "WATCH_TIMEOUT|$GoalId"
        exit 2
    }

    $poll++
    Start-Sleep -Seconds ([Math]::Max(1, $PollSeconds))
}
