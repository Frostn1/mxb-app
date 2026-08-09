/**
 * What a freshly launched server runs on first boot.
 *
 * EC2 executes this once, as SYSTEM, with no console attached and nobody watching. Every
 * failure mode therefore has to end in one of two states: a server that works, or an
 * instance that shuts itself down. A box that boots, fails halfway and sits there is the
 * expensive outcome — it bills by the hour and looks, from the outside, exactly like one
 * that is merely still starting.
 *
 * That is why the whole script is wrapped in a trap that calls `Stop-Computer`: instances
 * are launched with `InstanceInitiatedShutdownBehavior=terminate`, so shutting down *is*
 * self-destruction, and a bootstrap that cannot finish takes the instance with it.
 */

export interface BootstrapInputs {
  /** Bearer token the agent will require. Generated per server, never reused. */
  agentToken: string;
  /** Where to fetch the `mxb-agent` binary. Must be reachable without credentials. */
  agentUrl: string;
  /**
   * The MX Bikes installer.
   *
   * PiBoSo publishes no separate dedicated-server package — the server lives inside the
   * full installer, which is why this fetches a 2 GB `.exe` and unpacks it rather than
   * pulling down a small archive. Pointed at PiBoSo's own URL so nothing is rehosted.
   */
  gameUrl: string;
  /** Server name written into `dedicated.ini`, shown in the game's browser. */
  serverName: string;
  /** UDP port the game listens on. */
  gamePort: number;
  /** TCP port the agent listens on. */
  agentPort: number;
  /** This server's row id, so the box can announce itself against it. */
  serverId: string;
  /** Base URL of the control plane, for that announcement. */
  controlPlaneUrl: string;
}

/** Where everything lands on the instance. Referenced by the agent's config too. */
const ROOT = "C:\\mxb";

/**
 * PowerShell for EC2 user-data.
 *
 * Wrapped in `<powershell>` because that is how EC2Launch decides to run it as PowerShell
 * rather than as cmd, and `<persist>false</persist>` keeps it to first boot only — this
 * script is not idempotent and re-running it on every start would rewrite live config.
 */
export function bootstrapScript(input: BootstrapInputs): string {
  // Values are interpolated into a PowerShell string, so anything that could close a quote
  // or start a new statement has to be refused rather than escaped. The callers validate
  // too; this is the last line before it becomes code on someone's machine.
  for (const [field, value] of Object.entries(input)) {
    if (typeof value === "string" && /["'`$\r\n]/.test(value)) {
      throw new Error(`${field} contains a character that can't go in the bootstrap script`);
    }
  }

  return `<powershell>
$ErrorActionPreference = "Stop"
Start-Transcript -Path "C:\\mxb-bootstrap.log" -Append

# Say what we are doing, so a server that takes a quarter of an hour to arrive can show its
# progress instead of an unexplained spinner. Best-effort by design: a report that fails must
# never be the thing that fails a build which is otherwise working.
function Send-Stage {
  param([string] $Stage, [bool] $Ok = $true, [string] $Log = "")
  try {
    $payload = @{ stage = $Stage; ok = $Ok; log = $Log } | ConvertTo-Json -Compress
    Invoke-RestMethod -Method POST -Uri "${input.controlPlaneUrl}/v1/servers/${input.serverId}/bootstrap" -Headers @{ "Authorization" = "Bearer ${input.agentToken}"; "Content-Type" = "application/json" } -Body $payload -TimeoutSec 15 | Out-Null
  } catch {
    Write-Output "couldn't report stage '$Stage': $_"
  }
}

# Any unhandled failure below takes the instance down with it. Launched with
# InstanceInitiatedShutdownBehavior=terminate, so this is self-destruction, not a pause:
# a half-built server that sits idle is the one outcome that quietly costs money.
#
# Which is why the transcript is sent *before* shutting down. There is no console on this box
# and no key pair, so the log cannot be read from outside — and terminating destroys it.
# Without this, a bootstrap that failed at minute twelve and one still downloading look
# identical from the outside: a server that never turns up.
trap {
  $reason = "$_"
  Write-Output "bootstrap failed: $reason"
  try { Stop-Transcript } catch {}
  $tail = ""
  try { $tail = Get-Content -Path "C:\\mxb-bootstrap.log" -Tail 150 -Raw } catch {}
  Send-Stage -Stage "failed" -Ok $false -Log "$reason\`n$tail"
  Stop-Computer -Force
  exit 1
}

New-Item -ItemType Directory -Force -Path "${ROOT}" | Out-Null
Set-Location "${ROOT}"

# TLS 1.2: Server 2022 defaults are fine, but Invoke-WebRequest on a fresh image has been
# known to negotiate down and fail against modern endpoints.
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

Send-Stage -Stage "downloading the game"
Write-Output "fetching the MX Bikes installer (about 2 GB)"
# BITS rather than Invoke-WebRequest: IWR buffers the whole response in memory, and two
# gigabytes of it on a small instance is how this step fails.
Start-BitsTransfer -Source "${input.gameUrl}" -Destination "${ROOT}\\mxbikes-installer.exe"

Send-Stage -Stage "extracting the game"
Write-Output "extracting the server files"
# There is no separate dedicated-server download; the installer carries it and \`-extract\`
# unpacks it to mxbikes.zip without installing anything.
#
# It exits 1 on SUCCESS. Treating that as failure — which every normal convention says to
# do — leaves the bootstrap dead at the one step that actually worked, so the exit code is
# deliberately ignored and the *artifact* is what gets checked instead.
$proc = Start-Process -FilePath "${ROOT}\\mxbikes-installer.exe" -ArgumentList "-extract" -WorkingDirectory "${ROOT}" -Wait -PassThru -NoNewWindow
Write-Output "installer exited $($proc.ExitCode) (1 is normal here)"

$zip = Get-ChildItem -Path "${ROOT}" -Filter "mxbikes*.zip" | Select-Object -First 1
if (-not $zip) { throw "the installer produced no zip — extraction did not work" }
Expand-Archive -Path $zip.FullName -DestinationPath "${ROOT}\\game" -Force

if (-not (Test-Path "${ROOT}\\game\\mxbikes.exe")) {
  # A layout we did not expect. Look one level down before giving up, since repacks
  # sometimes nest everything inside a single folder.
  $inner = Get-ChildItem -Path "${ROOT}\\game" -Recurse -Filter "mxbikes.exe" | Select-Object -First 1
  if (-not $inner) { throw "no mxbikes.exe in the extracted files" }
  Move-Item -Path "$($inner.Directory.FullName)\\*" -Destination "${ROOT}\\game" -Force
}

Send-Stage -Stage "installing the agent"
Write-Output "fetching the agent"
Invoke-WebRequest -Uri "${input.agentUrl}" -OutFile "${ROOT}\\mxb-agent.exe" -UseBasicParsing

# The game reads this once at startup and never again, which is why changing a setting
# through the agent restarts the process.
@"
[connection]
name = ${input.serverName}
maxclient = 20

[event]
track = Victoria
track_layout =
"@ | Set-Content -Path "${ROOT}\\game\\dedicated.ini" -Encoding ASCII

# The box's own public address, from the instance metadata service. IMDSv2 needs a token
# first; v1 is disabled on hardened AMIs and the extra call costs nothing. Without this the
# agent binds a wildcard, works out its address from the primary interface, and prints the
# *private* IP — an address nobody outside the VPC can use.
$imdsToken = Invoke-RestMethod -Method PUT -Uri "http://169.254.169.254/latest/api/token" \`
  -Headers @{ "X-aws-ec2-metadata-token-ttl-seconds" = "300" } -TimeoutSec 10
$publicIp = Invoke-RestMethod -Uri "http://169.254.169.254/latest/meta-data/public-ipv4" \`
  -Headers @{ "X-aws-ec2-metadata-token" = $imdsToken } -TimeoutSec 10
if (-not $publicIp) { throw "this instance has no public address" }
Write-Output "public address is $publicIp"

@"
{
  "token": "${input.agentToken}",
  "listen": "0.0.0.0:${input.agentPort}",
  "public_url": "http://$publicIp:${input.agentPort}",
  "game_dir": "${ROOT}\\\\game",
  "ini": "dedicated.ini",
  "game_port": ${input.gamePort}
}
"@ | Set-Content -Path "${ROOT}\\agent.json" -Encoding ASCII

# The security group already gates what reaches the box; these rules are what let traffic
# past Windows' own firewall once it is there.
New-NetFirewallRule -DisplayName "MXB game" -Direction Inbound -Protocol UDP -LocalPort ${input.gamePort} -Action Allow | Out-Null
New-NetFirewallRule -DisplayName "MXB agent" -Direction Inbound -Protocol TCP -LocalPort ${input.agentPort} -Action Allow | Out-Null

# A scheduled task rather than a bare process: it survives the session ending, and starts
# again by itself if the box is ever stopped and started rather than rebuilt.
$action = New-ScheduledTaskAction -Execute "${ROOT}\\mxb-agent.exe" -Argument "${ROOT}\\agent.json" -WorkingDirectory "${ROOT}"
$trigger = New-ScheduledTaskTrigger -AtStartup
$principal = New-ScheduledTaskPrincipal -UserId "SYSTEM" -RunLevel Highest
Register-ScheduledTask -TaskName "mxb-agent" -Action $action -Trigger $trigger -Principal $principal -Force | Out-Null
Start-ScheduledTask -TaskName "mxb-agent"
Send-Stage -Stage "waiting for the agent"

# Wait for the agent to actually answer before saying the server is up. /health is the one
# route it serves without a token, and it is served before anything else is ready — which is
# all this needs to know: the process is alive and listening.
$ready = $false
foreach ($attempt in 1..30) {
  try {
    Invoke-RestMethod -Uri "http://127.0.0.1:${input.agentPort}/health" -TimeoutSec 5 | Out-Null
    $ready = $true
    break
  } catch {
    Start-Sleep -Seconds 2
  }
}
if (-not $ready) { throw "the agent never came up" }

# Tell the control plane where this server is. Until this call, the row it was created from
# has an empty address and is published to nobody: the public IP is assigned while the box
# boots, long after that row was written, and nothing else is in a position to report it.
# Authenticated with the agent's own token, which only this instance and that row hold.
#
# Retried, because everything else has already succeeded by this point — a transient network
# failure here would otherwise self-destruct a server that is up and running.
$announced = $false
foreach ($attempt in 1..5) {
  try {
    Invoke-RestMethod -Method POST -Uri "${input.controlPlaneUrl}/v1/servers/${input.serverId}/hello" \`
      -Headers @{ "Authorization" = "Bearer ${input.agentToken}"; "Content-Type" = "application/json" } \`
      -Body '{"agentPort":${input.agentPort},"gamePort":${input.gamePort}}' -TimeoutSec 15 | Out-Null
    $announced = $true
    break
  } catch {
    Write-Output "announce attempt $attempt failed: $_"
    Start-Sleep -Seconds 5
  }
}
if (-not $announced) { throw "couldn't tell the control plane this server is up" }

Send-Stage -Stage "ready"
Write-Output "bootstrap complete"
Stop-Transcript
</powershell>
<persist>false</persist>`;
}
