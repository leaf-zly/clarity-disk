<#
.SYNOPSIS
Creates a disposable two-volume VHDX for partition recovery drills.

.DESCRIPTION
This script refuses physical disks and only creates a new dynamically expanding
VHDX beneath a pre-marked destructive-lab root on a dedicated self-hosted runner.
#>
[CmdletBinding()]
param(
  [Parameter(Mandatory = $true)][string]$LabRoot,
  [Parameter(Mandatory = $true)][string]$RunId
)

$ErrorActionPreference = 'Stop'
$root = [IO.Path]::GetFullPath($LabRoot)
$marker = Join-Path $root 'ALLOW-CLARITY-DISK-DESTRUCTIVE-LAB'
if (-not (Test-Path -LiteralPath $marker -PathType Leaf)) { throw 'Destructive lab marker is missing.' }
if ($RunId -notmatch '^[A-Za-z0-9._-]+$') { throw 'Lab run identity is invalid.' }

$runRoot = [IO.Path]::GetFullPath((Join-Path $root $RunId))
if (-not $runRoot.StartsWith($root, [StringComparison]::OrdinalIgnoreCase)) { throw 'Lab run escaped its dedicated root.' }
if (Test-Path -LiteralPath $runRoot) { throw 'Lab run directory already exists.' }
New-Item -ItemType Directory -Path $runRoot | Out-Null

$vhdPath = Join-Path $runRoot 'partition-fault-lab.vhdx'
New-VHD -Path $vhdPath -Dynamic -SizeBytes 8GB | Out-Null
$mounted = Mount-VHD -Path $vhdPath -PassThru
$disk = $mounted | Get-Disk
if ($disk.IsBoot -or $disk.IsSystem -or $disk.BusType -ne 'File Backed Virtual') {
  Dismount-VHD -Path $vhdPath -ErrorAction SilentlyContinue
  throw 'Created disk did not resolve to a disposable file-backed virtual disk.'
}

Initialize-Disk -Number $disk.Number -PartitionStyle GPT
$source = New-Partition -DiskNumber $disk.Number -Size 3GB -AssignDriveLetter
$target = New-Partition -DiskNumber $disk.Number -Size 3GB -AssignDriveLetter
Format-Volume -Partition $source -FileSystem NTFS -NewFileSystemLabel 'ClaritySource' -Confirm:$false | Out-Null
Format-Volume -Partition $target -FileSystem NTFS -NewFileSystemLabel 'ClarityTarget' -Confirm:$false | Out-Null

$fixtureRoot = "$($source.DriveLetter):\ClarityDiskLabFixture"
New-Item -ItemType Directory -Path $fixtureRoot | Out-Null
$buffer = New-Object byte[] (8MB)
[Random]::new(42).NextBytes($buffer)
[IO.File]::WriteAllBytes((Join-Path $fixtureRoot 'deterministic.bin'), $buffer)
$fixtureHash = (Get-FileHash -LiteralPath (Join-Path $fixtureRoot 'deterministic.bin') -Algorithm SHA256).Hash.ToLowerInvariant()

@{
  schemaVersion = 1
  runId = $RunId
  vhdPath = $vhdPath
  diskNumber = $disk.Number
  sourcePartitionNumber = $source.PartitionNumber
  targetPartitionNumber = $target.PartitionNumber
  fixtureSha256 = $fixtureHash
} | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $runRoot 'lab-state.json') -Encoding utf8

Write-Output $runRoot
