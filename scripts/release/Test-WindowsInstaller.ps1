<#
.SYNOPSIS
Runs a GitHub Runner-only install, signature, and uninstall smoke test.

.PARAMETER InstallerPath
Absolute path to the freshly built NSIS installer.

.PARAMETER ExpectedThumbprint
Certificate thumbprint imported by the release workflow.
#>
[CmdletBinding()]
param(
  [Parameter(Mandatory = $true)][string]$InstallerPath,
  [Parameter(Mandatory = $true)][string]$ExpectedThumbprint
)

$ErrorActionPreference = 'Stop'
$resolvedInstaller = (Resolve-Path -LiteralPath $InstallerPath).Path
$signature = Get-AuthenticodeSignature -LiteralPath $resolvedInstaller
if ($signature.Status -ne 'Valid' -or $signature.SignerCertificate.Thumbprint -ne $ExpectedThumbprint) {
  throw 'NSIS installer signature is missing, invalid, or issued by an unexpected publisher.'
}

$install = Start-Process -FilePath $resolvedInstaller -ArgumentList '/S' -Wait -PassThru -WindowStyle Hidden
if ($install.ExitCode -ne 0) { throw "NSIS silent installation failed with exit code $($install.ExitCode)." }

$uninstallRoots = @(
  'Registry::HKEY_CURRENT_USER\Software\Microsoft\Windows\CurrentVersion\Uninstall\*',
  'Registry::HKEY_LOCAL_MACHINE\Software\Microsoft\Windows\CurrentVersion\Uninstall\*'
)
$entry = Get-ItemProperty -Path $uninstallRoots -ErrorAction SilentlyContinue |
  Where-Object { $_.DisplayName -eq '澄盘' } |
  Select-Object -First 1
if (-not $entry -or -not $entry.InstallLocation) { throw 'Installed Clarity Disk registry entry was not found.' }

$installRoot = [IO.Path]::GetFullPath([string]$entry.InstallLocation)
$allowedRoots = @($env:LOCALAPPDATA, $env:ProgramFiles, ${env:ProgramFiles(x86)}) |
  Where-Object { $_ } |
  ForEach-Object { [IO.Path]::GetFullPath($_) }
if (-not ($allowedRoots | Where-Object { $installRoot.StartsWith($_, [StringComparison]::OrdinalIgnoreCase) })) {
  throw 'Installer wrote an unexpected installation location.'
}

$application = Join-Path $installRoot 'clarity-disk.exe'
if (-not (Test-Path -LiteralPath $application -PathType Leaf)) { throw 'Installed application executable was not found.' }
$applicationSignature = Get-AuthenticodeSignature -LiteralPath $application
if ($applicationSignature.Status -ne 'Valid' -or $applicationSignature.SignerCertificate.Thumbprint -ne $ExpectedThumbprint) {
  throw 'Installed application signature did not match the release publisher.'
}

$uninstaller = Join-Path $installRoot 'uninstall.exe'
if (-not (Test-Path -LiteralPath $uninstaller -PathType Leaf)) { throw 'NSIS uninstaller was not found.' }
$uninstall = Start-Process -FilePath $uninstaller -ArgumentList '/S' -Wait -PassThru -WindowStyle Hidden
if ($uninstall.ExitCode -ne 0) { throw "NSIS silent uninstall failed with exit code $($uninstall.ExitCode)." }
