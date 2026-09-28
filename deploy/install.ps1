# Published with the exact release tag substituted by the release producer.
[CmdletBinding()]
param(
    [string]$Version = '@QUAZONAI_VERSION@'
)
$ErrorActionPreference = 'Stop'
if ($env:OS -ne 'Windows_NT' -or -not [Environment]::Is64BitOperatingSystem -or
    $env:PROCESSOR_ARCHITECTURE -eq 'ARM64' -or $env:PROCESSOR_ARCHITEW6432 -eq 'ARM64') {
    throw 'This installer supports Windows x86_64. Use install.sh on Linux or macOS.'
}
$Tag = [regex]::Match($Version, '^v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(?:-([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?$')
if ($Version.Length -gt 128 -or -not $Tag.Success) {
    throw 'Expected a published vMAJOR.MINOR.PATCH[-prerelease] tag.'
}
if ($Tag.Groups[4].Success) {
    foreach ($Identifier in $Tag.Groups[4].Value.Split('.')) {
        if ($Identifier -match '^0[0-9]+$') { throw 'Numeric prerelease identifiers cannot have leading zeroes.' }
    }
}
$Base = "https://github.com/zhengui666/QuaZonai/releases/download/$Version"
$Archive = 'quazonai-cli-windows-x86_64.zip'
$Root = Join-Path $env:LOCALAPPDATA 'QuaZonai'
$Bin = Join-Path $Root 'bin'
$Work = Join-Path ([IO.Path]::GetTempPath()) ([IO.Path]::GetRandomFileName())
$Staged = $null
[IO.Directory]::CreateDirectory($Work) | Out-Null
try {
    [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
    Invoke-WebRequest -UseBasicParsing "$Base/SHA256SUMS" -OutFile (Join-Path $Work 'SHA256SUMS')
    $Checksums = @(Get-Content (Join-Path $Work 'SHA256SUMS') | Where-Object { $_ -cmatch ('^[0-9a-f]{64}  ' + [regex]::Escape($Archive) + '$') })
    if ($Checksums.Count -ne 1) { throw "Missing or duplicate checksum for $Archive" }
    $Expected = $Checksums[0].Substring(0, 64)
    $ZipPath = Join-Path $Work $Archive
    Invoke-WebRequest -UseBasicParsing "$Base/$Archive" -OutFile $ZipPath
    if ((Get-FileHash -Algorithm SHA256 $ZipPath).Hash -ne $Expected) { throw "Checksum mismatch for $Archive" }
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $Zip = [IO.Compression.ZipFile]::OpenRead($ZipPath)
    try {
        if ($Zip.Entries.Count -ne 4) { throw 'Unexpected CLI archive files.' }
        foreach ($Name in @('quazonai.exe', 'LICENSE', 'NOTICE', 'THIRD_PARTY_NOTICES.md')) {
            $Entries = @($Zip.Entries | Where-Object { $_.FullName -ceq $Name })
            if ($Entries.Count -ne 1) { throw "Expected exactly one $Name in the CLI archive." }
            [IO.Compression.ZipFileExtensions]::ExtractToFile($Entries[0], (Join-Path $Work $Name))
        }
    } finally { $Zip.Dispose() }
    $Candidate = Join-Path $Work 'quazonai.exe'
    & $Candidate --version
    if ($LASTEXITCODE -ne 0) { throw 'The downloaded CLI could not run on this computer.' }
    $Licenses = Join-Path $Root "licenses/$Version"
    [IO.Directory]::CreateDirectory($Licenses) | Out-Null
    foreach ($Name in @('LICENSE', 'NOTICE', 'THIRD_PARTY_NOTICES.md')) {
        Copy-Item -LiteralPath (Join-Path $Work $Name) -Destination $Licenses -Force
    }
    [IO.Directory]::CreateDirectory($Bin) | Out-Null
    $Staged = Join-Path $Bin ([IO.Path]::GetRandomFileName())
    Copy-Item -LiteralPath $Candidate -Destination $Staged
    $Destination = Join-Path $Bin 'quazonai.exe'
    $UserPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    if ($Bin -notin ($UserPath -split ';')) {
        [Environment]::SetEnvironmentVariable('Path', ($Bin + ';' + $UserPath), 'User')
    }
    if ([IO.File]::Exists($Destination)) {
        [IO.File]::Replace($Staged, $Destination, [NullString]::Value)
    } else {
        [IO.File]::Move($Staged, $Destination)
    }
    $Staged = $null
    if ($Bin -notin ($env:Path -split ';')) { $env:Path = $Bin + ';' + $env:Path }
    Write-Host "Installed QuaZonai CLI from ${Version}: $Destination"
    Write-Host 'The Docker stack runs on Linux x86_64; connect this CLI to that instance.'
} finally {
    if ($Staged -and [IO.File]::Exists($Staged)) { Remove-Item -LiteralPath $Staged -Force }
    Remove-Item -LiteralPath $Work -Recurse -Force
}
