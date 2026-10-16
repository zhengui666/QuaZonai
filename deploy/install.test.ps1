param([Parameter(Mandatory = $true)][string]$Archive, [Parameter(Mandatory = $true)][string]$Version)
$ErrorActionPreference = 'Stop'
$Root = Join-Path $env:RUNNER_TEMP ([IO.Path]::GetRandomFileName())
$OriginalPath = $env:Path
$OriginalUserPath = [Environment]::GetEnvironmentVariable('Path', 'User')
function Invoke-WebRequest {
    param([Parameter(Position = 0)][string]$Uri, [switch]$UseBasicParsing, [string]$OutFile)
    if ($Uri -ne "https://github.com/zhengui666/QuaZonai/releases/download/$Version/quazonai-cli-windows-x86_64.zip") { throw 'Unexpected download.' }
    Copy-Item -LiteralPath $Archive -Destination $OutFile
}
try {
    $Directory = Join-Path $Root 'installed cli'
    [IO.Directory]::CreateDirectory($Directory) | Out-Null
    $Config = Join-Path $Directory 'original-client.json'
    [IO.File]::WriteAllText($Config, 'original device connection')
    for ($Index = 0; $Index -lt 2; $Index++) {
        & (Join-Path $PSScriptRoot 'install.ps1') -Version $Version -Directory $Directory
        $Actual = & (Join-Path $Directory 'bin/quazonai.cmd') --version
        if ($LASTEXITCODE -ne 0 -or $Actual -ne "quazonai $Version") { throw 'Installed bundled Node CLI failed.' }
    }
    if ([IO.File]::ReadAllText($Config) -ne 'original device connection') { throw 'Client config was overwritten.' }
    if (-not (Test-Path -LiteralPath (Join-Path $Directory "releases/$Version/runtime/LICENSE"))) { throw 'Official Node license missing.' }
    Write-Host 'Bundled Node install/repeat and original config preservation passed.'
} finally {
    [Environment]::SetEnvironmentVariable('Path', $OriginalUserPath, 'User')
    $env:Path = $OriginalPath
    if (Test-Path -LiteralPath $Root) { Remove-Item -LiteralPath $Root -Recurse -Force }
}
