# Installs Snag from its GitHub release, for everyone on the machine (Chocolatey
# runs elevated). __VERSION__ and __CHECKSUM__ are filled in when the package is
# built, from the release's own SHA256SUMS.txt.
$ErrorActionPreference = 'Stop'

$packageArgs = @{
  packageName    = $env:ChocolateyPackageName
  fileType       = 'exe'
  url64bit       = 'https://github.com/EmreO33/snag/releases/download/v__VERSION__/Snag-__VERSION__-windows-setup.exe'
  checksum64     = '__CHECKSUM__'
  checksumType64 = 'sha256'
  silentArgs     = '/VERYSILENT /SUPPRESSMSGBOXES /NORESTART /SP- /ALLUSERS'
  validExitCodes = @(0)
  softwareName   = 'Snag'
}

Install-ChocolateyPackage @packageArgs
