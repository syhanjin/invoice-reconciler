param(
    [Parameter(Mandatory = $true)]
    [string]$Path
)

$ErrorActionPreference = "Stop"

if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
    throw "Windows executable was not found: $Path"
}

$resolvedPath = (Resolve-Path -LiteralPath $Path).Path
$bytes = [System.IO.File]::ReadAllBytes($resolvedPath)
if ($bytes.Length -lt 64) {
    throw "File is too small to contain a valid PE header: $resolvedPath"
}

$peOffset = [System.BitConverter]::ToInt32($bytes, 0x3c)
$subsystemOffset = $peOffset + 24 + 68
if ($peOffset -lt 0 -or $subsystemOffset + 2 -gt $bytes.Length) {
    throw "PE header points outside the executable: $resolvedPath"
}

$signature = [System.Text.Encoding]::ASCII.GetString($bytes, $peOffset, 4)
if ($signature -ne "PE`0`0") {
    throw "Invalid PE signature in executable: $resolvedPath"
}

$optionalHeaderMagic = [System.BitConverter]::ToUInt16($bytes, $peOffset + 24)
if ($optionalHeaderMagic -ne 0x10b -and $optionalHeaderMagic -ne 0x20b) {
    throw ("Unsupported PE optional-header magic 0x{0:x}: {1}" -f $optionalHeaderMagic, $resolvedPath)
}

$subsystem = [System.BitConverter]::ToUInt16($bytes, $subsystemOffset)
if ($subsystem -ne 2) {
    throw "Expected IMAGE_SUBSYSTEM_WINDOWS_GUI (2), found subsystem $subsystem in $resolvedPath"
}

Write-Host "Verified IMAGE_SUBSYSTEM_WINDOWS_GUI: $resolvedPath"
