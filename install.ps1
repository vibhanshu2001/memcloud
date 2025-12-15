# MemCloud Windows Installer
$ErrorActionPreference = "Stop"

Write-Host "╔══════════════════════════════════════════════════════════════╗" -ForegroundColor Magenta
Write-Host "║                                                              ║" -ForegroundColor Magenta
Write-Host "║               ☁  M E M C L O U D   I N S T A L L E R  ⚡      ║" -ForegroundColor Magenta
Write-Host "║                                                              ║" -ForegroundColor Magenta
Write-Host "║     'Turning nearby devices into your personal RAM farm.'    ║" -ForegroundColor Magenta
Write-Host "║                                                              ║" -ForegroundColor Magenta
Write-Host "╚══════════════════════════════════════════════════════════════╝" -ForegroundColor Magenta
Write-Host ""

function Log-Info { param([string]$msg); Write-Host "➤ $msg" -ForegroundColor Cyan }
function Log-Success { param([string]$msg); Write-Host "✔ $msg" -ForegroundColor Green }
function Log-Error { param([string]$msg); Write-Host "✖ $msg" -ForegroundColor Red }

Log-Info "Initializing MemCloud deployment sequence..."

# 1. Fetch Latest Version
Log-Info "Fetching latest release version..."
try {
    $release = Invoke-RestMethod -Uri "https://api.github.com/repos/vibhanshu2001/memcloud/releases/latest"
    $version = $release.tag_name
    Log-Info "Detected latest version: $version"
} catch {
    Log-Error "Failed to fetch latest version. Using fallback v0.1.0"
    $version = "v0.1.0"
}

# 2. Determine Paths
$installDir = "$env:USERPROFILE\.memcloud\bin"
if (!(Test-Path -Path $installDir)) {
    New-Item -ItemType Directory -Force -Path $installDir | Out-Null
}

$zipUrl = "https://github.com/vibhanshu2001/memcloud/releases/download/$version/memcloud-x86_64-pc-windows-msvc.zip"
$zipPath = "$env:TEMP\memcloud.zip"

# 3. Download
Log-Info "Downloading from $zipUrl..."
Invoke-WebRequest -Uri $zipUrl -OutFile $zipPath

# 4. Extract
Log-Info "Extracting to $installDir..."
Expand-Archive -Path $zipPath -DestinationPath $installDir -Force

# 5. Clean up
Remove-Item $zipPath -Force

# 6. Add to PATH if needed
$userPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($userPath -notlike "*$installDir*") {
    Log-Info "Adding $installDir to User PATH..."
    [Environment]::SetEnvironmentVariable("Path", "$userPath;$installDir", "User")
    Log-Success "Added to PATH (Restart terminal to take effect)"
} else {
    Log-Info "$installDir is already in your PATH."
}

Write-Host ""
Log-Success "MemCloud successfully installed! 🚀"
Write-Host ""
Write-Host "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
Write-Host "  ✨ You're Ready to Begin:" -ForegroundColor Green
Write-Host "    Start daemon:   memcli node start --name 'MyDevice'" -ForegroundColor Cyan
Write-Host "    Check status:   memcli node status" -ForegroundColor Cyan
Write-Host "    Stop daemon:    memcli node stop" -ForegroundColor Cyan
Write-Host ""
Write-Host "  Restart your terminal if 'memcli' is not found." -ForegroundColor Yellow
Write-Host "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
