param([Parameter(Mandatory = $true)][string]$Binary)
$ErrorActionPreference = 'Stop'
$Root = Join-Path ([IO.Path]::GetTempPath()) ([IO.Path]::GetRandomFileName())
$OriginalLocalAppData = $env:LOCALAPPDATA
$OriginalPath = $env:Path
$OriginalUserPath = [Environment]::GetEnvironmentVariable('Path', 'User')
$Version = 'v2.0.0-dev.20260928000101'
$Archive = 'quazonai-cli-windows-x86_64.zip'
$Downloads = [Collections.Generic.List[string]]::new()
function Assert($Condition, $Message) { if (-not $Condition) { throw $Message } }
function Invoke-WebRequest {
    param([Parameter(Position = 0)][string]$Uri, [switch]$UseBasicParsing, [string]$OutFile)
    Assert ($Uri.StartsWith("https://github.com/zhengui666/QuaZonai/releases/download/$Version/")) 'Unexpected download URL.'
    $Downloads.Add($Uri)
    Copy-Item -LiteralPath (Join-Path $Assets ($Uri.Split('/')[-1])) -Destination $OutFile
}
try {
    $Assets = Join-Path $Root 'assets'
    $Source = Join-Path $Root 'source'
    [IO.Directory]::CreateDirectory($Assets) | Out-Null
    [IO.Directory]::CreateDirectory($Source) | Out-Null
    $env:LOCALAPPDATA = Join-Path $Root 'local app data'
    Copy-Item -LiteralPath $Binary -Destination (Join-Path $Source 'quazonai.exe')
    foreach ($Name in @('LICENSE', 'NOTICE', 'THIRD_PARTY_NOTICES.md')) {
        Set-Content -LiteralPath (Join-Path $Source $Name) -Value $Name
    }
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $ZipPath = Join-Path $Assets $Archive
    [IO.Compression.ZipFile]::CreateFromDirectory($Source, $ZipPath)
    $Checksum = (Get-FileHash -Algorithm SHA256 $ZipPath).Hash.ToLowerInvariant()
    Set-Content -LiteralPath (Join-Path $Assets 'SHA256SUMS') -Value "$Checksum  $Archive" -Encoding ascii
    $Installer = Join-Path $Root 'install.ps1'
    $Template = Get-Content -Raw -LiteralPath (Join-Path $PSScriptRoot 'install.ps1')
    Set-Content -LiteralPath $Installer -Value $Template.Replace('@QUAZONAI_VERSION@', $Version) -Encoding utf8
    & $Installer
    $Destination = Join-Path $env:LOCALAPPDATA 'QuaZonai/bin/quazonai.exe'
    $Expected = (Get-FileHash -Algorithm SHA256 $Binary).Hash
    Assert ((Get-FileHash -Algorithm SHA256 $Destination).Hash -eq $Expected) 'Installed binary differs from its release.'
    & $Installer # Exercise native atomic replacement of an existing executable.
    Assert ((Get-FileHash -Algorithm SHA256 $Destination).Hash -eq $Expected) 'Updated binary differs from its release.'
    $Bin = Split-Path $Destination
    Assert ($Bin -in ([Environment]::GetEnvironmentVariable('Path', 'User') -split ';')) 'The CLI directory is absent from the user PATH.'
    Assert (Test-Path -LiteralPath (Join-Path $env:LOCALAPPDATA "QuaZonai/licenses/$Version/LICENSE")) 'License is missing.'
    Set-Content -LiteralPath $ZipPath -Value 'corrupt download'
    $Rejected = $false
    try { & $Installer } catch { $Rejected = $_.Exception.Message -match 'Checksum mismatch' }
    Assert $Rejected 'A corrupt download was accepted.'
    Assert ((Get-FileHash -Algorithm SHA256 $Destination).Hash -eq $Expected) 'Failed update changed the installed CLI.'
    $Before = $Downloads.Count
    foreach ($Invalid in @('../main', 'v02.0.0', 'v2.0.0-dev.01', 'v2.0.0+metadata')) {
        $Rejected = $false
        try { & $Installer -Version $Invalid } catch { $Rejected = $true }
        Assert $Rejected 'Invalid release tag was accepted.'
    }
    Assert ($Downloads.Count -eq $Before) 'Invalid tags reached the download layer.'
    Assert ((Get-ChildItem -LiteralPath $Bin).Count -eq 1) 'Temporary executable was not removed.'
    Write-Host 'Windows prebuilt installer smoke passed: install, replacement, hash failure, version validation and PATH.'
} finally {
    $env:LOCALAPPDATA = $OriginalLocalAppData
    $env:Path = $OriginalPath
    [Environment]::SetEnvironmentVariable('Path', $OriginalUserPath, 'User')
    if (Test-Path -LiteralPath $Root) { Remove-Item -LiteralPath $Root -Recurse -Force }
}
