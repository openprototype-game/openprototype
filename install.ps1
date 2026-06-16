#Requires -Version 5.0
<#
.SYNOPSIS
    OpenPrototype installer for Windows.
.DESCRIPTION
    Downloads the release binary, optionally fetches the original Prototype CD
    image from the Internet Archive, and runs the binary's own offline install
    to place the Start Menu shortcut and the disc. Re-run any time to update.

        irm https://raw.githubusercontent.com/openprototype-game/openprototype/main/install.ps1 | iex
.PARAMETER Cue
    Install from an existing PROTOTYPE.cue instead of downloading the disc.
.PARAMETER Yes
    Don't prompt; download the disc image if none is supplied.
#>
[CmdletBinding()]
param(
    [string]$Cue,
    [switch]$Yes
)

$ErrorActionPreference = "Stop"

$repo = "openprototype-game/openprototype"
$archiveItem = "prototype-1995"
$discBase = "https://archive.org/download/$archiveItem"
$discBinSha1 = "E3054EBB69F9CC8810D96822348818712476D06C"
$discCueSha1 = "B68B02D2313BB070087DD19263CF9164186D3FB0"
$target = "x86_64-pc-windows-msvc"
$asset = "openprototype-$target.zip"

function Get-RemoteFile($url, $destination) {
    Write-Host "  $url"
    $previous = $ProgressPreference
    $ProgressPreference = "SilentlyContinue"

    try {
        Invoke-WebRequest -Uri $url -OutFile $destination -UseBasicParsing
    } finally {
        $ProgressPreference = $previous
    }
}

function Assert-Sha1($path, $expected) {
    $actual = (Get-FileHash -Path $path -Algorithm SHA1).Hash

    if ($actual -ne $expected) {
        throw "Checksum mismatch for $path`n  expected $expected`n  got      $actual"
    }
}

$work = Join-Path ([System.IO.Path]::GetTempPath()) ("openprototype-" + [System.Guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path $work | Out-Null

try {
    $zip = Join-Path $work $asset
    Write-Host "Downloading $asset ..."
    Get-RemoteFile "https://github.com/$repo/releases/latest/download/$asset" $zip

    Expand-Archive -Path $zip -DestinationPath $work -Force
    $binary = Join-Path $work "openprototype.exe"

    if (-not (Test-Path $binary)) {
        throw "The release archive did not contain openprototype.exe."
    }

    if (-not $Cue) {
        Write-Host ""
        Write-Host "OpenPrototype needs the original Prototype CD image (about 270 MB)."
        Write-Host "It is preserved at the Internet Archive:"
        Write-Host "  https://archive.org/details/$archiveItem"
        Write-Host ""

        $download = $Yes

        if (-not $Yes) {
            $answer = Read-Host "Download it now? [Y/n]"
            $download = $answer -notmatch '^[Nn]'
        }

        if (-not $download) {
            Write-Host "Re-run with -Cue C:\path\to\PROTOTYPE.cue once you have the disc."
            return
        }

        Write-Host "Downloading the disc image ..."
        $discBin = Join-Path $work "PROTOTYPE.bin"
        $discCue = Join-Path $work "PROTOTYPE.cue"
        Get-RemoteFile "$discBase/PROTOTYPE.bin" $discBin
        Assert-Sha1 $discBin $discBinSha1
        Get-RemoteFile "$discBase/PROTOTYPE.cue" $discCue
        Assert-Sha1 $discCue $discCueSha1
        $Cue = $discCue
    }

    Write-Host ""
    Write-Host "Installing ..."
    & $binary install --cue $Cue
} finally {
    Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
}
