# Extensions for MXB App

Anyone can build an extension. An extension can:

- change how the app looks: **themes** (colours, corner radius), **button styles**, **backgrounds** and **fonts**;
- add **tabs** of its own, which run in a sandbox and read the app's data through a small API;
- offer **commands**, **settings** and **AI tools** (MCP) the person can use.

An extension can't touch your files, the game folder, keys, sign-ins or anything else on the PC. It gets only the data you allowed when you installed it.

Building with an AI agent? Start with [extensions-ai-agents.md](extensions-ai-agents.md).

Start from [`extensions/template`](../extensions/template). Three working samples are in [`extensions/samples`](../extensions/samples): **Dunes** (every kind of look), **Friends Online** (a tab, a command, settings and an AI tool) and **Coach Laps** (your MXB Coach sessions and best laps as a tab, plus a `best_laps` AI tool).

---

## 1. Try it in five minutes

1. In MXB App open **Extensions → Build** and turn on **Developer mode**.
2. Click **Load folder…** and pick `extensions/template`.
3. Its tab appears in the top rail under **Extensions**; its theme appears in the theme picker at the top of the Extensions page.
4. Change a file, then **Load folder…** again. The new version replaces the old one.

## 2. The package

An extension is a folder with `extension.json` at its root. To ship it, pack it into a `.mxbext` (a zip):

```
node scripts/extensions/pack.mjs --keygen my-key.pem          # once
node scripts/extensions/pack.mjs path/to/folder --key my-key.pem
```

The packer and the app accept only these file types: `json html htm js mjs css png jpg jpeg webp gif svg woff woff2 ttf otf txt md`. Anything else (`.exe`, `.dll`, `.dlo`…) makes the app refuse the whole package.

Limits: 25 MB package, 10 MB per file, 60 MB unpacked, 2000 files.

## 3. `extension.json`

```json
{
  "manifestVersion": 1,
  "id": "yourname.my-extension",
  "name": "My Extension",
  "version": "1.0.0",
  "author": "Your Name",
  "description": "One sentence.",
  "homepage": "https://…",
  "minAppVersion": "0.21.0",
  "permissions": ["friends.presence"],
  "network": ["https://api.example.com"],
  "updateUrl": "https://example.com/my-extension/latest.json",
  "contributes": { }
}
```

| Field | Rules |
|---|---|
| `id` | 2–4 lower-case parts joined by dots (`a-z 0-9 -`). `mxb.*`, `mxbsecure.*` and `frost.*` are reserved for packages signed by mxbsecure. |
| `version` | Semver, `x.y.z`. Updates compare by it. |
| `name`, `author` | 1–60 characters. |
| `permissions` | From the table in §5. An unknown one fails the install. |
| `network` | Up to 10 `https://host[:port]` origins your views may `fetch`. Needs the `network` permission. No wildcards, no paths. |
| `updateUrl` | `https://` only. See §7. |

### `contributes`

```json
{
  "themes": [{
    "id": "dunes", "label": "Dunes",
    "dark":  { "background": "#17110b", "primary": "#f0a35e" },
    "light": { "background": "#f8f1e6", "primary": "#c2671f" },
    "radius": "0.5rem",
    "background": "dune-lines",
    "buttonStyle": "pill",
    "fonts": { "body": "my-font", "display": "my-condensed" }
  }],
  "buttonStyles": [{ "id": "pill", "label": "Pill", "radius": "999px", "uppercase": false,
                     "letterSpacing": 0.02, "weight": 700, "borderWidth": 0, "shadow": "glow" }],
  "backgrounds": [{ "id": "dune-lines", "label": "Dune lines", "image": "assets/dunes.png",
                    "gradient": { "angle": 180, "stops": ["#0000", "#f0a35e22"] },
                    "opacity": 0.5, "blur": 0, "position": "bottom", "size": "cover" }],
  "fonts": [{ "id": "my-font", "family": "My Font", "src": "fonts/my-font.woff2", "weight": "400 700" }],
  "views": [{ "id": "main", "title": "My tab", "entry": "view/index.html" }],
  "main": "main.html",
  "commands": [{ "id": "refresh", "title": "Refresh" }],
  "settings": [{ "key": "showOffline", "type": "boolean", "label": "Show offline", "default": false }],
  "mcp": { "tools": [{ "name": "friends_online", "description": "…", "inputSchema": { "type": "object" } }] }
}
```

**Theme tokens** (`dark` / `light`): `window background foreground card cardForeground popover popoverForeground primary primaryForeground secondary secondaryForeground muted mutedForeground faint accent accentForeground destructive destructiveForeground success warning border input ring`. A theme with only `dark` uses it for light too. Unset tokens fall back to the colorway underneath.

**Values are data, never CSS.** Colours must be hex (`#rgb`, `#rrggbb`, `#rrggbbaa`) or `rgb() rgba() hsl() hsla() oklch() oklab() lab() lch()` with plain numeric arguments. Anything else (`url()`, `var()`, `calc()`, a named colour) is ignored. Lengths are `px` or `rem` within a range. There is no way to ship a stylesheet for the app itself; that's deliberate (§8).

**Branding, the Play button and component shapes** are part of a theme, so they come and go with it:

```json
{
  "id": "square-paddock",
  "label": "Square Paddock",
  "brand": { "logo": "assets/logo.png", "wordmark": "Square Paddock" },
  "play": { "label": "Ride", "icon": "flag", "radius": "3px", "uppercase": true, "width": "wide",
            "background": "#facc15", "foreground": "#0a0a0a" },
  "components": { "cardRadius": "3px", "inputRadius": "2px", "borderWidth": 1, "tabMarker": "block" }
}
```

| Token | Values |
|---|---|
| `brand.logo` | png, webp, jpg or gif in the package, at most 512 KB; drawn 20 px tall before the name |
| `brand.wordmark` | up to 24 characters, replaces the product name in the title bar |
| `play.label` | up to 12 characters ("Running" and "Starting" stay the app's own) |
| `play.icon` | `play`, `flag`, `bolt`, `rocket`, `gauge` or `none` |
| `play.radius` | px or rem, up to 999px |
| `play.background`, `play.foreground` | colours, same rules as theme tokens |
| `play.uppercase`, `play.width` | `true`; `normal` or `wide` |
| `components.cardRadius`, `components.inputRadius` | px or rem, up to 32px |
| `components.borderWidth` | 0 to 3 (px) |
| `components.tabMarker` | `line` (default) or `block` |

The "by mxbsecure" byline, the window title and the app's own wording never change. Spacing isn't a token: the app's layout is in fixed sizes, and the interface size in Settings → Appearance already scales all of it. [`extensions/samples/square-paddock`](../extensions/samples/square-paddock) uses every token above.

**Settings** types: `boolean`, `string`, `number`, `enum` (with `options`). The Extensions page draws the form; your code reads the values.

## 4. Tabs: the `mxb` API

A view is an HTML page from your package. The app inlines the scripts, stylesheets and images it links (relative paths inside the package only), adds its own security policy and the `mxb` client, and runs it in a sandboxed frame. Bundle your code into plain files: ES module `import`s of other files don't resolve, and remote `<script>`/`<link>` tags are dropped.

Your page gets the app's theme as CSS variables on `:root` (`--background`, `--foreground`, `--card`, `--primary`, `--primary-foreground`, `--muted-foreground`, `--border`, `--radius`, …) and a `light`/`dark` class, kept in sync as the person changes theme.

Every call returns a promise. A call needing a permission you weren't granted rejects with `permission denied: <name>`.

| Call | Permission | Returns |
|---|---|---|
| `mxb.app.info()` | – | `{ app, version, locale, theme }` |
| `mxb.permissions()` | – | the permissions you were granted |
| `mxb.storage.get(key)` / `.set(key, value)` / `.remove(key)` / `.keys()` | – | your own key–value store, 512 KB |
| `mxb.settings.get()` / `.onChange(fn)` | – | your settings, with defaults |
| `mxb.mods.list()` | `mods.read` | `[{ name, type, folder, size }]` |
| `mxb.servers.list()` | `servers.read` | `[{ name, address, players, maxPlayers, pingMs, passworded, location, track, session, rating }]` |
| `mxb.friends.list()` | `friends.presence` | `[{ name, online, server, track, riders, lastSeenAt }]` |
| `mxb.game.state()` / `.onChange(fn)` | `game.state` | `{ game, running }` |
| `mxb.presets.list()` | `presets.read` | `[{ name }]` |
| `mxb.coach.sessions()` | `coach.read` | MXB Coach sessions, newest first: `[{ started, track, bike, category, event, laps, bestMs, cleanLaps }]` (`cleanLaps`: times of laps with no cut, reset or crash) |
| `mxb.ui.toast(text, kind?)` | – | shows a toast (`info`, `success`, `error`) |
| `mxb.ui.openExternal(url)` | `openExternal` | asks the person, then opens an http(s) link |
| `mxb.commands.on(id, fn)` | – | runs `fn` when the person runs your command |
| `mxb.mcp.handle(name, fn)` | `mcp.tools` | answers an AI tool call (§6) |
| `fetch(url)` | `network` | only to the origins in `network` |

Extensions never see file paths, account ids, friend codes, server agent URLs or tokens. The app builds each answer field by field.

`main` is an invisible page with the same API that runs while your extension is enabled, whether or not a tab is open. Commands go to it and to any of your tabs that are open; since commands are run from Extensions → Installed, where no tab is open, answer them in `main`. AI tool calls go to `main` first.

## 5. Permissions

The install dialog lists what you ask for; the person can untick any of them, and change their mind later on the Extensions page. Ask only for what you need, and handle a rejected call.

| Permission | What the person is told |
|---|---|
| `mods.read` | See your installed mods (names and sizes, not where they are) |
| `servers.read` | See the public server list |
| `friends.presence` | See which friends are online and where they ride. **Unticked by default**: it's other people's data. |
| `game.state` | See whether the game is running |
| `presets.read` | See your preset names |
| `coach.read` | See your MXB Coach sessions and lap times |
| `network` | Talk to its own servers (listed in `network`) |
| `openExternal` | Ask to open links in your browser |
| `mcp.tools` | Offer tools to AI assistants through the app |

Looks and tabs need no permission.

## 6. AI tools (MCP)

Declare tools in `contributes.mcp.tools` with a JSON Schema, ask for `mcp.tools`, and answer them:

```js
mxb.mcp.handle("friends_online", async () => {
  const friends = await mxb.friends.list();
  return friends.filter((f) => f.online);
});
```

**Shipped in v1:** the declaration, the permission, the in-frame handler, and the app-side registry: `extensionTools()` lists every tool from enabled extensions that hold `mcp.tools`, and `callExtensionTool(ext, name, args)` calls one (10 s timeout) and returns its JSON answer (`packages/shared/src/extensions/bridge.ts`). A tool's answer can only contain what the extension could already read.

**Outside agents:** MXB App ships an MCP server. An MCP client (Claude Desktop, Claude Code, Cursor…) launches the app's own executable with `--mcp`; that process speaks MCP over stdio and relays to the running app, which lists every enabled extension's tools as `ext_<id>__<tool>` beside its developer tools. Setup and the tool list are in [extensions-ai-agents.md](extensions-ai-agents.md).

**Consuming** MCP servers from an extension needs nothing new: an HTTP MCP server is just an origin in `network`.

## 7. Signing, trust and updates

| Shown as | Means | Installs |
|---|---|---|
| Built in | Ships inside the app (Colorways, Style Pack) | Always there |
| Verified by mxbsecure | Signed by mxbsecure's key | Yes |
| Signed by key `abcd…`, not reviewed | Signed by the author's own key | Yes, with a warning. The key is pinned. |
| Unsigned | No signature | Only in developer mode |

A signature that is present but doesn't match the files is always refused.

**Pinned keys.** The first signed install of an id pins its key. An update signed by a different key is refused, whatever it claims to be, so keep your `.pem` backed up. A move to mxbsecure's key (after review) is allowed.

**Updates.** Point `updateUrl` at JSON like:

```json
{ "version": "1.1.0", "url": "https://example.com/my-extension-1.1.0.mxbext" }
```

Catalog installs only update from the catalog. Adding a permission or a network host asks the person again.
Older versions are blocked unless developer mode is on; uninstall first for an intentional downgrade.
Names associated with mxbsecure and Frost are reserved for verified packages.
Unsigned extensions pause whenever developer mode is off.

The app checks 30 s after start and every 6 hours (if the person left **Update automatically** on), and on demand. An update installs by itself only if it is newer, has the same id, passes the same key check, and asks for **no new permission or network host**. Otherwise it waits on the Extensions page for the person to review it in the install dialog.

**Install from a URL** takes `https://` addresses only.

## Publish your extension

1. Open **Extensions → Build** in MXB App.
2. Drop a `.mxbext` file or an extension folder into **Publish your extension**.
3. Check the preview, then choose **Publish**.
4. It is live immediately as **Unverified community**.
5. Under **Your extensions**, choose **Request Verified**. After review it shows the Verified badge.

Updates to a Verified extension, and any update that asks for more permissions, are reviewed before they are served. You can do the same at mxbsecure.com/extensions/publish after signing in with Steam. Packages are limited to 25 MB and publishing has a daily upload limit. Verified extensions are signed by mxbsecure.

## 7a. Publishing to the marketplace

The Extensions tab in MXB App browses a catalog on the control plane (`/v1/extensions`): search by name, author and tags, five categories, sort by popular, new or top rated, a page per extension with screenshots, permissions, version, installs and rating, and one-click install and update. Installing from the catalog goes through the same package checks and consent dialog as a file.

To list yours, add catalog fields to `extension.json`:

```json
{
  "categories": ["tabs", "game"],
  "tags": ["laps", "coach"],
  "screenshots": ["store/1.png", "store/2.png"]
}
```

Categories: `themes`, `tabs`, `integrations`, `ai-tools`, `game`. Up to 8 tags and 6 screenshots (png, jpg or webp, 2 MB each, inside the package; 16:9 shows best). The first sentence of `description` is the card's summary.

The Build tab is the easiest route. For scripted publishing, sign and upload with your MXB App account's token:

```
node scripts/extensions/pack.mjs my-extension --key my-key.pem
MXB_TOKEN=... node scripts/extensions/publish.mjs yourname.my-extension-1.0.0.mxbext
```

The first upload of an id pins it to your account and author key, and each upload must be a higher version. Community uploads list immediately as Unverified. Updates to Verified extensions and updates requesting more permissions wait for review while the previous version stays served. Installs are counted once per install (the server keeps a digest, not the install id), and ratings are one per account, 1 to 5, not on your own.

## 8. Security model and threat model

The boundary is the frame. Everything an extension runs is inside a sandboxed `<iframe srcdoc>` in the app's own window; everything outside it is the app.

### What protects what

| Layer | Where | What it stops |
|---|---|---|
| `sandbox="allow-scripts"` without `allow-same-origin` | `ExtensionHost.tsx` | The frame gets an opaque origin: no access to the app's DOM, `localStorage`, cookies or JS objects. No popups, top-level navigation, forms or modal dialogs. |
| The IPC invoke key | Tauri 2 | Tauri's IPC script, with its random per-launch key, runs in the main frame only. A frame still sees `window.ipc` and a stub `__TAURI_INTERNALS__` whose `invoke` sends without the key, and every such message is dropped before dispatch (no answer, no effect); a forged `ext_set_enabled` from a frame was tested and changed nothing. |
| IPC origin check | Tauri 2 | A request from the frame's `null` origin is "remote"; app commands are refused without a matching remote capability, and the app grants none to it. |
| The frame's own CSP | `sandbox.ts` `viewCsp` | `default-src 'none'`; no `connect-src` unless `network` is granted (then only the listed origins); no frames, workers, objects, `<base>`, form targets. Written before any extension HTML is parsed. |
| The app's `frame-src 'none'` | `apps/manager/index.html` | The frame can't navigate itself to another page. If it ever loads twice, the bridge cuts it off (`onLoad`). |
| Permissions policy | `allow=` on the frame | No camera, microphone, location, clipboard, USB, serial, HID, Bluetooth, payment. |
| The bridge | `bridge.ts` | Answers only messages whose `source` is that frame; a fixed method table; permission read fresh on every call; 60 calls/s. No method passes frame-chosen arguments to `invoke`. |
| Data shaping | `data.ts` | Answers are new objects with listed fields only: no paths, account ids, friend codes, LAN addresses, agent URLs or tokens. |
| Looks as data | `appearance.ts` | No author CSS reaches the app. Colours, lengths and numbers are validated; `url()` only for `data:` URLs the app builds from the package's own bytes. |
| Package checks | `extensions.rs` | File-type allow-list (no native code), zip-slip and zip-bomb limits, manifest validation, every referenced file must exist, Ed25519 signature over every file, pinned keys, reserved ids. |
| File confinement | `extensions.rs` `ext_read_*` | Reads only from an enabled extension's own folder, through the same symlink-resolving check plugins use. |
| Labels | `cleanLabel` | Names and titles lose control and bidi-override characters before they're shown. |

### Threats

| # | Threat | Result |
|---|---|---|
| T1 | Read or write the user's files, the game folder or the app's data | Blocked: no file API exists in the frame; IPC unreachable (no key, `null` origin); `ext_read_*` only serves the extension's own package files. |
| T2 | Steal keys or tokens (Steam session, mxbsecure account, sidecar keys, server agent tokens) | Blocked: none is reachable from the frame, and no bridge answer contains one. |
| T3 | Talk to the mxbsecure sidecar or run native code | Blocked: no process, IPC or native file types; packages with `.dll`/`.exe`/`.dlo` are refused. |
| T4 | Call app commands by forging IPC (`fetch("http://ipc.localhost/…")`, or `window.ipc.postMessage`) | Blocked: CSP `connect-src` stops the fetch; the postMessage path, like the fetch path, needs the invoke key the frame can't read; and a `null` origin is remote to Tauri's ACL. |
| T5 | Restyle the app to phish (fake sign-in, fake button) with author CSS | Blocked: no author CSS. A theme can recolour tokens, not add text or move elements. Extension tabs are drawn inside the content area under the app's own rail. |
| T6 | Exfiltrate data it was granted | **Accepted risk, disclosed.** With `network` it can send granted data to its listed hosts. Without it, `fetch`/XHR/WebSocket/images are blocked, but a browser has side channels (e.g. WebRTC) no CSP fully closes. The dialog says plainly that anything allowed can leave the PC, and friends' presence is opt-in. |
| T7 | Tamper with a package in transit or on a mirror | Blocked by the signature over every file. |
| T8 | Hijack an extension through an update | Blocked by key pinning; new permissions need the person's approval; `https` only. |
| T9 | Ship a malicious update from the real author | Limited: it can't gain permissions silently, and everything above still applies. Mitigation is review (verified tier) and the person's revoke/disable. |
| T10 | Navigate the frame to a phishing page or exfiltrate via navigation | Blocked by `frame-src 'none'` on the app; a second load disconnects the bridge. |
| T11 | Talk to another extension or impersonate one | Blocked: each frame is answered as itself only; storage is namespaced by id from the app's side, not the frame's. |
| T12 | Denial of service (spin the CPU, spam toasts, huge storage) | Limited: rate limit, storage cap, toast length; a misbehaving extension is one switch away from off. CPU in the frame is not capped. |
| T13 | Zip slip / zip bomb / path tricks in `entry` | Blocked: safe-path check, per-file and total limits enforced while reading, `resolveInPackage` in the webview. |
| T14 | Spoof trust ("MXB Official") | Limited: the trust line is drawn by the app from the signature, not the manifest; reserved ids need mxbsecure's key. A theme's wordmark replaces the product name but never the byline or window title. |
| T15 | A program reaches the app through the MCP bridge | Off until the person turns on "Let AI agents connect". Loopback only (127.0.0.1); a per-launch token in a file inside the person's own app data folder is required first. An agent can't grant permissions (installs go through the dialog, clicked by the person) and gets only what the extension API exposes. Any program running as the same user is already as trusted as the person. |

### Rules for anyone changing this code

- Never add `allow-same-origin` (or `allow-top-navigation`, `allow-popups`) to the frame.
- Never add a bridge method that forwards frame-chosen arguments to `invoke`.
- New data for extensions goes through a `toExt*` builder in `data.ts` and a new permission.
- New look properties are validated in `appearance.ts`; never interpolate an author string into CSS.
- An app that hosts extensions needs `frame-src 'none'` in its `index.html` before it mounts `ExtensionFrame`.

## 9. Where the code is

| Part | File |
|---|---|
| Packages, signatures, state, commands | `crates/core/src/extensions.rs` |
| Manifest types | `packages/shared/src/extensions/manifest.ts` |
| Permissions | `packages/shared/src/extensions/permissions.ts` |
| Looks → CSS | `packages/shared/src/extensions/appearance.ts` |
| Frame document, CSP, `mxb` client | `packages/shared/src/extensions/sandbox.ts` |
| Bridge, commands, MCP registry | `packages/shared/src/extensions/bridge.ts` |
| Data shaping | `packages/shared/src/extensions/data.ts` |
| Frame, looks applier, runtime | `packages/shared/src/extensions/ExtensionHost.tsx` |
| Extensions tab: Marketplace, Installed, Build | `packages/shared/src/extensions/Marketplace.tsx`, `ExtensionsPage.tsx`, `BuildPage.tsx` |
| MCP server and `--mcp` bridge | `crates/core/src/extmcp.rs` |
| Built-in Colorways and Style Pack | `packages/shared/src/extensions/builtins.ts` |
| MXB App's data sources | `apps/manager/src/lib/extensionData.ts` |
| Pack and sign | `scripts/extensions/pack.mjs` |
