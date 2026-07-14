param(
    [switch]$VerifyArchive
)

$ErrorActionPreference = "Stop"

$root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$executable = Join-Path $root "src-tauri\target\release\L2TV.exe"

if (-not (Test-Path -LiteralPath $executable -PathType Leaf)) {
    throw "Release executable was not found: $executable"
}

$bytes = [IO.File]::ReadAllBytes($executable)
if ($bytes.Length -lt 512 -or $bytes[0] -ne 0x4D -or $bytes[1] -ne 0x5A) {
    throw "Release executable does not have a valid DOS header."
}

$peOffset = [BitConverter]::ToInt32($bytes, 0x3C)
if ($peOffset -lt 0 -or $peOffset + 96 -ge $bytes.Length) {
    throw "Release executable has an invalid PE header offset."
}
if ($bytes[$peOffset] -ne 0x50 -or $bytes[$peOffset + 1] -ne 0x45) {
    throw "Release executable does not have a valid PE signature."
}

$optionalHeader = $peOffset + 24
$subsystem = [BitConverter]::ToUInt16($bytes, $optionalHeader + 68)
if ($subsystem -ne 2) {
    throw "Expected Windows GUI subsystem (2), found $subsystem."
}

$frontend = Join-Path $root "dist\frontend"
foreach ($required in @("index.html", "app.js", "styles.css", "tauri-bridge.js", "calendar.html", "calendar.css", "calendar.js")) {
    if (-not (Test-Path -LiteralPath (Join-Path $frontend $required) -PathType Leaf)) {
        throw "Prepared frontend is missing $required."
    }
}

Write-Output "Verified Windows GUI executable: $executable"

if ($VerifyArchive) {
    $archive = Get-ChildItem -LiteralPath (Join-Path $root "dist") -Filter "L2TV-Tauri*.7z" -File |
        Sort-Object LastWriteTimeUtc -Descending |
        Select-Object -First 1
    if (-not $archive) {
        throw "Release archive was not found."
    }
    $hashFile = "$($archive.FullName).sha256.txt"
    if (-not (Test-Path -LiteralPath $hashFile -PathType Leaf)) {
        throw "SHA-256 file was not found: $hashFile"
    }
    $sevenZip = (Get-Command 7z.exe -ErrorAction SilentlyContinue).Source
    if (-not $sevenZip) {
        $sevenZip = "C:\Program Files\7-Zip\7z.exe"
    }
    if (-not (Test-Path -LiteralPath $sevenZip -PathType Leaf)) {
        throw "7-Zip was not found."
    }
    & $sevenZip t $archive.FullName | Out-Host
    if ($LASTEXITCODE -ne 0) {
        throw "7-Zip archive verification failed with exit code $LASTEXITCODE."
    }
    $expectedHash = (Get-Content -LiteralPath $hashFile |
        Where-Object { $_ -match [regex]::Escape($archive.Name) } |
        Select-Object -First 1) -replace '^.*SHA256\s+', ''
    $actualHash = (Get-FileHash -LiteralPath $archive.FullName -Algorithm SHA256).Hash
    if (-not $expectedHash -or $actualHash -ne $expectedHash.Trim()) {
        throw "Archive SHA-256 verification failed."
    }
    Write-Output "Verified archive and SHA-256: $($archive.FullName)"
}
