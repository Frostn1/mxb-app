<#
.SYNOPSIS
  Authenticode-sign files with Azure Artifact Signing, or skip cleanly when it isn't set up.

.DESCRIPTION
  Set up by .github/actions/artifact-signing, which exports ARTIFACT_SIGNING=1 and the paths
  used below. Tauri calls this for each binary it signs (see that action's signCommand overlay),
  and the release workflows call it directly for the injected DLL.

  Without ARTIFACT_SIGNING=1 it prints a notice and exits 0, so an unsigned build (a fork, or
  before the Azure account exists) still succeeds.

.EXAMPLE
  pwsh scripts/sign-windows.ps1 apps/manager/src-tauri/src/mxbsecure.dll
#>
param(
  [Parameter(Mandatory = $true, ValueFromRemainingArguments = $true)]
  [string[]] $Files
)

$ErrorActionPreference = 'Stop'

if ($env:ARTIFACT_SIGNING -ne '1') {
  Write-Host "sign-windows: signing not configured; leaving unsigned: $($Files -join ', ')"
  exit 0
}

foreach ($name in @('ARTIFACT_SIGNING_SIGNTOOL', 'ARTIFACT_SIGNING_DLIB', 'ARTIFACT_SIGNING_METADATA')) {
  if (-not (Get-Item "env:$name" -ErrorAction SilentlyContinue)) {
    throw "sign-windows: ARTIFACT_SIGNING=1 but $name is not set; run .github/actions/artifact-signing first"
  }
}

# Artifact Signing's certificates last three days, so every signature is timestamped with
# Microsoft's RFC 3161 service, which keeps it valid after the certificate expires.
$timestamp = 'http://timestamp.acs.microsoft.com'

foreach ($file in $Files) {
  if (-not (Test-Path -LiteralPath $file -PathType Leaf)) {
    throw "sign-windows: no such file: $file"
  }
  # One retry: the service or the timestamp server occasionally drops a request, and a whole
  # release shouldn't fail over it.
  $signed = $false
  for ($attempt = 1; $attempt -le 2 -and -not $signed; $attempt++) {
    & $env:ARTIFACT_SIGNING_SIGNTOOL sign /v /fd SHA256 /tr $timestamp /td SHA256 `
      /dlib $env:ARTIFACT_SIGNING_DLIB /dmdf $env:ARTIFACT_SIGNING_METADATA $file
    $signed = $LASTEXITCODE -eq 0
    if (-not $signed -and $attempt -lt 2) {
      Write-Host "sign-windows: signing $file failed (exit $LASTEXITCODE); retrying in 10 s"
      Start-Sleep -Seconds 10
    }
  }
  if (-not $signed) {
    throw "sign-windows: could not sign $file"
  }
  & $env:ARTIFACT_SIGNING_SIGNTOOL verify /pa $file | Out-Null
  if ($LASTEXITCODE -ne 0) {
    throw "sign-windows: $file was signed but its signature does not verify"
  }
  Write-Host "sign-windows: signed $file"
}
