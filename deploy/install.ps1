# Published installers contain an exact release tag. Bundled Node needs no host npm.
[CmdletBinding()]
param([string]$Version = '@QUAZONAI_VERSION@', [string]$Directory = (Join-Path $env:LOCALAPPDATA 'QuaZonai'))
$ErrorActionPreference = 'Stop'
if ($env:OS -ne 'Windows_NT' -or -not [Environment]::Is64BitOperatingSystem -or $env:PROCESSOR_ARCHITECTURE -eq 'ARM64') {
    throw 'Supported host: Windows x86_64. Use install.sh on Linux or macOS.'
}
$Repository = 'https://api.github.com/repos/zhengui666/QuaZonai'
if ($Version -eq ('@QUAZONAI_' + 'VERSION@') -or $Version -eq 'latest') {
    $Releases = @(); $Page = 1
    do {
        $Items = @(Invoke-RestMethod "$Repository/releases?per_page=100&page=$Page")
        $Releases += $Items; $Page++
    } while ($Items.Count -eq 100)
    $Required = @('install.sh','install.ps1','release.json','quazonai-deploy.tar.gz','README.md','quazonai-cli-linux-x86_64.tar.gz','quazonai-cli-macos-x86_64.tar.gz','quazonai-cli-macos-aarch64.tar.gz','quazonai-cli-windows-x86_64.zip','quazonai-image-application.tar.gz','quazonai-image-runtime.tar.gz','quazonai-image-database.tar.gz')
    $Release = $Releases | Where-Object {
        $Names = @($_.assets | Where-Object { $_.state -eq 'uploaded' -and $_.size -gt 0 } | ForEach-Object name)
        -not $_.draft -and $_.tag_name -match '^v\d+\.\d+\.\d+-dev\.\d{14}\.\d+$' -and @($Required | Where-Object { $_ -notin $Names }).Count -eq 0
    } | Sort-Object tag_name -Descending | Select-Object -First 1
    if (-not $Release) { throw 'No complete dev release. Supply -Version TAG explicitly.' }
    $Version = $Release.tag_name
}
if ($Version -notmatch '^v\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$') { throw 'Expected an exact vMAJOR.MINOR.PATCH[-prerelease] tag.' }
$Base = "https://github.com/zhengui666/QuaZonai/releases/download/$Version"
$Work = Join-Path ([IO.Path]::GetTempPath()) ([IO.Path]::GetRandomFileName())
$Bin = Join-Path $Directory 'bin'
$ReleaseDirectory = Join-Path $Directory "releases/$Version"
[IO.Directory]::CreateDirectory($Work) | Out-Null
try {
    $ZipPath = Join-Path $Work 'cli.zip'
    Invoke-WebRequest -UseBasicParsing "$Base/quazonai-cli-windows-x86_64.zip" -OutFile $ZipPath
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $Zip = [IO.Compression.ZipFile]::OpenRead($ZipPath)
    $Candidate = Join-Path $Work 'distribution'
    [IO.Directory]::CreateDirectory($Candidate) | Out-Null
    try {
        foreach ($Entry in $Zip.Entries) {
            if ($Entry.FullName -match '(^[/\\]|(^|/)\.\.(/|$)|\\)' -or $Entry.FullName.EndsWith('/')) { throw 'Invalid CLI archive member.' }
            $Target = Join-Path $Candidate $Entry.FullName
            [IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($Target)) | Out-Null
            [IO.Compression.ZipFileExtensions]::ExtractToFile($Entry, $Target)
        }
    } finally { $Zip.Dispose() }
    foreach ($Name in @('runtime/node.exe','runtime/LICENSE','app/apps/cli/src/main.ts','quazonai.cmd','distribution.json','LICENSE','NOTICE','THIRD_PARTY_NOTICES.md')) {
        if (-not (Test-Path -LiteralPath (Join-Path $Candidate $Name) -PathType Leaf)) { throw "Incomplete JS/Node CLI: $Name" }
    }
    $Actual = & (Join-Path $Candidate 'quazonai.cmd') --version
    if ($LASTEXITCODE -ne 0 -or $Actual -ne "quazonai $Version") { throw 'Bundled Node CLI version does not match.' }
    [IO.Directory]::CreateDirectory((Split-Path $ReleaseDirectory)) | Out-Null
    if (-not (Test-Path -LiteralPath $ReleaseDirectory)) { Move-Item -LiteralPath $Candidate -Destination $ReleaseDirectory }
    $Installed = & (Join-Path $ReleaseDirectory 'quazonai.cmd') --version
    if ($LASTEXITCODE -ne 0 -or $Installed -ne "quazonai $Version") { throw 'Preserve incomplete installed distribution before retrying.' }
    [IO.Directory]::CreateDirectory($Bin) | Out-Null
    # An old .exe would precede .cmd in PATHEXT. Retain it rather than delete it.
    $OldExe = Join-Path $Bin 'quazonai.exe'
    if (Test-Path -LiteralPath $OldExe) { Move-Item -LiteralPath $OldExe -Destination (Join-Path $Directory ('legacy-cli-' + [IO.Path]::GetRandomFileName() + '.exe')) }
    $Staged = Join-Path $Bin ([IO.Path]::GetRandomFileName())
    [IO.File]::WriteAllText($Staged, "@echo off`r`ncall `"$ReleaseDirectory\quazonai.cmd`" %*`r`nexit /b %errorlevel%`r`n")
    $Destination = Join-Path $Bin 'quazonai.cmd'
    if ([IO.File]::Exists($Destination)) { [IO.File]::Replace($Staged, $Destination, $null) } else { [IO.File]::Move($Staged, $Destination) }
    $UserPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    if ($Bin -notin ($UserPath -split ';')) { [Environment]::SetEnvironmentVariable('Path', "$Bin;$UserPath", 'User') }
    if ($Bin -notin ($env:Path -split ';')) { $env:Path = "$Bin;$env:Path" }
    Write-Host "Installed bundled Node CLI $Version at $Destination"
} finally { Remove-Item -LiteralPath $Work -Recurse -Force }
