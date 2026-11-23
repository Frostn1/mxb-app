# MXB App extensions for AI agents

This page is for AI coding agents (Claude Code, Codex, Cursor…) and the people driving them. It covers two things: **building** an extension with an agent, and the **MCP tools** an extension can offer to, or use from, AI assistants.

The full reference is [extensions.md](extensions.md). Everything here follows its rules.

---

## Build your first extension with your AI agent

You need MXB App, Node 18+, and a copy of this repository (or just its `extensions/template` folder and `scripts/extensions/pack.mjs`).

**1. Copy the template.**

```
cp -r extensions/template my-extension
```

**2. Give your agent this prompt** (edit the last line):

> You are building an MXB App extension in `./my-extension`. Read `docs/extensions.md` first and follow it exactly.
> Rules: everything goes in `extension.json` and plain HTML/CSS/JS files in the folder. The tab's only API is `window.mxb` (listed in docs §4); there is no Node, no Tauri, no `import` of other files, and no network unless the manifest lists the host under `network` and asks for the `network` permission. Ask for the fewest permissions that work, and handle a rejected call (`permission denied: …`) with a readable message. Use the CSS variables the app provides (`--background`, `--foreground`, `--card`, `--primary`, `--border`, `--radius`…) so it matches every theme. Use `textContent`, never `innerHTML`, for anything from another player. Change `id` to `yourname.something`.
> Build: *a tab that lists my installed tracks, newest first, with their size.*

**3. Load it.** MXB App → **Extensions → Build** → turn on **Developer mode** → **Load folder…** → pick `my-extension`. Its tab appears under **Extensions**. After each change, **Load folder…** again. With the MCP bridge (below) the agent does this itself: `extension_validate`, then `extension_install`, and you click Install in the app.

**4. Fix what you see.** If the tab is blank, the usual causes are: a script that `import`s another file (bundle it into one), a remote `<script>` (dropped by design), or a call that needs a permission you didn't list. Tell the agent what the tab shows; it can't see the app.

**5. Pack and sign it** when it works:

```
node scripts/extensions/pack.mjs --keygen my-key.pem      # once; keep it safe
node scripts/extensions/pack.mjs my-extension --key my-key.pem
```

Share the `.mxbext`. Others install it from **Install from file…**, or from a direct https link with **Install from URL**. They see "Signed by key …, not reviewed" until mxbsecure reviews and signs it.

### What an agent should never do

- Add `allow-same-origin`, or try to reach `parent`, `top`, Tauri or `window.ipc`. It can't, and an extension that tries reads as malicious.
- Ship CSS meant for the app itself. Looks go in `contributes.themes` / `buttonStyles` / `backgrounds` / `fonts` as data.
- Put secrets in the package. Anyone can unzip a `.mxbext`.

---

## MCP: tools an extension offers

An extension can give AI assistants tools that answer from the data it is allowed to read.

**Declare** the tool in `extension.json` and ask for `mcp.tools`:

```json
{
  "permissions": ["friends.presence", "mcp.tools"],
  "contributes": {
    "main": "main.html",
    "mcp": {
      "tools": [{
        "name": "friends_online",
        "description": "List the user's MX Bikes friends who are online right now, with server and track.",
        "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false }
      }]
    }
  }
}
```

**Answer** it from the extension's background page (`main`), which runs whether or not a tab is open:

```js
mxb.mcp.handle("friends_online", async (args) => {
  const friends = await mxb.friends.list();
  return friends.filter((f) => f.online);
});
```

Rules:

- Tool names are `a-z 0-9 _`, up to 64 characters. The description is what the assistant reads to decide when to call it; say what it returns.
- The answer must be JSON. It is copied, so functions and DOM nodes don't survive.
- A call times out after 10 seconds.
- A tool can only return what the extension could already read. `mcp.tools` adds no data access; it only lets the answer reach an assistant.
- The person sees "Offer tools to AI assistants through the app" at install and can switch it off.

**Test a tool**: with Developer mode on, each tool gets a **Try** button on the extension's card in Extensions → Installed, which calls it with `{}` and shows the answer.

## The MXB App MCP server: setup

MXB App is an MCP server. Your agent's client starts the app's own executable with `--mcp`, which speaks MCP over stdio and relays to the app you have open.

1. In MXB App: **Extensions → Build → Let AI agents connect**. It's off by default.
2. Copy the config shown under it into your client's MCP settings. It names your executable, for example:

   ```json
   { "mcpServers": { "mxb-app": { "command": "C:\\Users\\you\\AppData\\Local\\MXB App\\MXB App.exe", "args": ["--mcp"] } } }
   ```

   Claude Code: `claude mcp add mxb-app -- "<that path>" --mcp`.
3. Keep MXB App open. If it isn't running, the server still starts and its `app_status` tool says so.

**Tools**

| Tool | Does |
|---|---|
| `app_status` | App version, developer mode, installed count |
| `extensions_list` | Installed extensions: id, version, enabled, trust, permissions asked and granted, tabs, commands, AI tools |
| `extension_validate` `{path}` | The app's own checks on a folder or `.mxbext`: manifest, permissions, file types, every named file. Errors, or a summary. |
| `extension_pack` `{path, key?, out?}` | Pack a folder into a `.mxbext`, signed with your `.pem` if given, and check the result |
| `extension_install` `{path}` | Install a folder (developer mode) or `.mxbext`. **The app shows its consent dialog and you decide**; the tool waits up to 5 minutes and says what happened. |
| `extension_open` `{id, view?}` | Show the extension's tab in the app |
| `extension_logs` `{id}` | The last 300 console lines and uncaught errors from its tabs and background page |
| `extension_set_enabled` `{id, enabled}` | Turn it on or off |
| `extension_tool_call` `{extension, tool, arguments?}` | Call a tool an extension offers |
| `ext_<id>__<tool>` | Every enabled extension's own tools, listed directly |

**The loop an agent runs:** edit files → `extension_validate` → `extension_install` (you click Install the first time; reinstalls of the same unsigned folder also ask) → `extension_open` → `extension_logs` → fix → again. When it works: `extension_pack` with your key.

**What the bridge can't do:** grant a permission, install without you, reach files, keys or the app's commands, or accept connections from another machine (it listens on 127.0.0.1 only, and wants a per-launch token from a file in your own app data folder). Turning the switch off ends every session.

## MCP: tools an extension uses

An extension can call an MCP server that speaks HTTP (Streamable HTTP or SSE): list its origin under `network`, ask for `network`, and `fetch` it from the tab or the background page.

```json
{ "permissions": ["network"], "network": ["https://mcp.example.com"] }
```

```js
const r = await fetch("https://mcp.example.com/mcp", {
  method: "POST",
  headers: { "Content-Type": "application/json", Accept: "application/json, text/event-stream" },
  body: JSON.stringify({ jsonrpc: "2.0", id: 1, method: "tools/list" }),
});
```

There is no access to local (stdio) MCP servers: an extension can't start processes.

---

## Checklist an agent can run before handing over

- [ ] `extension.json` parses, `id` is `name.something`, `version` is `x.y.z`.
- [ ] Every file the manifest names exists; only allowed file types are in the folder.
- [ ] Each permission is used, and each call that needs one handles `permission denied`.
- [ ] No `innerHTML` with outside data; no remote scripts; no `import`.
- [ ] Looks only through `contributes`; colours are hex or `rgb()/hsl()/oklch()` numbers.
- [ ] It loads from **Load folder…** in developer mode and the tab works in light and dark.
- [ ] Packed with `pack.mjs`, signed with your own key.
