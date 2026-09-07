/**
 * Render both bootstrap scripts to disk, so PowerShell itself can be asked whether they parse.
 *
 * These two scripts run in exactly one place -- as EC2 user-data on a Windows instance with no
 * console and no key pair -- and every failure path ends in `Stop-Computer`, which destroys the
 * evidence. Three builds have been lost to PowerShell mistakes that a parser would have caught
 * in a second: `Get-Content -Tail N -Raw` (an invalid combination), and an `agent.json` built by
 * hand whose `C:\mxb\game` was never valid JSON. Neither is a type error, so `tsc` sees nothing:
 * to TypeScript this file is a string.
 */
import { writeFileSync } from "node:fs";
import { bootstrapScript, imageBootstrapScript } from "../src/bootstrap.js";
import type { BootstrapInputs } from "../src/bootstrap.js";

const inputs: BootstrapInputs = {
  agentToken: "ci-token",
  agentUrl: "https://control-plane.example.com/v1/agent.exe",
  gameUrl: "https://www.mxbikes.com/installer.exe",
  // Deliberately not plain ASCII: the name is the operator's, it is interpolated into the
  // script, and a non-Latin-1 one has broken a launch before.
  serverName: "CI 🏁 server",
  gamePort: 54210,
  agentPort: 8787,
  serverId: "srv_ci",
  controlPlaneUrl: "https://control-plane.example.com",
};

const out = process.argv[2] ?? ".";
writeFileSync(`${out}/bootstrap.ps1`, bootstrapScript(inputs));
writeFileSync(`${out}/image-bootstrap.ps1`, imageBootstrapScript(inputs));
