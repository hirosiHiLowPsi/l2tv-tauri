$ErrorActionPreference = "Stop"

$root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$dist = Join-Path $root "dist"
$stage = Join-Path $dist "release-stage"
$package = Join-Path $stage "L2TV"
$archive = Join-Path $dist "L2TV-Tauri-v3.0.0-win-x64.7z"
$executable = Join-Path $root "src-tauri\target\release\L2TV.exe"

if (-not (Test-Path -LiteralPath $executable -PathType Leaf)) {
    throw "Release executable not found. Run npm run build first."
}

if (Test-Path -LiteralPath $stage) {
    $resolvedStage = (Resolve-Path -LiteralPath $stage).Path
    if (-not $resolvedStage.StartsWith($root + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) {
        throw "Refusing to remove a stage directory outside the project: $resolvedStage"
    }
    Remove-Item -LiteralPath $resolvedStage -Recurse -Force
}

if (Test-Path -LiteralPath $archive) {
    Remove-Item -LiteralPath $archive -Force
}

New-Item -ItemType Directory -Path $package -Force | Out-Null
Copy-Item -LiteralPath $executable -Destination (Join-Path $package "L2TV.exe")
Copy-Item -LiteralPath $executable -Destination (Join-Path $package "L2TV-Electron-Data-Exporter.exe")
Copy-Item -LiteralPath (Join-Path $root "README.md") -Destination $package
Copy-Item -LiteralPath (Join-Path $root "readme.txt") -Destination $package
Copy-Item -LiteralPath (Join-Path $root "Electron版データ引継ぎツール.txt") -Destination $package
Copy-Item -LiteralPath (Join-Path $root "RELEASE_NOTES_3.0.0.md") -Destination $package
Copy-Item -LiteralPath (Join-Path $root "LICENSE") -Destination $package
Copy-Item -LiteralPath (Join-Path $root "THIRD_PARTY_NOTICES.md") -Destination $package

$sevenZip = (Get-Command 7z.exe -ErrorAction SilentlyContinue).Source
if (-not $sevenZip) {
    $sevenZip = "C:\Program Files\7-Zip\7z.exe"
}
if (-not (Test-Path -LiteralPath $sevenZip -PathType Leaf)) {
    throw "7-Zip was not found. Install 7-Zip or add 7z.exe to PATH."
}

Push-Location $stage
try {
    & $sevenZip a -t7z -mx=9 -mmt=on $archive "L2TV" | Out-Host
    if ($LASTEXITCODE -ne 0) {
        throw "7-Zip failed with exit code $LASTEXITCODE."
    }
}
finally {
    Pop-Location
}

$exeHash = (Get-FileHash -LiteralPath $executable -Algorithm SHA256).Hash
$archiveHash = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash
$hashFile = "$archive.sha256.txt"
@(
    "L2TV.exe  SHA256  $exeHash",
    "L2TV-Electron-Data-Exporter.exe  SHA256  $exeHash",
    "$(Split-Path -Leaf $archive)  SHA256  $archiveHash"
) | Set-Content -LiteralPath $hashFile -Encoding utf8

[pscustomobject]@{
    Archive = $archive
    ArchiveBytes = (Get-Item -LiteralPath $archive).Length
    ArchiveSHA256 = $archiveHash
    ExecutableBytes = (Get-Item -LiteralPath $executable).Length
    ExecutableSHA256 = $exeHash
    ExporterSHA256 = $exeHash
    HashFile = $hashFile
} | Format-List
