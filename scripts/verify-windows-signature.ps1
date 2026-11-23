<#
.SYNOPSIS
  Fail unless every given Windows binary carries a valid, timestamped Authenticode signature
  from the expected signer.

.DESCRIPTION
  The release workflows run this on every .exe/.dll they are about to publish, after signing
  and before anything is made public. For each file it checks:
    - Get-AuthenticodeSignature reports Valid (signature, chain and trust all good),
    - the signer certificate's subject contains -Signer (default "Creste LLC"),
    - the signature carries an RFC 3161 timestamp (Artifact Signing certificates last three
      days; without a timestamp the signature stops verifying after that),
    - `signtool verify /pa` passes, when the artifact-signing action exported a SignTool.
  Prints one line per file and writes a table to the job summary when there is one.

.EXAMPLE
  pwsh scripts/verify-windows-signature.ps1 target/release/frost-converter.exe
#>
[CmdletBinding(PositionalBinding = $false)]
param(
  [Parameter(Mandatory = $true, Position = 0, ValueFromRemainingArguments = $true)]
  [string[]] $Files,
  [Parameter()]
  [string] $Signer = 'Creste LLC'
)

$ErrorActionPreference = 'Stop'

$failures = @()
$rows = @()
foreach ($file in $Files) {
  if (-not (Test-Path -LiteralPath $file -PathType Leaf)) {
    $failures += "${file}: no such file"
    continue
  }
  $name = Split-Path -Leaf $file
  $sha = (Get-FileHash -LiteralPath $file -Algorithm SHA256).Hash.ToLowerInvariant()
  $sig = Get-AuthenticodeSignature -LiteralPath $file
  $subject = if ($sig.SignerCertificate) { $sig.SignerCertificate.Subject } else { '' }
  $stamp = if ($sig.TimeStamperCertificate) { $sig.TimeStamperCertificate.Subject } else { '' }
  $problems = @()
  if ($sig.Status -ne 'Valid') { $problems += "signature is $($sig.Status)" }
  if ($subject -notlike "*$Signer*") { $problems += "signer is '$subject', expected '$Signer'" }
  if (-not $stamp) { $problems += 'no timestamp' }
  if ($env:ARTIFACT_SIGNING_SIGNTOOL -and (Test-Path -LiteralPath $env:ARTIFACT_SIGNING_SIGNTOOL)) {
    $vout = & $env:ARTIFACT_SIGNING_SIGNTOOL verify /pa /v $file 2>&1 | Out-String
    if ($LASTEXITCODE -ne 0) {
      Write-Host $vout
      $problems += 'signtool verify /pa failed'
    }
  }
  Write-Host "${name}: $($sig.Status); signer: $subject; timestamp: $stamp; sha256: $sha"
  $rows += "| $name | $($sig.Status) | $subject | $(if ($stamp) { 'yes' } else { 'no' }) | ``$sha`` |"
  if ($problems) { $failures += "${name}: $($problems -join '; ')" }
}

if ($env:GITHUB_STEP_SUMMARY) {
  @('### Windows signatures', '', '| File | Status | Signer | Timestamp | SHA256 |',
    '| --- | --- | --- | --- | --- |') + $rows | Add-Content $env:GITHUB_STEP_SUMMARY
}

if ($failures) {
  foreach ($f in $failures) { Write-Host "::error title=Unsigned or badly signed binary::$f" }
  throw "verify-windows-signature: $($failures.Count) file(s) not properly signed"
}
