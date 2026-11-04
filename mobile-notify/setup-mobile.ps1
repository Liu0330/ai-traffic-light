$ErrorActionPreference = 'Stop'

Write-Host 'AI Traffic Light - Mobile Notifications'
Write-Host '1. ntfy (iPhone + Android, recommended)'
Write-Host '2. Bark (iPhone)'
Write-Host '3. Generic webhook'
Write-Host '4. Disable mobile notifications'
$choice = Read-Host 'Choose 1-4'

$enabled = $true
$provider = ''
$url = ''

switch ($choice) {
    '1' {
        $provider = 'ntfy'
        $topic = Read-Host 'Enter a private ntfy topic, or press Enter to generate one'
        if (-not $topic) {
            $topic = 'ai-light-' + [guid]::NewGuid().ToString('N')
        }
        if ($topic -match '^https?://') {
            $url = $topic
        } else {
            $url = 'https://ntfy.sh/' + $topic
        }
        Write-Host "Subscribe to this topic in the ntfy phone app: $topic"
    }
    '2' {
        $provider = 'bark'
        $url = Read-Host 'Paste the Bark base URL, for example https://api.day.app/your-key'
    }
    '3' {
        $provider = 'generic'
        $url = Read-Host 'Enter the HTTPS webhook URL'
    }
    '4' {
        $enabled = $false
        $provider = 'disabled'
        $url = ''
    }
    default { throw 'Invalid choice.' }
}

$events = Read-Host 'Notify events [waiting,error]'
if (-not $events) { $events = 'waiting,error' }
$cooldownText = Read-Host 'Cooldown seconds [60]'
$cooldown = 60
if ($cooldownText) { $cooldown = [Math]::Max(5, [int]$cooldownText) }

$configDir = Join-Path $env:LOCALAPPDATA 'AITrafficLight'
$configPath = Join-Path $configDir 'notifications.json'
New-Item -ItemType Directory -Force -Path $configDir | Out-Null
@{
    enabled = $enabled
    provider = $provider
    url = $url
    events = $events
    cooldown_seconds = $cooldown
} | ConvertTo-Json | Set-Content -LiteralPath $configPath -Encoding UTF8

Write-Host "Saved: $configPath"
if ($enabled) {
    try {
        Invoke-RestMethod 'http://127.0.0.1:17321/notify/test' -Method Post -TimeoutSec 10 | Out-Null
        Write-Host 'Test notification sent. Check your phone.' -ForegroundColor Green
    } catch {
        Write-Host 'Configuration was saved, but the test failed. Make sure AI Traffic Light is running and the URL is correct.' -ForegroundColor Yellow
    }
}
