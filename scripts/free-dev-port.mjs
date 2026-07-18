#!/usr/bin/env node
// Free the Vite dev port before `tauri dev` starts. A previous run that was
// Ctrl-C'd (or hid to the tray) can leave vite/frost squatting on the port;
// because vite.config.ts uses `strictPort`, the next run would fail to bind and
// die. This clears it so `tauri dev` reliably starts on the first try.
import { execSync } from "node:child_process";

const PORT = 1420;

function pidsOnPort(port) {
  try {
    if (process.platform === "win32") {
      const out = execSync(`netstat -ano -p tcp`, { encoding: "utf8" });
      return [
        ...new Set(
          out
            .split(/\r?\n/)
            .filter((l) => l.includes(`:${port}`) && /LISTENING/i.test(l))
            .map((l) => l.trim().split(/\s+/).pop())
            .filter((p) => p && p !== "0"),
        ),
      ];
    }
    const out = execSync(`lsof -ti tcp:${port} -sTCP:LISTEN`, {
      encoding: "utf8",
    });
    return out.split(/\s+/).filter(Boolean);
  } catch {
    // No listener (lsof/netstat exit non-zero when nothing matches) → nothing to do.
    return [];
  }
}

const pids = pidsOnPort(PORT);
for (const pid of pids) {
  try {
    if (process.platform === "win32") {
      execSync(`taskkill /F /PID ${pid}`, { stdio: "ignore" });
    } else {
      process.kill(Number(pid), "SIGKILL");
    }
    console.log(`[predev] freed port ${PORT} (killed stale pid ${pid})`);
  } catch {
    /* already gone */
  }
}
