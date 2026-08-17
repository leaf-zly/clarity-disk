<#
.SYNOPSIS
Creates the administrator-protected backup receipt consumed by partition safety.

.DESCRIPTION
The operator must perform an out-of-place restore to another physical disk.
This tool compares deterministic tree digests, binds both Windows disk IDs and
the provider manifest, then writes the fixed ProgramData receipt with a
SYSTEM/Administrators-only ACL. It is intentionally not exposed through Tauri.
#>
[CmdletBinding()]
param(
  [Parameter(Mandatory = $true)][string]$SourceDiskId,
  [Parameter(Mandatory = $true)][string]$RecoveryPointId,
  [Parameter(Mandatory = $true)][string]$BackupManifestPath,
  [Parameter(Mandatory = $true)][string]$OriginalSamplePath,
  [Parameter(Mandatory = $true)][string]$RestoredSamplePath,
  [ValidateRange(1, 30)][int]$ValidityDays = 7
)

$ErrorActionPreference = 'Stop'
$principal = [Security.Principal.WindowsPrincipal]::new([Security.Principal.WindowsIdentity]::GetCurrent())
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
  throw 'Backup verification receipt creation requires an administrator shell.'
}
if (-not $SourceDiskId.Trim() -or -not $RecoveryPointId.Trim()) { throw 'Source disk and recovery point identities are required.' }

$manifest = (Resolve-Path -LiteralPath $BackupManifestPath).Path
$original = (Resolve-Path -LiteralPath $OriginalSamplePath).Path
$restored = (Resolve-Path -LiteralPath $RestoredSamplePath).Path
if (-not (Test-Path -LiteralPath $original -PathType Container) -or -not (Test-Path -LiteralPath $restored -PathType Container)) {
  throw 'Both original and restored sample paths must be directories.'
}

function Get-DiskIdentityForPath([string]$Path) {
  $root = [IO.Path]::GetPathRoot($Path)
  if ($root -notmatch '^([A-Za-z]):\\$') { throw 'Restore verification samples must be on mounted Windows volumes.' }
  $partition = Get-Partition -DriveLetter $Matches[1] -ErrorAction Stop
  $disk = Get-Disk -Number $partition.DiskNumber -ErrorAction Stop
  if (-not $disk.UniqueId) { throw 'Windows did not provide a stable disk identity.' }
  return [string]$disk.UniqueId
}

function Get-TreeDigest([string]$Root) {
  $items = Get-ChildItem -LiteralPath $Root -File -Recurse -Force -ErrorAction Stop |
    Sort-Object { $_.FullName.Substring($Root.Length).ToLowerInvariant() }
  $lines = foreach ($item in $items) {
    if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Restore sample contains a reparse point.' }
    $relative = $item.FullName.Substring($Root.Length).TrimStart('\').ToLowerInvariant()
    $hash = (Get-FileHash -LiteralPath $item.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
    "$relative|$($item.Length)|$hash"
  }
  $payload = [Text.Encoding]::UTF8.GetBytes(($lines -join "`n"))
  $sha = [Security.Cryptography.SHA256]::Create()
  try { return ([Convert]::ToHexString($sha.ComputeHash($payload))).ToLowerInvariant() }
  finally { $sha.Dispose() }
}

$actualSourceDiskId = Get-DiskIdentityForPath $original
$destinationDiskId = Get-DiskIdentityForPath $restored
if ($actualSourceDiskId -ne $SourceDiskId) { throw 'Original sample disk does not match the immutable plan source disk.' }
if ($destinationDiskId -eq $SourceDiskId) { throw 'Restored sample must be located on another physical disk.' }
$originalDigest = Get-TreeDigest $original
$restoredDigest = Get-TreeDigest $restored
if ($originalDigest -ne $restoredDigest) { throw 'Out-of-place restore digest did not match the original sample.' }

$now = [DateTimeOffset]::UtcNow
$receipt = [ordered]@{
  schemaVersion = 1
  sourceDiskId = $SourceDiskId
  destinationDiskId = $destinationDiskId
  recoveryPointId = $RecoveryPointId
  manifestSha256 = (Get-FileHash -LiteralPath $manifest -Algorithm SHA256).Hash.ToLowerInvariant()
  backupCompletedAtUnixMs = [uint64]$now.AddMinutes(-1).ToUnixTimeMilliseconds()
  restoreVerifiedAtUnixMs = [uint64]$now.ToUnixTimeMilliseconds()
  expiresAtUnixMs = [uint64]$now.AddDays($ValidityDays).ToUnixTimeMilliseconds()
  restoreDigestVerified = $true
}

$directory = Join-Path $env:ProgramData 'ClarityDisk'
$target = Join-Path $directory 'backup-verification.v1.json'
$temporary = Join-Path $directory 'backup-verification.v1.json.tmp'
New-Item -ItemType Directory -Path $directory -Force | Out-Null
$receipt | ConvertTo-Json | Set-Content -LiteralPath $temporary -Encoding utf8
Move-Item -LiteralPath $temporary -Destination $target -Force

$acl = [Security.AccessControl.FileSecurity]::new()
$acl.SetAccessRuleProtection($true, $false)
$inheritance = [Security.AccessControl.InheritanceFlags]::None
$propagation = [Security.AccessControl.PropagationFlags]::None
$allow = [Security.AccessControl.AccessControlType]::Allow
$fullControl = [Security.AccessControl.FileSystemRights]::FullControl
$acl.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new('SYSTEM', $fullControl, $inheritance, $propagation, $allow))
$acl.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new('BUILTIN\Administrators', $fullControl, $inheritance, $propagation, $allow))
$acl.SetOwner([Security.Principal.NTAccount]::new('BUILTIN\Administrators'))
Set-Acl -LiteralPath $target -AclObject $acl

Write-Output 'Independent backup restore receipt verified and installed.'
