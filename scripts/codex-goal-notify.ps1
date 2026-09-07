[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)] [string] $GoalId,
    [Parameter(Mandatory = $true)] [string] $RepoRoot,
    [int] $PollSeconds = 5,
    [int] $MaxPolls = 0,
    [switch] $TestMode,
    [string] $TerminalStateFile
)

$ErrorActionPreference = 'Stop'
$repo = [IO.Path]::GetFullPath($RepoRoot)
$stateRoot = Join-Path $repo '.git\lanternleaf-goal-state'
if ([string]::IsNullOrWhiteSpace($TerminalStateFile)) {
    $TerminalStateFile = Join-Path $stateRoot "$GoalId.terminal"
}
$ackFile = "$TerminalStateFile.ack"

function Get-TerminalState {
    $done = Test-Path (Join-Path $repo "docs\work\done\$GoalId.md")
    $blocked = Test-Path (Join-Path $repo "docs\work\blocked\$GoalId.md")
    if ($done -and $blocked) { throw "Goal $GoalId exists in both done and blocked." }
    if ($done) { return 'done' }
    if ($blocked) { return 'blocked' }
    if (Test-Path $TerminalStateFile) {
        $value = (Get-Content -LiteralPath $TerminalStateFile -Raw).Trim().ToLowerInvariant()
        if ($value -in @('done', 'blocked')) { return $value }
        if ($value) { Write-Warning "Ignoring malformed terminal state '$value'." }
    }
    return $null
}

function Write-TerminalAck([string] $state) {
    $parent = Split-Path -Parent $ackFile
    New-Item -ItemType Directory -Path $parent -Force | Out-Null
    if (-not (Test-Path $ackFile)) {
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
    try {
        Add-Type -AssemblyName System.Runtime.WindowsRuntime -ErrorAction SilentlyContinue
        $null = [Windows.UI.Notifications.ToastNotificationManager, Windows.UI.Notifications, ContentType = WindowsRuntime]
        $xml = New-Object Windows.Data.Xml.Dom.XmlDocument
        $xml.LoadXml("<toast><visual><binding template='ToastGeneric'><text>$title</text><text>$body</text></binding></visual><audio src='ms-winsoundevent:Notification.Default'/></toast>")
        $toast = [Windows.UI.Notifications.ToastNotification]::new($xml)
        [Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier('LanternLeaf').Show($toast)
    } catch {
        Write-Warning "Windows notification delivery failed (non-fatal): $($_.Exception.Message)"
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
