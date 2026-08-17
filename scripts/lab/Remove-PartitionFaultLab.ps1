<#
.SYNOPSIS
Removes one disposable VHDX lab run after validating its marker and state.
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
if (-not $runRoot.StartsWith($root, [StringComparison]::OrdinalIgnoreCase) -or $runRoot -eq $root) {
  throw 'Refusing to remove an unscoped lab path.'
}
$statePath = Join-Path $runRoot 'lab-state.json'
if (-not (Test-Path -LiteralPath $statePath -PathType Leaf)) { throw 'Lab state is missing; manual review is required.' }
$state = Get-Content -LiteralPath $statePath -Raw -Encoding UTF8 | ConvertFrom-Json
$vhdPath = [IO.Path]::GetFullPath([string]$state.vhdPath)
if (-not $vhdPath.StartsWith($runRoot, [StringComparison]::OrdinalIgnoreCase) -or [IO.Path]::GetExtension($vhdPath) -ne '.vhdx') {
  throw 'Lab VHDX path is outside the scoped run.'
}
Dismount-VHD -Path $vhdPath -ErrorAction SilentlyContinue
Remove-Item -LiteralPath $runRoot -Recurse -Force
