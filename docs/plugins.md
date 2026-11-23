# MX Bikes plugins in MXB App

This guide is for authors of MX Bikes game plugins: the `.dlo` files the game loads from its
`plugins` folder. It covers how MXB App finds and lists your plugin, the optional manifest you
can ship beside it, how to get it into the Browse list, how updates are found, and how your
settings are shown.

You don't have to do anything for MXB App to list your plugin. Everything below is optional,
and adds to what players see.

## How MXB App finds your plugin

MXB App reads the game's install folder (`<MX Bikes>\plugins`), the same folder the game loads
plugins from. Each `.dlo` file there is one plugin. A few files are never listed or touched:
the game's own `proxy64.dlo`, `proxy_udp64.dlo` and `xinput64.dli`.

For a plugin it doesn't recognise, MXB App shows:

- the file name (without `.dlo`) as the name;
- as its files: the `.dlo`, and beside it `<name>_data\`, `<name>\`, `<name>.ini` and
  `<name>.log` when they exist.

Those files move together when a player turns the plugin off, and go to the Recycle Bin
together when they uninstall it. A manifest (below) can name more.

**Turning a plugin off** moves its files to `<MX Bikes>\mxbapp_disabled\plugins`, keeping the
same folder layout, so the game doesn't load it. Turning it back on moves them back. Nothing is
renamed and nothing is deleted. If a file of the same name is already on the other side, the
switch is refused and nothing moves. These changes are refused while MX Bikes is running.

**Uninstalling** sends the plugin's files to the Recycle Bin. Settings in the player's Documents
folder are left alone.

## The manifest: `<name>.mxbplugin.json`

Put a JSON file next to your `.dlo`, named after it: for `myplugin.dlo`, ship
`myplugin.mxbplugin.json`. Every field is optional.

```json
{
  "name": "My Plugin",
  "version": "1.4.0",
  "author": "You",
  "description": "One or two plain sentences about what it does.",
  "category": "hud",
  "homepage": "https://example.com/my-plugin",
  "gameVersion": "beta21",
  "files": ["myplugin_data/", "myplugin_fonts/", "myplugin.cfg"],
  "settings": [
    { "file": "myplugin.ini", "label": "Settings" },
    { "file": "myplugin/layout.ini", "label": "Layout", "where": "documents" }
  ],
  "keys": [
    { "key": "F7", "label": "Open the menu" },
    { "key": "Ctrl+F7", "label": "Reset the layout" }
  ],
  "racing": "stay",
  "conflicts": [
    { "with": "otherplugin.dlo", "reason": "Both draw the lap timer in the same place." }
  ],
  "updates": { "github": "you/my-plugin" }
}
```

| Field | What MXB App does with it |
|---|---|
| `name`, `author` | Shown instead of the file name. Up to 60 characters each. |
| `version` | Shown as the installed version (`v1.4.0`). Compared with your newest release when `updates` is set. |
| `description` | Shown on the plugin's page. Up to 400 characters, plain text. |
| `category` | One of `hud`, `graphics`, `physics`, `fixes`, `timing`, `other`. Used for Browse's filters. |
| `homepage` | Must start with `https://`. Shown as "Open its page". |
| `gameVersion` | The MX Bikes beta you built for, like `beta21`. If the player's game is a different beta, they see a note that it may not load. Letters after the number are ignored, so `beta21` matches `beta21e`. |
| `files` | More files and folders that belong to your plugin, relative to `plugins\`. A folder ends in `/`. These move with the plugin when it's turned off and go with it on uninstall. Paths with `..`, absolute paths and the game's own files are ignored. Up to 64 entries. |
| `settings` | Your settings files. Each one appears in the plugin's Settings editor (see below). `where` is `plugins` (the default, relative to `plugins\`) or `documents` (relative to `Documents\PiBoSo\MX Bikes`). Only files that exist are shown. Up to 16. |
| `keys` | Your keyboard shortcuts, listed on the plugin's page. Up to 16. |
| `racing` | Your default for **racing mode** (below): `off`, `stay` or `always-on`. |
| `conflicts` | Plugins yours doesn't get on with. `with` is the other plugin's `.dlo` file name or its Browse id (like `graphics-overhaul`). When both are on, players see your `reason`, credited to you. Up to 16. |
| `updates.github` | `owner/repo`. Your newest GitHub release's tag is your latest version (see Updates). |

Unknown fields are ignored, so you can add your own. A manifest that isn't valid JSON is
ignored as a whole, and the reason is written to MXB App's log. A manifest over 64 KB isn't
read. The manifest itself counts as one of your plugin's files.

For plugins already in the Browse list, the list's own facts are used, and the manifest only
adds your `version`.

## Racing mode

Racing mode is one switch that turns off the plugins a player doesn't want in an online race.
It can also switch on by itself when the player joins a server from MXB App with Race mode on.
Each plugin carries a tag the player can change:

- `off`: off when racing (looks-only plugins: lighting, effects, cameras);
- `stay`: stays on when racing (HUDs, timing, status). This is the default for unknown plugins;
- `always-on`: crash fixes and the like. Racing mode never turns it off, and the player can't
  change that.

Your `racing` value only sets the starting choice. What the player picks wins.

## Settings

A plugin's page in MXB App has a **Settings** section for each file you list in `settings`:

- the file opens as text in an editor in the page;
- **Save** writes it. The previous version is kept beside it as `<file>.bak`, one copy,
  replaced on each save. The write goes through a temporary file, so a failed save leaves
  your settings as they were;
- saving is refused while MX Bikes runs, because plugins usually write their own settings
  back when the game closes and would undo the edit;
- **Open in editor** opens the same file in the player's default program;
- files over 512 KB aren't loaded in the editor.

Only files your manifest lists, or that the Browse list knows about, can be opened or saved
this way.

If your plugin saves settings on its own, read the file when the game starts. Don't keep a copy
in memory that you write back over the player's edit.

## The Browse list

Browse is the list of plugins MXB App can install or link to. Each entry has a short summary,
a category, a racing default, the plugin's known files and keys, and where it comes from:

- **GitHub releases**: MXB App downloads the release asset (a `.zip` or a bare `.dlo`) and
  installs it in one click;
- **mxb-mods**: MXB App installs the post's default download in one click;
- **its own website**: shown with a "Get it" button. Used for plugins that need their own
  installer or overlay program.

Pictures, descriptions, ratings and update dates on plugin pages come from the plugin's post
on [mxb-mods.com](https://mxb-mods.com), in its Plugins or Code Mods category.

**To be listed:** post your plugin on mxb-mods.com in the Plugins or Code Mods category, then
open an issue on [github.com/Frostn1/mxb-app](https://github.com/Frostn1/mxb-app/issues) with:

- the post's link;
- where releases come from (a GitHub repo, or the mxb-mods post);
- your plugin's files and settings files;
- its keys;
- your racing default;
- any conflicts.

The entry is reviewed before it's added, because MXB App installs it for players in one click.

**What one-click install does:**

- It unpacks the download. Your plugin files are the contents of a `plugins/` folder at the
  top of the archive (or one level down). If there's no such folder, they're the shallowest
  folder with a `.dlo` in it.
- It copies them into the game's `plugins` folder.
- It never overwrites a player's existing `.ini`, `.cfg` or `.txt` file, or anything in a
  `presets` folder. Those are the player's settings.
- It skips readmes, licences and changelogs at the archive's top level, and builds for other
  PiBoSo games (`*_gpb.dlo`, `*_krp.dlo`, `*_wrs.dlo`, `*_kart.dlo`).
- A download that only contains a setup program (`.exe` or `.msi`) is refused. Those plugins
  are listed with "Get it" instead.

A good archive for MXB App:

```
myplugin-1.4.0.zip
  plugins/
    myplugin.dlo
    myplugin.mxbplugin.json
    myplugin_data/...
    myplugin.ini          <- defaults; never overwrites the player's copy
  README.txt              <- for people installing by hand; MXB App skips it
```

## Updates

- **Browse plugins from GitHub:** the newest release's tag (a leading `v` is dropped) is
  compared with the installed version, number by number. If it's newer, an **Update** button
  appears, and updating is the same one-click install.
- **Browse plugins from mxb-mods:** mxb-mods has no version numbers, so MXB App compares the
  post's last-edited date with the date it installed. Edit your post when you release.
- **Plugins outside Browse with `updates.github` in their manifest:** MXB App compares your
  newest release's tag with the manifest's `version`. When yours is newer, players see
  **Update on its page**, which opens your release page. MXB App never downloads or installs
  a plugin that isn't in Browse.

Update checks are cached for an hour. A failed check is retried after ten minutes.
