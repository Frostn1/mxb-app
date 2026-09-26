#!/usr/bin/env node
// Run the Tauri CLI with CARGO_TARGET_DIR steered off a space in the repo's own path.
//
// The Windows mingw toolchain tauri-winres calls (windres, then cc1) splits its argv on
// spaces with no quoting, so a build whose out-dir sits under a folder like `Sean Dahan`
// breaks partway through `cargo build`, in a way that has nothing to do with anything this
// repo did. Nobody's path is hardcoded here: on Windows, if the current directory's path
// has a space and CARGO_TARGET_DIR isn't already set, this asks Windows for that same
// directory's short (8.3) name, which is always space-free, and points cargo's target dir
// under that instead — wherever the repo happens to be cloned, for whoever cloned it.
import { execFileSync, spawnSync } from "node:child_process";

const args = process.argv.slice(2);
const env = { ...process.env };
// `tauri` on Windows is a `.cmd` shim, which needs a shell to run at all; with one, Node
// warns unless the whole line is one string, so build it and quote each arg ourselves rather
// than hand `args` to spawnSync directly.
const quote = (a) => (/[\s"]/.test(a) ? `"${a.replace(/"/g, '\\"')}"` : a);

if (process.platform === "win32" && !env.CARGO_TARGET_DIR && process.cwd().includes(" ")) {
  try {
    // `for %I in (.) do @echo %~sI` is cmd's own way to ask for a path's 8.3 short form.
    const short = execFileSync("cmd", ["/d", "/c", "for %I in (.) do @echo %~sI"], {
      encoding: "utf8",
    }).trim();
    if (short && !short.includes(" ")) {
      env.CARGO_TARGET_DIR = `${short}\\target`;
    }
  } catch {
    // 8.3 short names can be switched off machine-wide (fsutil 8dot3name). If so, this falls
    // through and the build fails with cargo's own error instead of one from this script.
  }
}

const result = spawnSync(["tauri", ...args.map(quote)].join(" "), { stdio: "inherit", env, shell: true });
process.exit(result.status ?? 1);
