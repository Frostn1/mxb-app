# Changelog

## 2026-08-07 — the Rider preview can wear a rider model

### Added
- **Custom rider models show up on the rider.** A rider model is a whole new body mesh, not a
  texture — Rider+ and its variants install as folders under `mods/rider/riders`, and the
  preview never looked there. It read the body out of the game's `rider.pkz` and nowhere else,
  so picking an installed model listed the profile, found nothing to load, and rendered gear
  floating where the rider should be. The body is now resolved from the installed model first,
  loose or packed as a `.pkz`, and falls back to the game's own rider only when no model
  supplies one. A rider packed as `riders/<name>.pkz` is listed in the picker too, as gear
  already was.
- **A rider model wears the kits you already own.** Rider+ ships its `paints` and `gloves`
  folders empty on purpose, because existing gear is meant to work on it. A kit or glove paint
  is now looked for in the chosen profile, then inside its archive, then under the stock
  profiles — by exact name at every step, so reaching further never quietly swaps in a
  different paint. The kit dropdown lists what you own rather than going blank on a fresh
  model.

### Fixed
- **The rider stands up and faces forward.** Rider meshes don't agree on which axis is up: the stock motocross
  rider is authored Y-up, while the supermoto rider and Rider+ are Z-up and arrived lying on
  their back. Every piece of gear is anchored and scaled to a fraction of the body's height,
  so a body on its side measured a quarter of a metre tall instead of a metre and a bit — the
  helmet and boots shrank to specks and sank into the torso, which read as gear that never
  loaded. Standing it up alone left it facing backwards, which matters just as much: the
  viewer nudges the helmet and boots forward, so a rider turned around wears its gear through
  its own back. A body whose longest axis isn't its height is now rolled upright *and* turned
  to face front; one that already stands is left alone.
- **A rider model loads without decoding pixels nobody sees.** Dressing a body in its own
  baked textures used to inflate and re-encode every texture the mesh carried, then throw
  most of them away — skin renders as a flat colour and the name and number planes render as
  nothing at all. On a rider body that decode cost more than parsing the mesh. Only textures
  the viewer can actually draw are decoded now, and they're kept per model so changing a
  dropdown doesn't re-read a 67 MB body.
- **The rider's textures are read off the model instead of memorised.** Which texture a body
  part wears was decided by its material number: 1 was gloves, 2 was the face, 3 and 4 were
  hidden. That is not a rule, it's the stock motocross rider's texture order memorised — and
  no two rider models write that order the same way. The supermoto rider lists its face second
  and its gloves third, so it has been wearing its face on its hands; Rider+ lists its gloves
  first and its suit last, so it would have worn the glove texture over its whole body. Each
  part now binds to the texture the mesh itself says it was drawn against, the same reading the
  bike and gear previews already take. Anything a paint doesn't cover falls back to the
  model's own texture, so a rider that ships no paints — or a model with pieces of its own —
  renders as it was built rather than in flat grey.

## 2026-08-07 — v0.7.1 — the Rider preview stops failing in silence, and a blocked Browse says why

### Fixed
- **The Rider preview no longer goes quiet when it fails to update.** If resolving the
  rider hit an error — a missing profile, a gear file the loader couldn't read — the Rider
  tab caught it and did nothing with it. The previous model stayed on screen, deliberately,
  so the preview never blanks; but with no error anywhere that is indistinguishable from a
  pick that genuinely changed nothing, and it made a real fault read as "changing this slot
  does nothing". A failed resolve now raises a toast with the reason, leaves a badge on the
  preview for as long as what you're looking at is out of date, and writes the error to the
  console. The toast fires once per distinct message rather than once per pick, since a
  persistent fault is re-hit on every slot edit.

### Changed
- **When mxb-mods.com refuses us, the log now says enough to act on.** Browse failing with
  "mxb-mods.com refused the request (403)" wrote nothing to the log beyond whether the check
  window earned a cookie — the request that was actually refused went unrecorded, so a report
  of it and a screenshot of it carried the same information. A refusal now logs which endpoint
  was blocked (the catalog API and the rendered mod page sit behind different Cloudflare
  rules), Cloudflare's `cf-ray`, `cf-mitigated` and `retry-after` headers, the block reason
  from the response body, and which cookies the request actually carried — by name, never by
  value. The retry is narrated too, so the log distinguishes the two ways this fails: never
  earning a clearance, versus earning one in a real browser and being refused anyway, which
  points at the HTTP client's TLS fingerprint rather than at anything a cookie can fix.
- **The 403 dialog carries a reference id.** The `cf-ray` of the refused request is appended
  to the error, so a screenshot alone is enough to identify the block.
- **Two silent failures now speak up.** A catalog response of 400 that isn't "you paged past
  the end" used to render as an empty listing, and a mod page that yielded no downloads
  without being a Cloudflare interstitial said nothing at all. Both are logged.
- **`MXB_LOG=debug` traces every mxb-mods.com request.** Off by default, because search runs
  on each keystroke and a line per keystroke would bury the failure worth reading.

## 2026-08-07 — v0.7.0 — Race mode, an in-game overlay, and six languages

### Added
- **Race mode, and mods you can switch off.** MX Bikes mounts every archive in your mods
  folder at startup, so a big library is paid for on every load — even though a race needs
  one track, one bike, one gear set and a support pack or two. Give a preset the track it's
  ridden on, pin the packs that have to stay, and **Race mode** puts the preset's look on
  *and* takes everything else out of the game's way in one action. The paint, gear and model
  swap come from the loadout automatically, so the only things to pick by hand are the ones
  a loadout can't express. Nothing is deleted — disabled mods move to
  `<MX Bikes>\mxbapp_disabled`, mirroring the folder they came from, and **Enable
  everything** puts each one back in the exact path it left. The new **Manage** section does
  it by hand too: a switch per mod, bulk enable/disable of whatever the filter shows, and
  delete straight to the recycle bin. It's in the sidebar and in the overlay, so the next
  race can be lined up without leaving the game. Loose paints, model-swap sets and sound
  folders are left alone — they aren't what a load waits on.
- **An in-game overlay, on a hotkey.** Ctrl+Shift+X (rebindable in Settings → In-game
  overlay) brings Presets, the Locker and Browse up over MX Bikes in a floating panel; Esc
  hands control straight back to the game, and **Open full app** beside it switches to the
  main window instead, for Rider, the Library and Settings. It's the same UI as the main
  window, and it pays off because presets and model swaps already apply to a *running* game
  — pick a gear set from the pits and it's on you, no restart. One limit: nothing can draw
  over a game in exclusive fullscreen, so run MX Bikes borderless or windowed. Settings →
  In-game overlay says whether the game is running, offers **Show overlay now**, and names
  the reason if the shortcut didn't bind (another app owning the combo is the usual cause).
- **New versions show what's new in them.** An update used to land silently — the banner
  said a version was available, the app restarted, and the same screen came back, which is
  no way to find out about a feature you have to know a shortcut to use. After an update
  the app now shows the release's headline feature, a line each for the rest, and a link to
  the full notes. Once per version, never on a fresh install, and re-openable from
  Settings → About & updates → What's new.
- **MXB App speaks six languages** — English, Italian, Spanish, French, German and
  Brazilian Portuguese. Pick one under Settings → Appearance, or leave it on `System` to
  follow the OS. Every screen, dialog, toast and empty state is covered, and the wording
  follows what the community actually says: `mod`, `setup`, `preset` and `Stock` stay as
  loanwords, while gear is translated (`casco`, `casque`, `Helm`). Dates follow the app's
  language too, so picking Italian doesn't leave half the UI in English.
- **Browse can sort by what people actually ride.** New sort next to the category filters:
  **Most popular**, **Popular this month**, **Popular this week** (by views on
  mxb-mods.com) and **Oldest**, for digging back to the 2019 originals — instead of being
  locked to newest-first, which buries a track thousands of people ride under whatever went
  up this morning. The popular listings can't be searched, so they step aside while you're
  typing in the search box.
- **Star ratings on browse thumbnails.** Cards now carry the mxb-mods.com score — stars,
  average and vote count — so a well-rated mod stands out before you open it. Unrated mods
  show nothing rather than five empty stars, and ratings load after the cards appear, so
  browsing never waits on them.
- **A Play button that launches MX Bikes.** It's in the sidebar on every tab, and reads
  "MX Bikes running" while the game is up so it can't start a second copy. Windows launches
  the game directly — Steam copies and standalone alike — and Linux hands off to Steam,
  since a Proton install is Steam's to start.
- **A model swap now shows up in-game straight away.** Applying a swap in the Locker, or a
  preset that carries one, re-applies the bike you have selected — no more switching bike
  class away and back to see it. Needs FrostMod and the **Instant refresh** setting.
- **The 3D preview offers a gear model's stock paint, not just its liveries.** A helmet or
  boots that ship liveries had no way back to their own look. "Stock" now leads the paint
  list whenever the mesh carries a texture, with a separate entry for a helmet's goggles.
  Preview only — the game names a `.pnt` in your loadout and has no word for "the model's
  own look".

### Fixed
- **Every bike now paints the parts a paint is meant to reach.** This is the third pass at
  the same bug, and the first that goes at what was actually wrong. A part's material was
  being looked up in a table read from the top of the mesh file, treated as the model's
  one and only table. It isn't a model-wide table at all — it is simply the *first* node's,
  because that node's geometry starts exactly where it ends. **Every part carries its own**,
  and a material id means nothing outside the part it belongs to. Reading one part's ids
  through another's put the blank number-plate texture — the one the game composites race
  numbers onto, which no paint can touch — over real bodywork: the Suzuki RM250 and RM125
  wore it on the fork lowers, triple clamps and both levers, the Honda CR500AF on its entire
  swingarm and front end, the Husqvarna TC 125/TC 250 on the fork guards, chain guard and
  front bodywork. Across the 53 stock bikes it covered about 124,000 triangles of bodywork;
  it now covers about 4,900, all of it geometry the mesh itself marks as number plates.
  Bikes that share a part with another bike — much of the KTM, Husqvarna and GasGas range —
  now bind that part identically on every one of them, where 9 such parts previously
  disagreed.
- **Swingarms and chain guards wearing each other's texture.** Where one mesh group holds
  several materials, the ids were assumed to count upward from the group's first. They
  don't — each range names its own. On the Husqvarna FC 250 and FC 350 that swapped the
  swingarm body onto the plastics sheet and its chain guard onto the metals one, against
  fourteen sibling bikes carrying the identical part the right way round.
- **Bikes no longer look different from one launch to the next.** Choosing between two
  readings of a material meant scoring a part's UV layout against the textures, and the
  backdrop colour that scoring rested on was picked by iterating a hash map — so tied
  colours resolved differently on each run, and sometimes twice within one run. Six bikes
  bound parts differently between launches; on the Triumph TF 450-RC that was the whole
  20,570-triangle frame and engine going blank on some runs. With one reading of a material
  id there is nothing to score and nothing to guess: the scoring machinery is gone.
- **The Rider preview no longer goes quiet when it fails to update.** If resolving the
  rider hit an error — a missing profile, a gear file the loader couldn't read — the Rider
  tab caught it and did nothing with it. The previous model stayed on screen, deliberately,
  so the preview never blanks; but with no error anywhere that is indistinguishable from a
  pick that genuinely changed nothing, and it made a real fault read as "changing this slot
  does nothing". A failed resolve now raises a toast with the reason, leaves a badge on the
  preview for as long as what you're looking at is out of date, and writes the error to the
  console. The toast fires once per distinct message rather than once per pick, since a
  persistent fault is re-hit on every slot edit.
- **Bikes wearing the wrong texture on their bodywork.** A part's material was matched to
  the texture list in whatever order the exporter wrote it, which only works on bikes
  written in material order. The Kawasaki KX250/KX450 wore their blank number-plate texture
  over the whole bike, so an installed paint changed nothing visible; the Yamaha
  YZ125/YZ250 had chassis and engine swapped. The model's own material table now decides,
  and where the two readings disagree the mesh breaks the tie per part — which keeps the
  KTM 125 SX on its plastics.
- **The front fender and fork guards rendering in bare metal.** One mesh group can hold
  several materials — a fork leg and the plastic guard on it — and all of them wore the
  first one's texture. Each range now binds its own.
- **Goggles: switching a lens now reaches the model.** Two faults stacked: the preview
  watched every rider slot except the goggles, so a new lens only showed once you touched
  some *other* slot — and even then it was worn by nothing, because the goggles were
  identified by mesh-group name, and a helmet's goggles are as often called `mask` or sit
  in a node with no groups at all. The mesh's own materials now say which piece draws from
  which texture, with names as a hint rather than the whole story. Goggle paints that ship
  apart from the helmet are loaded too, and the game's free helmets — which never loaded a
  goggle paint at all — now wear one like an installed helmet does. Gear this still doesn't
  cover is being worked on; if a lens won't take, the log now names the paint it couldn't
  find, which is the thing to send along.
- **The overlay shortcut no longer defaults to Discord's mute key.** Ctrl+Shift+M is
  Discord's default push-to-mute; Discord registers it globally and gets there first, so on
  many machines the overlay hotkey never bound — invisibly, since a shortcut that was never
  registered has nothing to report when you press it. The default is now **Ctrl+Shift+X**,
  and an install still carrying the old default is moved across on next launch. A combo you
  picked yourself is left alone.
- **The Locker stops claiming a swap "Refreshed live in-game" when it didn't.** The note came
  from the look-loader call succeeding, which says nothing about whether the mesh reloaded.
  Model and sound swaps now report separately, and the model note says what actually
  happened: refreshing, FrostMod not running, or instant refresh off. It also catches a
  FrostMod too old to do it — re-applying the bike needs v0.9.9 or newer, and an older build
  takes the message, logs "unknown verb" and drops it, which from here looks exactly like
  success; that now reads "Update FrostMod to see model swaps live".
- **Installing a new version by hand no longer loops back to the "already installed"
  page.** MXB App sits in the tray after you close its window, so an installer you started
  yourself nearly always finds it running — and when the old uninstaller can't close it,
  the installer bounces you back to that page instead of installing. It now closes the app
  itself, WebView2 children and all, before anything is written or removed. The in-app
  updater was never affected, which is why this only turned up on a manual install.
  **Upgrading from 0.6.3 by hand?** That version's uninstaller predates the fix, so quit
  MXB App from the tray first (right-click the tray icon → Quit) — once you're on 0.7.0,
  it takes care of itself.
- **Updating FrostMod with MX Bikes open no longer fails** with "the process cannot access
  the file… (os error 32)". Windows won't let a loaded `frostmod.dll` be overwritten, so no
  amount of retrying could outlast a running game. The old binaries are renamed aside
  instead and the new ones take their place, so the update lands with the game still up —
  and the toast says to restart MX Bikes if the old FrostMod is the one still loaded.
- **A half-applied FrostMod update can't strand you on a version you never installed.**
  Both binaries now stage together and move into place as one unit, so a failure puts the
  previous install back and the version is recorded only once both files are really on
  disk. An install already carrying the wrong version number is caught by checksum rather
  than assumed fine, and repaired on next launch — or on demand with **Repair install**.
- **Browse gets past mxb-mods.com's bot protection.** When the site refuses the app, it now
  opens a small mxb-mods.com window, lets the Cloudflare check clear, and reuses the
  clearance afterwards — the same route already used to sign into the MX Bikes Shop.
  Headers alone couldn't fix this, which is why the same build worked on one connection and
  was refused on another. Honest caveat, as last release: the block isn't reproducible
  here, so it's verified by construction rather than by watching it cure the fault.
- **Browse knows what you already have.** The "Installed" badge almost never showed — one
  library scored 0 of 96 bikes, every one of them installed. It counted packed `.pkz` files
  only, missing extracted tracks and every paint, and compared post titles as exact
  strings, which they never survive. It now reads the full Library scan and matches titles
  the way a person would. (#26)
- **Downloads that Google Drive refuses now say why.** Download limit, private file and
  deleted file each come through with what happened and what to do — for a quota block,
  copy the file to your own Drive or wait a day — instead of blaming the page and sending
  you to download it manually into the same wall.
- **Presets works when your mods folder lives somewhere else.** `mxbikes.ini` can move the
  mods folder, but the game still writes profiles to `Documents`, so Presets came up blank.
  It now falls back there, and Settings shows the path it actually resolved to. (#27)
- **The empty Presets tab explains itself** — the folder it read, whether that folder
  exists, the likely cause, and a button straight to the Settings picker.
- **A slot can be cleared back to stock.** Every slot dropdown now leads with a "Stock
  (none)" row. Before, picking `full` for Protection could only be undone in the game's own
  UI or by hand-editing `profile.ini`. (#28)

### Changed
- **Library thumbnails show a bike's manufacturer logo** instead of a coloured sliver — its
  `logo.tga` was losing a tie to `team.tga`, a 32x64 strip. A real preview image still wins
  where a mod ships one, and cached thumbnails are rebuilt.
- **Hovering a name in the Library shows the full name, folder and location** — the row
  truncates hard, and the folder id is what you need to match a paint to its bike.
- **Translations can't silently go missing.** Each locale is typed against English, so a
  missing or invented key is a compile error rather than a runtime blank, and plurals go
  through `Intl.PluralRules` — French treats 0 as singular, and now so do we.

## 2026-08-06 — v0.6.3 — model swaps stop breaking bikes, Linux builds

### Added
- **Releases now say which file to download.** The release body led with "See the assets
  below", which was thin with three files and no help at all now there are six across
  three platforms. It opens with the Windows `.exe` — what nearly everyone here needs —
  and tucks macOS and Linux behind a fold, noting that the `.sig`/`.tar.gz` files are the
  updater's and shouldn't be downloaded. The Discord announcement labels the Windows link
  "start here" and gained a Linux (AppImage) link. Both describe files by extension, so
  the rename step can't make them wrong.
- **Linux builds.** Releases now produce an AppImage, a `.deb` and an `.rpm` alongside the
  Windows and macOS installers, built on a third CI leg. The AppImage isn't optional —
  it's the only Linux artifact the updater can use, so `latest.json` gets its
  `linux-x86_64` entry from it. Pinned to Ubuntu 22.04 rather than `latest`, because an
  AppImage inherits its builder's glibc as a floor and would otherwise refuse to start on
  older distros.
- **MX Bikes is found automatically under Proton.** The game runs as a Windows process
  there, so it writes into the Wine prefix —
  `steamapps/compatdata/655500/pfx/drive_c/users/steamuser/Documents/PiBoSo/MX Bikes` —
  and never touches the real `~/Documents`. Detection checks the prefix first, and now
  also finds Steam installed via Flatpak or snap, or at `~/.steam/root`.

### Fixed
- **A failed install is no longer a dead end once you've browsed away.** The failure
  toast only offered Retry, and the error itself — the message, the destination picker,
  the reinstall controls — lives on the mod's own page. Leave that page while the
  download is running and the failure had nowhere to send you back to. The banner is now
  the way back: clicking it reopens that mod's detail page, restoring the mod type the
  install targeted so Browse and the detail page agree on folders and livery routing.
  Retry and the dismiss X still do only their own job. Shop installs, which have no
  browse page, keep the plain toast.
- **Swapping a bike's model no longer takes the bike with it.** The swapper treated every
  loose file in `mods/bikes/<Bike>/` as part of the model, so a swap carted the bike's own
  setup — the `.hrc` files naming each part's mesh, plus `.cfg` and `.geom` — off into
  `FrostMod Models/`. The game then couldn't see the bike at all, which is why a swapped
  model "didn't show in game" and then the bike itself vanished from the list. A swap now
  moves only the files a swap actually provides:
  - Each parked set records what it owns in `_files.txt` on the way in, so the reverse
    swap moves back exactly what it moved out instead of guessing.
  - Before any manifest exists, the set is scoped to the meshes at the bike root plus
    whatever that bike's other swap folders contain — self-scoping, and it leaves setup
    files the swaps never mention exactly where the game expects them.
  - A swap that legitimately ships its own `.hrc` still displaces the bike's, and still
    gives it back when you swap away.
- **Bikes already broken by this can be repaired in one click.** The Locker now spots a
  bike with no `.hrc` at its root — nothing left to tell the game which mesh each part
  uses — and offers to put the missing files back from the swap folder holding them. It
  copies rather than moves, so repairing can't break the swap set they came from, and a
  bike stripped bare (what swapping to an empty "no model" variant used to do) gets its
  whole set back and its active marker corrected, rather than setup with no model. Bikes
  that carry a packed `.pkz` inside their folder are left alone — a missing `.hrc` is
  normal there, since the loose files only layer over the packed bike.
- **Bikes and swaps whose mesh isn't called `model.edf` are visible again.** A bike may
  split its mesh one `.edf` per part (`96cr250.edf`, `96cr250_st.edf`, …); the viewer
  learned that in 0.5.2 but the swapper never did, so those bikes never appeared in the
  Locker, their swap folders were never offered for registration, and applying one failed
  with "missing its model.edf". Both sides now share one definition of a bike's files
  (`bikefiles`) and accept any `.edf` as a mesh.
- **Model swaps show up in Presets.** The scan keys on the bike's folder name while the
  Presets slot asked by the `bikeid` in `profile.ini`; the two agree in case only by
  convention, and any divergence silently produced an empty dropdown. The lookup is now
  case-insensitive, for bike paints as well. Incomplete sets (files but no mesh) are no
  longer offered, since applying one could only fail.
- **Browse survives mxb-mods.com's bot protection better, and says something useful when
  it doesn't.** A user hit `403 Forbidden` on every browse and got the raw reqwest text,
  percent-encoded URL and all. The client claimed to be Chrome while sending none of
  Chrome's headers — no `Accept-Encoding` at all, since reqwest's `gzip`/`brotli` features
  weren't enabled — kept no cookies, and was rebuilt from scratch on every call.
  - One client for the session, with a cookie jar so a `cf_clearance` is replayed rather
    than arriving cold, and connection reuse so typing in the search box costs one TLS
    handshake instead of one per keystroke — the traffic shape that invites rate limiting.
  - The header set Chrome actually sends, a full four-part Chrome version (no browser
    emits the `Chrome/126.0` form we were using), and gzip/brotli enabled.
  - 403 / 429 / 503 retry with backoff instead of failing on the first refusal, and each
    maps to a plain-English message with something to do about it.
  - A Cloudflare interstitial served as a 200 no longer reads as "No download link was
    found on this page" — it says the page was intercepted.

  Honest caveat: the 403 is not reproducible from here (the current client gets 200s), so
  this is a set of well-founded improvements rather than a confirmed cure. The clearer
  error means the next report will say which of these it is.
- **Folder lookups no longer depend on how a name is capitalised.** Windows and macOS
  don't care, so hardcoded lowercase `mods` / `profiles` always resolved. Under Proton the
  filesystem is case-sensitive, and a folder the game or a mod archive created as `Mods`
  was simply invisible. Resolution now falls back to a case-insensitive match, in the one
  helper every `mods/...` path already goes through.

### Changed
- **The Locker says what to do when it finds nothing**, instead of only what's missing —
  the two conditions a swap needs, and a Scan button — and the empty "Model swap" slot in
  Presets now explains that swaps are registered in the Locker and links there.
- **New swaps get noticed.** The startup prompt to file loose swap folders used to show
  once ever; it now tracks which folders it has asked about, so a swap installed later
  still gets offered. The Locker and Presets also re-scan when the mods folder changes,
  rather than waiting for a manual Refresh.
- **Windows-only features are hidden rather than offered and broken on Linux.** The
  FrostMod section — a Win32 DLL injected into the game — no longer appears, and can't be
  asked to download two `.exe`/`.dll` files that would never run. Instant preset refresh
  explains why it's unavailable instead of saying "Windows only" to a Windows user. The
  setup screen shows the Proton path rather than `Documents\PiBoSo\MX Bikes`. The frontend
  now asks the backend which OS it's on, instead of inferring it from `navigator.userAgent`
  (which can spot a Mac and nothing else).
- **Closing the window on Linux really closes it.** Close-to-tray relies on the tray
  surviving, but Tauri doesn't receive tray clicks through libayatana-appindicator and a
  stock GNOME desktop has no tray at all — hiding there could strand the window with no
  way back.


## 2026-08-06 — v0.6.2 — mod-manager performance, Discord release announcements

### Added
- **Every tagged release now announces itself in Discord.** A new `notify` job runs after
  the installers are renamed and posts one embed to the server's release channel: the
  version and its headline, the changelog section for that version (continuation lines
  folded back onto their bullets, since the file hard-wraps mid-sentence and Discord
  renders those breaks literally), and direct Windows / macOS download links pointing at
  the finished assets. The logic lives in `scripts/notify-discord.sh` rather than inline
  YAML so it can be run locally against a published release — `--print` dumps the payload
  without sending — before it ever fires in CI.
  - Gated to `Frostn1/mxb-app` on a real `v*` tag, so forks stay silent and
    `workflow_dispatch` test builds don't reach the channel.
  - The webhook is a credential and lives in the `DISCORD_WEBHOOK_URL` Actions secret,
    never in the repo. If it's missing the job warns and passes rather than failing a
    release that already built and published fine.
- **`Join the Discord` in Settings → About & updates**, opening the community invite in
  the system browser. The invite is permanent — a link that expires would leave a dead
  button in every already-shipped build.

### Fixed
- **A large library no longer locks the machine up on first open, or after changing the
  MX Bikes folder.** Two people hit the same wall from different directions — one on the
  very first launch with a big collection, one every time they repointed the folder.
  Both are the same storm:
  - The Library renders a card per installed mod and every card asked for its metadata
    the moment it mounted. Each request opens the `.pkz`, reads its descriptor, and
    decodes the preview image to a full-size bitmap before shrinking it to a 192px
    thumbnail — so a few hundred mods meant a few hundred simultaneous archive reads and
    image decodes competing for the same disk and RAM. Cards now request metadata only
    once they scroll into view, a few at a time.
  - The backend now gates archive inspection to 2–4 at once no matter how many callers
    arrive, and caps what a single preview decode may allocate. The gate is the real
    safety net: no UI change can reopen the floodgates.
  - Metadata cache entries were keyed on the mod's **absolute path**, so pointing the app
    at a moved or copied MX Bikes folder invalidated every entry at once and re-inspected
    the entire collection in one burst. They're now keyed on the file itself (name, size,
    mtime), which survives a move.
  - A freshly scanned library pulls everything already cached in a single round trip
    instead of one request per card, so a library that's been opened before paints
    without touching an archive at all.
- **`Change…` on the MX Bikes folder no longer blocks the window** — `set_mods_path` ran
  on the UI thread, where re-detecting the Steam install and restarting the watcher could
  stall it.
- **Scanning the tracks folder walks it once, not twice.** An extracted track's folder is
  the mod, so the scan now stops descending at it rather than walking its (often
  thousands of) interior files and comparing every path against every track found so far.

### Changed
- **The folder watcher waits for a copy to finish, and says what it picked up.** It used
  to pulse a reload on every debounced burst, so dropping a folder of tracks in asked the
  game to re-scan its content over and over *while the files were still being written* —
  a plausible cause of a track that only shows up after a full game restart. It now
  accumulates changes until the folder goes quiet (3s, capped at 45s) and fires one
  reload, skips half-written downloads (`.crdownload`, `.part`, `.tmp`), and collapses
  every change inside a mod to one entry so an extracting track counts once, not
  hundreds of times. The toast names what landed.
  - Scoping the reload to just the new mods stays FrostMod's call — its reload already
    rebuilds the content lists surgically, one list per frame. All this side owes it is
    a single pulse, once the writes are done.
- **The folder-watcher toast no longer claims more than it knows.** Signalling FrostMod
  only tells us its reload event exists and was poked; FrostMod can still abort (offsets
  mismatch on an unrecognised game build) or drop the request as re-entrant. The toast
  now says the mods were *added* and that a reload was *asked for*, rather than
  announcing that the game refreshed.

## 2026-08-06 — v0.6.1 — Rider tab gear slots

### Fixed
- **Rider tab: Kit / suit, Gloves and Boot paint no longer come up empty.** Three
  separate causes, all of which left slots blank or inert in the Rider studio while
  Presets looked fine:
  - Kit / suit, gloves and profile goggles are all looked up by **rider profile**.
    Presets gets one for free when it captures the live loadout; the Rider tab started
    from an empty loadout, so every profile-keyed slot resolved to nothing. It now seeds
    the first installed rider profile on load (a preset opened via *View in Rider* still
    wins).
  - Picking a glove or kit paint changed nothing in the 3D preview, because
    `load_rider_paint` bailed outright when no profile was set. It now falls back to the
    stock `default_mx` profile, matching what the body mesh already did.
  - A loose `.pnt` dropped straight into `mods/rider/boots` (or `helmets` / `protection`)
    belongs to no model folder, and the scan silently discarded every parentless paint.
    Those now land in a shared bucket that's offered for every model of that type.
- Slot options are unchanged for Presets other than picking up the same
  previously-discarded parentless paints.

## 2026-08-06 — v0.6.0 — garage bike-switch groundwork, CI gating, first-run setup fixes

### Added
- **Garage bike-switch — cross-platform groundwork.** First slice of letting a player
  swap their whole bike mid-session (offline, restricted to the race's class) without
  relogging or an admin restart:
  - New `bikeswap` module reads a bike's id / display name / **class** (`[data] cat`)
    from its `.ini`/`.cfg` (reusing the existing `cfg` parser), with class-matching that
    mirrors the dedicated-server `[event] category` semantics (empty = Open,
    `/`-separated list) and an installed-bike scanner. Unit-tested.
  - New FrostMod **command channel**: `signal_swap_bike` writes a `frostmod_cmd.json`
    command file and pulses a dedicated `Local\FrostModCommand` event (the reload event
    is left untouched). Tauri commands `garage_scan_bikes` / `garage_swap_bike`.
  - Pairs with FrostMod **Stage A** (observation-only) in the sibling repo, which logs
    the game's bike-load calls to confirm the loader offset before any live swap.
  - Online swapping is intentionally **out of scope** — the server is authoritative and
    anti-cheat validates client integrity on join; this is offline/local only.
- **CI verification on every push and PR** (`.github/workflows/ci.yml`) — nothing checked
  a change before it landed: `release.yml` only runs on a version tag and `pages.yml` only
  deploys the site. Two jobs now run on pushes to `main` and on every PR: frontend
  (`npm ci` → typecheck → lint → build) and Rust (`cargo test --locked`) on both Linux and
  Windows, since Windows is what ships. Tests needing a real MX Bikes asset stay
  `#[ignore]`d. Verified the sidecar-less public variant — what CI actually builds —
  compiles and passes (108 tests). No `cargo fmt --check` gate yet: the tree has never
  been rustfmt'd, so that wants its own dedicated commit.

### Changed
- **ESLint actually runs clean now** — the config reported 85 errors, 72 of them
  `react/no-unknown-property` firing on react-three-fiber's three.js JSX in
  `ModelViewer.tsx` (typed by `@react-three/fiber`, so `tsc` is the real check) and one
  from a rule that was never registered. Added `eslint-plugin-react-hooks`
  (`rules-of-hooks` as an error, `exhaustive-deps` as a warning — v7's `recommended` also
  pulls in the React Compiler ruleset, which flags ~50 pre-existing patterns and deserves
  its own pass), pointed the Node-globals override at `.mjs` scripts too, and ignored
  Vite's gitignored timestamped config copies. Now 0 errors / 3 dependency-array
  warnings, so lint can gate CI.

### Security
- **Bump swiper to 14.0.7** — clears a critical prototype-pollution advisory covering
  6.5.1–12.1.1. Unlike the vite/rollup/esbuild advisories (build-time only), this one
  ships: swiper drives the mod-detail image gallery. Major bump, but the API the gallery
  uses (`modules`, `navigation`, `pagination`, `onSwiper`, `onSlideChange`, `slideTo`)
  and the `swiper-button-*` / `swiper-pagination-bullet*` class names our CSS targets are
  unchanged; no React peer-dependency constraint. Remaining audit findings are all
  build-time tooling and don't reach the shipped bundle.

### Fixed
- **First-run setup no longer replays on every launch.** Two pieces of first-run state
  were stored in places that don't reliably survive a restart, and losing either one
  put the user back at the start:
  - *The setup screen.* `is_configured` was a bare "does `config.json` exist?" check, so
    anything that took the file out — an app-data wipe, a config written under a
    different Windows account, a half-written file, a failed save — dropped the user back
    into setup with no way out but redoing it. Since that screen's default action only
    runs auto-detection anyway, startup now re-detects instead: `config::load_or_detect`
    rebuilds the config from `Documents\PiBoSo\MX Bikes` when it's recognizably an MX
    Bikes folder (has `profiles/` or `mods/`), and setup appears only when that finds
    nothing. A corrupt `config.json` is now a parse *error* that triggers the same
    rebuild, rather than deserializing to an empty config that left the app pointed at
    no folder at all. Startup logs the config path and whether it was found.
  - *The intro slideshow and guided tour.* Both were gated on `localStorage` alone, which
    the webview drops whenever its storage is cleared. They're now recorded in the config
    (`welcomeSeen` / `tourDone`), with the old keys still honored and migrated on first
    launch so nobody sees the intro twice.
- **Changing the MX Bikes folder in Settings no longer resets everything else.** Both
  "choose a different folder" and "re-detect" called `createConfig` with just the path,
  and every field the frontend omitted was refilled from `AppConfig::default()` — so
  picking a new folder also wiped the detected game install and reset launch-at-startup,
  run-in-background, auto-run FrostMod, instant refresh and the mods watcher. They now
  call a `set_mods_path` command that touches only the folder (and restarts the watcher),
  matching how `set_game_path` and the other setting commands already behaved.
- **Seven unescaped apostrophes** in the viewer's empty/error copy (`ViewerDialog.tsx`).
- **Bikes that ship one mesh per part now load in the 3D viewer** — the viewer assumed
  every bike packs all four parts into a single `model.edf`, so a mod naming its meshes
  after the bike (e.g. `MX1OEM_1996_Honda_CR250`: `96cr250.edf`, `96cr250_fs.edf`,
  `96cr250_rs.edf`, `96cr250_st.edf`) failed outright with `no model.edf for bike folder`.
  Each part's mesh is now resolved the way the game does it — through its `.hrc`'s
  `level0 { scene = … }` — and every referenced mesh is parsed and merged. Textures are
  bound per mesh file, since a submesh's material index selects from its own file's
  texture pool. Bikes sharing one `model.edf` are unaffected (verified byte-identical
  output on the stock 2023 KTM 450 SX-F).

## 2026-08-04 — docs/site copy corrections

### Fixed
- **Supported mod types** — the site FAQ, feature list, meta description and README said
  tracks only; tracks, bikes and rider gear are all first-class today (`MOD_TYPES` in
  `src/api/mods.ts`), so the copy now says so and stops naming `mods/tracks` as the single
  install target.
- **MEGA downloads** — the site and README claimed MEGA isn't automated. It is: MEGA links
  are fetched and decrypted in-app (`download_mega` in `src-tauri/src/install.rs`). Only
  MEGA *folder* links still need a manual grab, which the copy now states precisely.
- **`.rar` archives** — both the site FAQ and README said `.rar` isn't supported. It is
  (`extract_rar` via the `unrar` crate); `.pnt` files are placed as-is alongside `.pkz`.
- **Installer formats** — the site download card and README offered an `.msi`. The bundle
  targets are `nsis`/`app`/`dmg`, so Windows ships an `.exe` only.
- **Release flow** — the README (and the workflow's own header comment) said releases are
  drafted for manual publishing; `release.yml` sets `releaseDraft: false` and publishes.
- **Stale roadmap** — rider gear/liveries and self-update are both shipped, so they moved
  out of "coming next"; the download section now says the app updates itself, and the
  tech-stack list names three.js / React Three Fiber behind the 3D previews.

## 2026-08-04 — v0.5.1 — repository housekeeping

### Changed
- **Patch version bump to 0.5.1** across `package.json`, `src-tauri/Cargo.toml` and
  `src-tauri/tauri.conf.json` (plus both lockfiles). No app or runtime behaviour changes
  since v0.5.0 — this release exists to tag a clean tree after the branch cleanup below.

### Removed
- **Stale branch cleanup** — deleted 13 local branches whose work is already in `main`
  (including the `backup/pre-email-rewrite` history backup and the superseded duplicate of
  the v0.5.0 commit), and the 7 matching branches on `origin`. `feature/garage-bike-switch`
  is deliberately kept: it holds unmerged bike-switch groundwork.

## 2026-08-04 — v0.5.0 — mods-folder auto-reload, live locker swaps, showcase site

### Added
- **Auto-reload on folder changes** — a debounced watcher on `<modsPath>/mods` signals
  FrostMod to reload the game when tracks or bikes are added outside MXB App (e.g. a manual
  download dropped into the folder). Toggleable in Settings → FrostMod, on by default. Only
  the content folder is watched — never `profiles/` — so gameplay churn (replays, telemetry)
  never triggers a reload.
- **Public showcase website** — a single-page GitHub Pages landing site (`site/`) for
  prospective users: hero, feature grid, how-it-works, download CTAs, and FAQ, styled with
  the app's frost/dark brand and a hand-built UI mockup (no external assets). Deployed by a
  new `.github/workflows/pages.yml` workflow on pushes that touch `site/**`. Repo-meta only —
  no app/runtime changes.
- **Gesture hint in the 3D viewer** — a muted legend in the canvas corner (rotate / zoom /
  pan) so it's obvious the preview is draggable. Hidden while a model or paint is loading.
- **Locked archives show their real name & preview** — on builds with the optional
  decoder module, a creator-locked `.pkz` (e.g. a locked track) now surfaces its name,
  author, length and thumbnail in the library instead of an anonymous "Locked" entry. It
  stays flagged as locked (the files remain sealed — no unpack or 3D preview), and public
  builds without the module are unchanged.

### Fixed
- **Paint picker no longer sits under the close button** — the 3D preview's close button now
  sits inside the header row, vertically centred with the Paint/Goggles dropdowns instead of
  overlapping them.
- **First-run tour no longer runs behind the welcome slides** — the guided tour now starts
  only after the intro slideshow is dismissed, so its spotlights land on visible UI instead
  of hidden elements (previously it ran under the overlay and appeared to show nothing). The
  slideshow was also trimmed to just the intro; the per-feature walkthrough it used to
  duplicate is left to the tour.
- **Locker swaps now refresh live in-game** — switching a bike's model or sound in the
  Locker re-runs the game's look loader instantly (the same `instant_refresh` path presets
  already used), so the swap shows up without reselecting your profile. The swap toast now
  reports the refresh result. `apply_model_swap`/`apply_sound_swap` return a
  `SwapApplyOutcome`, and the refresh step is shared with `presets_apply`.

### Security
- **Bump postcss to 8.5.25** — pins the transitive `postcss` (pulled in by Vite) via an npm
  `overrides` entry, clearing two GHSA advisories for path traversal / arbitrary `.map` file
  disclosure through attacker-controlled `sourceMappingURL` in CSS comments. Build-time only;
  the shipped app is unaffected. (Landed after v0.4.0 was published.)

## 2026-08-04 — v0.4.0 — onboarding tour, editable presets, decoder-aware previews

### Added
- **First-run guided tour** — an interactive spotlight walkthrough that runs once on
  first launch (after Setup), highlighting the Browse, Library, Locker, Presets, Rider,
  FrostMod status, and Settings areas with anchored coach-mark bubbles and driving the
  navigation as it goes. Layered on top of the existing Welcome carousel (reused, not
  replaced), gated on a `mxb:tourDone:v1` flag so it shows once for everyone, and
  replayable anytime via a "Replay tour" button in Settings → About.
- **Per-screen help hints** — a small `?` icon beside each screen's title (Browse,
  Library, Locker, Presets, Rider, Settings) opens a popover explaining what the screen
  does. Reuses the existing popover component. The redundant inline header subtitles on
  Locker, Presets, and Rider were removed now that the same copy lives in the hint.
- **Edit saved presets after creation** — each saved preset has an Edit action that
  loads it into the builder in an explicit "editing" mode; you can rename it or change
  any slot, and saving asks for confirmation (spelling out an update, a rename, or a
  replace) before writing.

### Changed
- **Setup surfaces the MX Bikes install path** — first-run onboarding now actively
  scans for your Steam MX Bikes install (the folder with `rider.pkz`, powering the 3D
  rider preview) and shows the detected path with a "Found" badge, or a manual folder
  picker when it can't be found. The chosen/confirmed path is saved on completion. The
  path was already auto-detected silently; this makes it visible and correctable during
  install.
- **Game install path auto-detected on launch** — if the MX Bikes install folder was
  never set (e.g. the game was installed after first setup), it's now detected and saved
  on startup, so the 3D rider preview works without a manual pick.
- **Rider preview shows a loading state** — while the rider model resolves for the first
  time, the preview shows "Loading rider…" instead of a placeholder body.
- **Bike 3D preview requires the optional decoder module** — builds compiled without the
  optional local module now hide the bike 3D preview entirely instead of showing an empty
  one; official release builds include the module. Rider/gear previews are unaffected.
- **UI cleanup** — the Shop tab is hidden for now, the app title moved into the sidebar
  header (above Browse), Settings moved to the bottom of the sidebar, and the title bar
  logo was removed.

## 2026-07-29 — v0.3.2 — presets: tolerate non-UTF-8 profile.ini

### Fixed
- **Presets tab no longer errors on non-UTF-8 `profile.ini`** — MX Bikes writes profiles in
  Windows-1252/Latin-1, which isn't always valid UTF-8, so reading them failed with
  "stream did not contain valid UTF-8". Profiles are now decoded tolerantly (UTF-8 with a
  Latin-1 fallback), and applying a preset re-encodes in the original encoding so accented
  names round-trip byte-for-byte and the `.bak` stays identical to the original.

## 2026-07-22 — v0.3.1 — folder downloads, library multi-select, full-height fix

### Added
- **Library multi-select** — a new **Select** mode turns cards into checkboxes so you can
  act on many mods at once: **Uninstall** (each to the Recycle Bin), **Move to folder**
  (packaged `.pkz` items), and **Select all / none**. Reuses the existing per-item move/
  uninstall commands.

### Fixed
- **Google Drive folder links** now resolve to the mod's `.pkz` inside the folder instead
  of failing with "Google Drive returned an unexpected page". The folder listing is scraped
  and the archive is picked, skipping bundled server/source sub-folders.
- **Installs place only the `.pkz`** — when a downloaded archive bundles the client `.pkz`
  alongside a dedicated-"server" build and the unpacked track source, only the `.pkz` is
  installed; the extras no longer get dumped into the game folder. Applies to every host
  (Google Drive, MediaFire, MEGA).
- **Download origin is now accurate** — the shown mirror (Google Drive / MediaFire / MEGA /
  …) is derived from the actual link, not from an author-typed label that could read as
  something unrelated (e.g. "GoWithTheFlow").
- **Toast banners are dismissible** — added a close (✕) button and swipe-to-dismiss, so a
  persistent failed-install banner can be cleared.
- **Full app height on macOS** — the sidebar and content now fill the whole window even
  when a view's content is short. WKWebView doesn't resolve `height: 100%` against a `1fr`
  grid row, which collapsed the layout to content height; the outer shell is now a flexbox
  column.

## 2026-07-19 — v0.3.0 — sound swaps, auto-register loose sets, update banner

### Added
- **Sound swaps (sound mods)** — the Locker now manages each bike's engine sound the
  same way it manages models. A sound set (`engine.scl` + `sfx.cfg`, plus any `.wav`/
  `.mp3`) is swapped between the active loose files at the bike root and variants parked
  in `<Bike>/FrostMod Sounds/<name>/`, with an always-present **Stock** entry to revert to
  the built-in sound. Model and sound swap **independently** — switching a model preserves
  the sound (a model swap no longer drags audio along). A sound can optionally be **tied**
  to a model swap (`_bindings.json`), so it travels with that model: activating the model
  pulls its sound in, and leaving reverts to Stock. New Tauri commands `scan_sound_swaps` /
  `apply_sound_swap` / `bind_sound` / `unbind_sound`. The sidebar item is renamed
  **Model Swaps → Locker**.
- **Auto-register loose model & sound sets** — on launch the app now scans each bike for
  model sets (a folder with a `model.edf`) **and** sound sets (a folder with `engine.scl` +
  `sfx.cfg`) dropped outside their library — either straight in the bike dir or in an
  ad-hoc container folder like `models/` or `sounds/`. If any are found it offers to
  **register** them: "Register & move" relocates each into the right library
  (`<Bike>/FrostMod Models/<name>/` for models, `<Bike>/FrostMod Sounds/<name>/` for
  sounds) so they appear in the Locker, while "Just create folders" only creates the
  library folder(s) and leaves the files put. The prompt shows once then snoozes; the
  Locker keeps a persistent banner to register later. New Tauri commands
  `detect_loose_swaps` / `register_loose_swaps`.
- **Update banner** — when a newer signed build is available, a slim dismissible bar now
  appears below the title bar (`MXB App vX.Y.Z is available`) with an "Update & restart"
  button that shows live download progress. It replaces the previous transient toast for
  the "update available" case. The app re-checks every 6 hours while it's open (not only at
  launch), and dismissing a version keeps it hidden until a newer one ships. Manual "Check
  for updates" in Settings still toasts "You're on the latest version" / errors.

## 2026-07-19 — v0.2.3

### Fixed
- **Empty model swaps are now selectable** — a model-swap variant with no files (an
  intentional "no model" set) is applicable instead of greyed out. Applying it backs the
  current model into the library and leaves the bike with no model; swapping back restores
  it. Sets that have files but are missing `model.edf` remain disabled as incomplete.

## 2026-07-19 — v0.2.2

### Added
- **Custom profiles folder** — Settings can now point at a `profiles` folder that lives
  outside your MX Bikes folder (the split-folder edge case), so preset creation works for
  those players. It defaults to `<MX Bikes folder>/profiles` and appears nested under the
  mods folder as an optional customization, with a "Reset to default".
- **Automatic Steam game-install detection** — the MX Bikes install (which holds
  `rider.pkz`) is now found automatically by scanning Steam libraries, incl. extra library
  drives via `libraryfolders.vdf`, so the 3D rider preview works out of the box. Added a
  "Detect automatically" action for the install folder, plus a runtime fallback so
  existing configs benefit without reconfiguring.

## 2026-07-19

### Fixed
- **Rider gear renders textured, never smeared or blank** — helmets, boots and
  protection now bind each submesh to its own paint:
  - A helmet whose selected paint is missing no longer renders solid white (the goggle
    lens was being smeared over the whole shell); the shell falls back to the model's
    first packed paint.
  - Gear with an unknown/stale paint name (e.g. a boot paint not packed in the model)
    now falls back to the model's first paint instead of rendering flat grey.
  - Stock / "free" gear (helmet, boots, protection) is now textured — the game-pkz
    fallback path was loading the mesh but never binding its paint.
  - An unbound submesh now renders neutral grey instead of borrowing another part's
    texture.
- **Rider gear load failures are now logged** — a chosen model/paint that fails to load
  is written to the app log (instead of silently vanishing), so client-side issues are
  diagnosable.

## 2026-07-18 — v0.2.1

### Fixed
- **Helmet sits better on the rider model** — slightly smaller helmet scale so it
  proportions correctly against the body.

## 2026-07-18 — v0.2.0 — per-part bike textures, rider gear preview, library 3D quick-view

Highlights of this release (full detail in the dated entries below):
- **Per-part bike textures in the 3D viewer** — each mesh part binds its own map
  (metals, plastics, number plate, exhaust) via the model's material index, instead
  of one texture smeared over the whole bike.
- **Rider gear preview** — helmets, boots and goggles (and their paints) now render
  on the rider model, including paints from extracted (loose-folder) gear.
- **Library 3D quick-view** — a one-click 3D preview button on library items.
- **Rider / Presets reorg** — presets no longer embed a 3D preview; preview a build
  from the Rider tab instead.

## 2026-07-18 — internal cleanup

### Changed
- Trimmed verbose source comments across the codebase, keeping only short notes
  that clarify non-obvious parameters, byte offsets and invariants.

## 2026-07-18 — rider gear paints from extracted folders + goggles

### Fixed
- **Extracted (loose-folder) gear now shows its paints instead of rendering grey.**
  The paint-name match only accepted `.pkz`-style internal paths (`/paints/…`), so a
  paint in an *extracted* gear folder (`paints/…`, no leading slash) was skipped.
- **Folder helmets now load their goggle paints.** The gear-folder reader only scanned
  `paints/`; it now also reads `goggles/`, and the loadout's goggle-paint choice is
  threaded through `load_gear` so the selected goggles render.

## 2026-07-18 — helmet/boot browse shows models only

### Fixed
- **Rider browse "Helmets" and "Boots" now list only models, not paints.** The chips
  pointed at the parent categories (Helmets 33 / Boots 31), which also aggregate
  paints, goggles and addons — so models and paints were mixed. They now query the
  dedicated model subcategories (Helmet Models 313 / Boot Models 343).

## 2026-07-18 — per-part bike textures, library 3D quick-view, Rider/Presets

### Added
- **Quick 3D-view button on library items.** A 3D-cube icon on each bike / gear /
  paint card opens the 3D viewer directly (shared eligibility logic with the detail
  view's "View in 3D").
- **Save a rider look by name in the Rider tab**, plus a **"View in Rider"** button in
  the Presets builder (and on saved cards) that opens the look on the player model.

### Changed
- **Bike parts now bind their real texture from the model's per-submesh material
  index** (the `.edf` `block-4` field), so each part gets its correct map — metals,
  plastics, number plate, exhaust — instead of the largest texture smeared over
  everything. Validated across Honda/KTM/Yamaha/Suzuki/TM. Number plates stay on the
  `gfx.cfg` override; a part whose index can't be resolved renders **neutral grey**
  rather than the wrong texture.
- **Presets no longer embeds a 3D preview** — preview a build via the Rider tab.
- **Refined rider boot seating on the player model** — larger boots, seated higher off
  the leg-bottom and nudged forward, with a wider outward stance so they sit naturally
  under the legs.

### Fixed
- **Heavy paints render again.** `.pnt` paint textures are downscaled to 1024² for the
  preview (they were shipped at full 4096²), so multi-map paints no longer blow the
  webview's WebGL memory budget and fail to show.

## 2026-07-18 — viewer: boots preview orientation & framing

### Fixed
- **Boots preview now renders correctly.** Four separate defects in the gear
  preview's boot handling:
  - *Upside down.* Boots share the gear frame but their worn-up axis is the
    **opposite** of the helmet's — after `to_right_handed` negates gear X, a boot's
    leg-opening sits at ≈-0.07 and its sole at ≈-0.50 (measured from the real Fox
    Instinct mesh), so "up" is +X, the reverse of the helmet's -X crown. Boots now
    take a dedicated `BOOT_ROT` (+90° roll) instead of the helmet's `GEAR_ROT` (-90°).
  - *Boots overlapping ("smooshed").* A boots `.edf` ships both feet as separate
    nodes (`boot_l`/`boot_r`) authored coincident at the ankle; they were rendered
    stacked. New `bootSides` splits them onto left/right feet (on-body), and the
    Library solo preview separates the pair by half a boot width.
  - *Toe-in splay.* Each foot now yaws so its heel→toe axis points straight forward
    (`straightenYaw`, from the front/back 20% of the mesh), cancelling the mould's
    built-in angle.
  - *Framing.* The solo boots pair is recentred on the origin explicitly (the
    up-righted bbox sits well below its own origin), so the camera frames it instead
    of leaving it under view.

## 2026-07-18 — viewer: un-mirror gear/rider

### Fixed
- **Rider gear/helmet artwork no longer renders mirrored.** MX Bikes is a DirectX
  (left-handed) engine; bikes were already converted to three.js' right-handed frame
  (`to_right_handed`) but gear/rider meshes deliberately were not — on the assumption
  a mirror is "invisible on a helmet shell." It isn't: decal text ("Red Bull",
  "Oakley", "Troy Lee Designs") read backwards on every gear part. Gear/rider now goes
  through `to_right_handed` too, and `GEAR_ROT` flips to a −90° roll to keep the helmet
  upright under the flipped up-axis (front-back and the left-right→−X mapping are
  unchanged, so the boot mirror and seating anchors still hold). Removed the now-unused
  `orient_windings_nodes`/`orient_windings` lighting-only path.

## 2026-07-17 — viewer: preview gear paints on the stock model

### Added
- **Loose gear paints (boots / helmet / protection) preview on the game's stock
  model** (`load_stock_gear_model`). A boot paint installs as a bare `.pnt` with no
  model — the boot *model* is stock, in `rider.pkz` — so previewing one now loads
  the stock boots mesh and applies the paint, the same way a rider-outfit paint
  renders on the stock body. Wired for the `bootPaint`/`helmetPaint`/`protectionPaint`
  Library categories; outfit/glove paints keep the rider-body preview.
- A caption notes when a paint is shown on the stock model, since a `.pnt` painted
  for a *different* model (its texture name won't match the stock one) is
  force-applied and may not line up — e.g. the installed `Purple White Alpinestar
  Boots.pnt` (texture `aboots`) and `RDS Leopard GBootz W.pnt` are for boot models
  not installed, so they render on the stock boots but with mismatched UVs. Stock
  boots + a stock-named paint line up perfectly.

### Fixed
- **Rider gear LODs no longer render stacked.** Gear packs its LODs as repeated node
  names in one `.edf` (the stock boots ship `boot_l`/`boot_r` three times);
  `keep_lod0` keeps only the highest-detail node of each name wherever rider-side
  meshes are decoded.

## 2026-07-17 — browse: separate boot/protection paints from models

### Added
- **Boot Paints and Protection Paints browse filters.** mxb-mods splits each gear
  type into a model category and a paints child (Boots 31 / Boot Paints 126,
  Protection 36 / Protection Paints 135) — the same split the site's search uses.
  The Rider tab already surfaced Helmet Paints; it now surfaces all three, so a
  paint can be found without wading through models.

### Changed
- **Gear paints install onto the right model kind.** When a paint comes from a
  known paints category (`riderPaintKind`), the install destination is biased to
  that kind's installed models only — a boot paint targets a boot model (and
  falls back to the sole installed boot model, the "just installed a new model"
  case), never a helmet/protection folder. The shared per-type remembered folder
  is also ignored when it belongs to a different gear kind.

### Added
- **Goggles are textured and paint-selectable.** A helmet's `.edf` carries a
  separate `goggles` submesh, and the mod ships a `goggles/` paint folder (its own
  texture, e.g. `TLDSE4goggle`) distinct from the shell's `paints/` (`TLDSE4`). The
  viewer now binds each submesh to its own paint and adds a **Goggles** dropdown
  next to the Paint one, so the lens/strap colour can be picked independently. Six
  goggle paints + fourteen helmet paints listed for the TLD SE4, verified.

### Fixed
- **Goggles wore the helmet skin.** Gear was drawn with one material per `.edf`
  node, so the goggles submesh sampled the helmet atlas at its own UVs and rendered
  dark/garbled. Gear now builds **per-submesh** materials (the path bikes already
  use), binding `goggles`→goggle texture and everything else→the shell texture. The
  binding is by submesh name, so it holds across mods without hard-coded texture
  names.

## 2026-07-17 — viewer: the model is left-handed (mirrored bike + inside-out lighting)

### Fixed
- **`.edf` models are authored left-handed (DirectX); three.js is right-handed**
  (`edf.rs::to_right_handed`, applied in `main.rs` after `assemble_bike`). Feeding
  those coordinates straight in mirrored the model, which caused two bugs that
  looked unrelated:
  - **Mirrored artwork** — "HONDA" on the seat and "CRF"/"450R" on the shrouds
    rendered back-to-front (the reported "seat is flipped").
  - **"Holes" / dark facets** — a mirror inverts triangle orientation, so against
    the model's own normals **100.0%** of the Honda's `chassis`/`fsusp` triangles
    read as back-facing. Backface culling was never involved (the viewer renders
    `DoubleSide`, so nothing ever vanished): `DoubleSide` lighting does
    `normal *= gl_FrontFacing ? 1 : -1`, so every normal was negated and the whole
    bike was lit from the inside. The geometry was complete the entire time.

  Negating X on positions + normals fixes both at once, and the winding then agrees
  with the normals with no re-winding. Applied *after* assembly deliberately — the
  `.geom` mounts and rake rotations are authored in the game's frame, and mirroring
  X inverts a rotation about X. Proven by software-rendering the real Honda: the
  text reads correctly and the black facets resolve into solid red bodywork.
- **Rider/gear lighting** (`edf.rs::orient_windings_nodes`). Gear shares the same
  left-handed convention (boots 100.0%, TLD SE4 helmet 99.9% back-facing) and was
  lit inside-out too, but it's authored X-up and the viewer bbox-fits it with
  anchors tuned to the un-converted frame — so its winding is corrected for
  lighting while its geometry is left alone.
- Confirmed **`flipY = false` is correct** and left unchanged. Both `.pnt` and
  embedded `model.edf` textures are stored bottom-row-first, and `flipY = true`
  drags the atlas's engine-metals region onto the bodywork. The mirrored text came
  from the mesh, not the texture.

### Added
- **Paints that don't fit the model are now labelled** instead of silently doing
  nothing (`BikePaint.appliesToModel`; dropdown shows "— not for this model" plus a
  note on the canvas). A `.pnt` replaces a model texture *by name*, which is the
  whole mechanism: the Honda binds `2021crf`/`w_plate`/`chain`, so its own
  `stock.pnt` (ships `chain`, `wheel`…) applies, while `#96_CR450F'26.2HRC_TRD.pnt`
  ships only `plastics`/`plastics_n` — it's painted for a '26 HRC model-swap body
  kit that isn't installed, binds nothing, and the game wouldn't apply it either.

## 2026-07-17 — viewer: `.edf` indices start at `ic+4` (the root of every mesh artifact)

### Fixed
- **`.edf` index buffers are read from `ic+4`, not `ic+8`** (`edf.rs`). There is no
  flag word after `tri_count`: the zero read there is `idx0`, which is 0 because
  every node's first triangle is `(0,1,2)`. The off-by-one validated itself —
  skipping `idx0` at the front and eating the trailing `submesh_count` at the back
  landed the name anchor exactly — so blocks "checked out" while every triangle was
  built from a shifted index window. This was the single root cause behind the
  shattered/faceted gear, the streaks, the holes and the half-rendered rider.
  Proof: the decode now yields exactly `tri_count` triangles with zero degenerates
  (stock helmet 4120/4120, TLD SE4 6318/6318, boots 1950, armour 2922), and the
  rider body renders as a clean, complete figure.

### Removed
- **The strip decoder, the degenerate-ratio heuristic and the UV-span streak
  filter** — all three existed only to compensate for the bad offset. There is no
  strip encoding in this format; bikes, gear and the rider body are all plain
  triangle lists. `parse_rider` is gone (callers use `parse`), and the UV-span rule
  is gone with it — it was deleting real geometry.

## 2026-07-16 — viewer: read the bike's configs instead of guessing

### Added
- **`gfx.cfg` + `.hrc` are now loaded** from a bike (`cfg.rs`). They ship as plain
  text inside the `.pkz` and state outright what the viewer used to infer: which
  node is a part's full-detail mesh, and which mesh group binds to which texture.
- **Every texture packed in a `model.edf` is extracted under the model's own name**
  (`2021crf`, `exhaust_22`, `w_plate`; `plastics`, `450f_metals` on the KTM) instead
  of keeping only the largest and renaming it `albedo`. Those names are the whole
  binding mechanism, and collapsing them threw them away.

### Changed
- **LOD selection is `.hrc`-driven.** A `.hrc` names `level0` and its LOD variants
  outright, so the old heuristic (strip a `b`/`c` before the first digit, tiebreak on
  triangle count — which once silently flipped the KTM 450 onto its un-placeable
  LOD-B chassis) is now only a fallback for a loose `.edf` with no configs.
- **Texture binding moved to Rust and is driven by the bike's files**, not by regex
  over mesh-group names in the viewer. `gfx.cfg`'s `texture = …` overrides win
  (`plate → w_plate`, `chain → chain`); everything else takes the model's primary
  diffuse; a paint replaces a model texture of the **same name**. The viewer now just
  looks the resolved name up.
- Bike textures use `RepeatWrapping` — the number plates' UV islands run outside
  0–1, and the Honda's exhaust is authored on UV tile 1.

### Fixed
- **Bike paints smeared across the whole bike** (rider number in the wrong place, the
  paint map dragged over the rear fender). The viewer forced *every* mesh group to
  the paint's `plastics`, so a paint authored for a different model was stretched
  over parts it was never drawn for — engine, forks, swingarm and all. A paint only
  applies where the model names a texture the paint carries; a stock Honda's body
  texture is `2021crf`, so its `'26 HRC` paints (drawn for a swapped model) correctly
  leave it alone now, rather than being smeared over it.
- **The exhaust wore body graphics.** The Honda's exhaust is authored wholly on UV
  tile 1 (u ∈ [1.001, 2.000]), which selects a second texture (`exhaust_22`) sampled
  at `u - 1`. Verified by rendering: it now reads as brushed metal.

## 2026-07-16 — v0.1.6

### Fixed
- **Gear mods rendered as shattered polygons**: a mesh's index buffer is either a
  stitched triangle *strip* or a plain triangle *list*, and content exports both —
  PiBoSo's own gear/rider are strips (~50% of their triples are degenerate
  stitches), while e.g. the `TLD SE4` helmet mod is a list (6%). Decoding a list as
  a strip invents ~3.7 triangles per vertex of garbage (a closed mesh is ~2), which
  is exactly the shattered surface and streaks. Each node now picks from its own
  degenerate ratio instead of being told by the caller.
- **Gear rendered lying on its face**: helmets/boots/protection are authored
  **X-up** (a helmet extends up from an origin at the neck), not Z-up like bikes or
  Y-up like the rider body — so they need a roll about Z, not the bike's X flip.
  Verified by rendering the mesh down each axis.
- **Gear previewed at a top-down angle**: the viewer's camera sits high to look over
  a bike; a single gear item is small and centred, so it now gets a level view.
  (The move also has to go through OrbitControls, which owns the camera and was
  silently reverting it.)
- **Packaged `.pkz` mods extracted as empty files**: an archive written *streaming*
  leaves its per-entry sizes zero in the local file header (they live only in the
  central directory), so every entry came out 0 bytes — a packaged gear mod looked
  like it simply had no model. Sizes are now read from the central directory, with
  the local header as fallback. Verified on a real 30 MB helmet mod (`helmet.edf`
  0 → 9.1 MB) with no change to OEM bike/track archives.
- **Installed gear mods never rendered**: the gear loader only looked for an
  *extracted folder*, but gear installs as a packaged `.pkz` — and the Library only
  passed bikes to the 3D viewer, so opening a helmet showed the rider body wearing
  that helmet's paint. Gear now loads from a folder **or** a `.pkz`, and a
  helmet/boots/protection item opens as a standalone 3D preview of that piece.
- **Paint colours were channel-swapped everywhere**: `.pnt` pixels are stored
  **RGBA**, but the decoder swapped them as if they were BGRA — turning every
  paint's navy into brown and red into blue, on bike liveries as well as rider
  kits. Proven against PiBoSo's own stock `white_navy.pnt`, whose navy only reads
  as navy unswapped. Pixels are now returned verbatim; added an `#[ignore]`
  real-file guard (`stock_white_navy_decodes_navy_not_brown`) so it can't regress.
  (The old `libpnt` fixture uses the opposite order to the real game files, so it's
  no longer treated as ground truth for channel order.)
- **Rider body rendered as a shredded half-mesh**: skinned models store their
  indices as stitched triangle **strips**, not lists — reading them as a list
  recovered only ~1/3 of the surface, wrongly grouped. New `edf::parse_rider`
  strip-decodes them (rider body: 2,840 → 11,701 triangles, a solid figure), while
  bikes/gear keep the list decode (`edf::parse`) their non-submesh parts need.
- **Rider paint mapped as noise**: the `.edf` UV block is a single 2-float set
  (**stride 8**), not two sets at stride 16 — the old read sampled every *other*
  vertex's UV. Per-triangle UV span dropped 0.44 → 0.053.
- **Paints were mapped upside-down**: MX Bikes is a DirectX game, so its textures
  use a **top-left UV origin**; three.js's default `flipY` mirrored them, making
  the torso sample the pants region ("the rotation is off"). Now `flipY = false`.
  Proven on stock `white_navy.pnt`: flipped it renders an asymmetric kit (one leg
  yellow, one navy), unflipped a correct symmetric one. Affects bike liveries too.
- **Streaky "lines" across the rider**: strip-transition triangles whose vertices
  straddle different UV islands smear the atlas across the model. They're often
  short in 3D, so the existing long-and-thin sliver test missed them; they're now
  dropped by a UV-span test (a real triangle covers ~0.03 of the sheet, these span
  0.85–1.0). Strip decode only, so bikes are untouched.
- **Rider preview was slow**: the body was re-read from the 105 MB `rider.pkz` and
  re-parsed (28 MB `.edf`) on every open; now cached per profile.

### Added
- **Full-bundle preset sharing ("they have nothing" import)**: a preset can now be
  shared as a complete asset bundle, not just a config code. **Create full bundle**
  in the Share dialog resolves every asset the loadout references (liveries, gear
  models + paints, gloves, outfit, tyres, model-swap variants) via `scan_library`,
  zips them into a `mods/`-mirrored tree plus `preset.json`, uploads it to an
  anonymous host (pixeldrain), and returns a share code with the download link
  embedded. **Full import** on the other end downloads the bundle (reusing the
  Google Drive / MediaFire / Mega / direct download + `place_mod` pipeline) and
  installs every file into the correct `mods/` subfolder — so a recipient who owns
  none of the mods still gets the complete look. New Rust modules `bundle.rs`
  (resolve/build/import) and `upload.rs`; `preset_bundle_stats` /
  `preset_bundle_create` / `preset_bundle_import` commands. Backward-compatible: an
  optional `bundle` field on `Preset`, so legacy `MXBP1-` codes still decode. The
  Share dialog previews bundle size + which slots can't travel and notes the link
  is public/temporary; free-text fonts and stock/uninstalled slots aren't bundled.
- **3D preview in Presets (bike + rider)**: a live 3D panel renders the current
  loadout — the real bike model decoded from its `.edf`/`.pkz`, its livery, and the
  rider's installed gear (helmet/boots/protection meshes + suit/gloves paints) — and
  updates as you change slots. Native decoders (`edf`, `paint`) mean no external
  tools. Optional **game install folder** setting (Settings → MX Bikes folder) points
  at the MX Bikes install so the real rider **body** (`rider.pkz`) can load.

### Changed
- Heavy backend commands (model/paint/library loads, `.pkz` decode) now run **off
  the UI thread** (`async` + `spawn_blocking`), so opening the viewer or library no
  longer freezes the window and a malformed asset returns an error instead of
  crashing.

### Fixed
- **More paints render in the 3D viewer**: some `.pnt` paints are packaged in a
  non-plain container rather than a plain `PNT\0` file. The paint decoder now
  handles both transparently (`paint::decode_any`), used everywhere paints are read.
- **Much faster 3D bike-model load**: textures encode with fast deflate and in
  parallel (`rayon`) instead of serial max-compression PNG — big bikes load quickly.
- **No more freezes / blank white screens**: added a global + canvas `ErrorBoundary`
  with a WebGL context-loss handler, so a render error shows a recoverable panel
  instead of an unrecoverable white screen.
- **Rider body no longer see-through**: rider meshes render double-sided
  (`THREE.DoubleSide`), so the body reads as solid.
- **Reliable `tauri dev` startup**: a `predev` step frees the Vite port, and dev
  builds fully quit on window close (release builds still hide to tray) so the port
  isn't orphaned.

## 2026-07-15 — v0.1.5

### Added
- **Instant preset refresh (Windows)**: applying a preset while MX Bikes is
  running now updates the bike's look **live** — no game restart and no manual
  profile reselect. It re-runs the game's own profile loader in place (found by
  reverse-engineering `mxbikes.exe`). **On by default**; toggle under
  **Settings → General → Instant preset refresh**.
- **Honest apply feedback**: the apply toast now says exactly how it took effect
  — refreshed live, "reselect your profile in-game to load it" (while the game is
  open), or "loads next launch" — instead of implying a FrostMod content reload
  already applied the new look.

### Changed
- Instant refresh lives in **Settings** (default on), not as a toggle inside the
  preset menu, so it doesn't alarm players mid-customization.
- `presets_apply` returns a richer `PresetApplyOutcome` (`content_reload`,
  `game_running`, `live_refresh`); new `gameproc` module handles game detection
  and the in-place loader call; new `instant_refresh` setting persists the choice.

### Fixed
- **FrostMod update no longer fails with "file in use"**: updating FrostMod from
  the app now **stops** the running FrostMod first, overwrites
  `frostmod.exe`/`.dll` (with a short lock-release retry), then **restarts** it —
  so updates are seamless instead of erroring because the files were in use.

## 2026-07-15 — v0.1.4

### Added
- **Sound mods visible in the Library**: installed bike sounds now appear as
  their own **Sound** entries. Because a sound merges into an OEM bike folder
  (indistinguishable from stock on disk), the app records provenance at install
  time (`soundmods` store → `sound-mods.json`) and the Library surfaces exactly
  those bike folders that still carry the sound files — no guessing, no
  mislabeling stock bikes. New `sound` library category (label/icon).
- **Auto-pick the right sound download per bike**: sound-mod pages list a
  *different* download per bike ("Just KTM 250SX-F") plus a "Main pack with all
  bikes" default — these are **not** mirrors. The install dialog now treats them
  as per-bike options, auto-selecting the link that matches the chosen bike (and
  falling back to the all-bikes pack), instead of claiming "all mirrors contain
  the same file". New `pickDownloadForBike` + `isSoundContext`/`SOUND_CATEGORY_ID`.
- **Presets — customization loadouts**: new Presets tab that saves a full look
  (bike livery, number/suit fonts, tyres, rider kit, helmet + paint, goggles,
  gloves, boots + paint, protection + paint, riding style, race number) and applies
  it to a bike on command. MX Bikes keeps the selected look **per bike** in
  `profiles/<profile>/profile.ini` (one section per slot, keyed by bikeid); a preset
  is a bike-agnostic bundle of those values. Capture a bike's current look or build
  one from installed mods (dependent pickers — helmet paints follow the chosen
  helmet, etc.), save it named, and quick-apply — writing only the target bike's
  rows (with a `profile.ini.bak` backup) and nudging a running FrostMod to reload.
  A preset can also carry a **model swap** (applied via the Locker's model-swap
  machinery). **Share** presets as portable `MXBP1-…` codes (copy/paste) that others
  **Import**, with a missing-mod warning for anything they haven't installed. New
  line-oriented profile.ini editor (`presets` Rust module) that rewrites only the
  targeted `<bikeid>=` lines, `presets_*` Tauri commands, and a `presets.json` store.
- **Model Swaps — in-app bike model swaps**: new Model Swaps tab that mirrors FrostMod's
  in-game model swapper (F8 > 3) from the app. Lists each swappable bike (a folder
  with a loose `model.edf` **or** a `FrostMod Models/` library — so a bike whose
  active Original is `.pkz`-packed still shows and stays reachable), its active
  model, and every alternate set under `<Bike>/FrostMod Models/`, and lets you
  switch between them — the same backup-current / move-in-chosen file dance (whole
  loose set, `paints/` left put, with rollback) and `_active.txt` marker FrostMod
  uses, so the two stay interchangeable. Signals a running FrostMod to live-reload
  after a swap. New `scan_model_swaps` / `apply_model_swap` Tauri commands (Rust
  `modelswap` module).
- **Silent FrostMod setup**: FrostMod now installs and starts automatically on
  first run instead of showing a "Set up FrostMod?" prompt. Added a manual
  re-check button next to the FrostMod row in Settings.
- **In-app MEGA downloads**: MEGA public file links now install directly in the
  app (fetch + decrypt via the pure-Rust `mega` crate on the existing reqwest
  client) with the same progress stages as other hosts — no browser round-trip
  and no external megatools/MEGAcmd binary required. Folder links still fall back
  to manual browser download.
- **In-app MediaFire downloads**: MediaFire file links install directly in the app
  again. Verified empirically (full 427 MB `.pkz`) that MediaFire's CDN no longer
  blocks the rustls client, so `resolve_mediafire` + the normal download path
  handle them — the old "CDN blocks non-browser TLS" workaround no longer applies.

### Changed
- **MX Bikes Shop installs route by mod type**: a purchased download no longer
  always lands in `mods/tracks`. A structured archive (a `mods/` tree, top-level
  `bikes/tracks/rider/…`, or a `<Bike>/paints/` livery bundle) now self-routes by
  its own folders — the livery-bundle case works regardless of the caller's
  default type — and content that can't be inspected (a locked `.pkz`) picks
  its bucket from the item's title (`guess_mod_type`) instead of assuming tracks.
- **FrostMod update check**: Settings now re-checks FrostMod against GitHub when
  it opens (and when the About "Check for updates" button is pressed), so a newer
  release surfaces an "Update to vX" button instead of a stale "Up to date".
- MEGA and MediaFire are no longer treated as "blocked" hosts in the install UI,
  so their mirrors get the in-app install button instead of the
  download-and-import fallback (`BLOCKED_HOST_PATTERNS` is now empty).

### Fixed
- **Sound mods no longer routed into a bike's `paints/`**: bike **sound** mods
  (`engine.scl` + `sfx.cfg`, plus audio samples) install to the bike-folder
  **root** (next to `paints/`), never inside it. The install picker now offers a
  per-bike **root** destination and defaults sounds to the name-matched bike; the
  Rust placer gained a sound-bundle guard that strips a stray `paints` segment so
  loose sound files can't be misfiled (with new placement tests). Well-packaged
  mods that carry a full `mods/bikes/…` tree already merged correctly — this also
  removes the misleading "install to paints" the dialog used to suggest.
- **FrostMod "up to date" false positive**: a failed or offline GitHub check no
  longer displays as "Up to date". The panel now distinguishes *Checking…*,
  *Couldn't check* (offering "Reinstall latest"), and a confirmed-current install,
  so users aren't told they're current when the check simply didn't run.
- **MediaFire link resolution**: replaced the stale `id="downloadButton"` fallback
  regex (which matched nothing on today's pages) with the current
  `aria-label="Download file"` link inside `#download_link`.
- **Bare `.pnt` paints install**: mods shipped as a loose `.pnt` file (not zipped)
  now pass through extraction like `.pkz` does, instead of failing with
  "Unsupported archive type". More common now that MEGA links install in-app.

## 2026-07-15 — v0.1.3

### Fixed
- **Kaizo servers no longer hidden from the browser**: the app now manages
  FrostMod's `frostmod_serverfilter.yaml` in the FrostMod folder. FrostMod's stock
  default filter blocked Kaizo (a `kaizo` name rule + a `k[a4][il1]z[o0]` spam
  regex); we now write a curated `# frostmod-filter v4` config that keeps the
  ad/cheat-shop spam rules but drops the Kaizo matches. Written on FrostMod
  install/update and refreshed before each managed launch, so existing installs
  get corrected automatically; a filter the user has hand-edited is left untouched.

### Removed
- **Locker (experimental 3D bike-livery viewer)**: removed the Locker scene and its
  sidebar/dashboard entries; the feature is dropped for this release.

## 2026-07-15 — testing feedback pass

### Added
- **Full library detail view**: clicking any installed track/bike/gear card opens
  a dedicated detail page — large preview (**click to enlarge in a lightbox**),
  all parsed metadata (name, author, length, altitude, location), format, size and
  on-disk path, plus quick actions (Move / Show in Explorer / Uninstall). Backed by
  a new `get_pkz_preview` command that returns a full-resolution preview (the card
  thumbnail stays small); `pkz` internals refactored into a shared `inspect` used
  by both.
- **Extracted-folder tracks now appear in the library**: tracks installed as loose
  files (not a single `.pkz`) are detected by their track markers (`.map`/`.trh`/…)
  and shown as one item with name/author/preview read from their loose `.ini` — the
  old scan only listed `.pkz` and silently dropped these.
- **Every rider category is now visible**: the Rider (player) library groups by
  category — Helmets, Helmet Paints, Goggles, Boots, Boot Paints, Protection,
  Gloves and Outfit/Kit — surfacing loose paints/gloves/goggles/outfit that the old
  `.pkz`-only scan hid (only helmets/boots showed before). Each item carries its
  info/thumbnail where readable.
- **Bike detail shows its liveries + model swaps**: a bike's detail view lists the
  paints in `<Bike>/paints` and any model-swap `.pkz` inside the bike's folder;
  gear models likewise list their paints/goggles. Backed by a richer
  `scan_library` command (kind/category/parent per item) used by the library while
  install pickers keep the leaner `get_installed_mods`.

### Fixed
- **New bikes no longer install into a bike's `paints` folder**: only actual bike
  **liveries** (WP category 37) default/route into `<Bike>/paints`; new bikes,
  sounds and unknown bike content default to `mods/bikes` root, and a remembered
  livery `paints` folder is no longer inherited by a subsequent new-bike install.
- **Install dialog no longer clips its own header/X**: the dialog is capped at
  `85vh` with a scrolling body, so expanding the folder picker can’t push the modal
  past the viewport and hide the close button.
- **Guard against accidental reinstalls**: quick-install, bulk-install and "Add to
  Library" now show an "are you sure — this overwrites the installed files" confirm
  when the mod is already in your library.

### Changed
- **Removed the retired "Wheels" bike browse category** (id 95) — it no longer
  maps to real content.
- **Uninstall works on extracted-folder mods**, not just `.pkz` files (moves the
  whole folder to the Recycle Bin).

## 2026-07-15

### Changed
- **v0.1.2 release** — bumped version across `package.json`, `tauri.conf.json`
  and `Cargo.{toml,lock}`.
- **About credits trimmed** to a single "Frost" credit (links to
  github.com/Frostn1); removed the Blarne / "Long live MXBMM" lines.
- **All app state now lives in one Local AppData folder**: config, shop session,
  and the FrostMod install moved from Roaming to
  `%LOCALAPPDATA%\com.frost.mxbikes\` (joining the existing cache), so everything
  is in one per-machine place. No migration (pre-release) — old Roaming files are
  simply re-created on next launch.

### Added
- **Rider content**: a new **Rider** browse section (Rider Kit, Helmets, Helmet
  Paints, Gloves, Boots, Protection) installing into `mods/rider`. Paints route to
  the right place automatically — helmet/boot/protection paints into their model's
  `paints`/`goggles` (pick the installed model, name-matched like bike liveries),
  and rider outfit + gloves into the rider **profile** you choose
  (`riders/<profile>/{paints,gloves}`, scanned from your install via a new
  `scan_rider_targets` command).
- **File logging**: added `tauri-plugin-log`, writing to
  `%LOCALAPPDATA%\com.frost.mxbikes\logs\`. Startup logs the app version and the
  data/log dir paths, and shop session/login/download failures are now logged.

### Added
- **First-launch welcome tour**: a 3-slide intro overlay (what MXB App is →
  browse & install → FrostMod) shown once on first launch before folder setup,
  tracked via a `mxb:welcomeSeen:v1` localStorage flag. New
  `Components/Welcome/Welcome.tsx`.
- **Windows executable publisher & metadata**: the installer and the `.exe`
  version info now carry a publisher ("Frost"), copyright, homepage and
  description so Windows shows a proper publisher/details instead of blanks.
  Set via `bundle.publisher`/`copyright`/`homepage`/`shortDescription`/
  `longDescription` in `tauri.conf.json`. (Does not replace Authenticode code
  signing — SmartScreen may still warn until the exe is signed.)

- **Rich library cards from inside the `.pkz`**: plain-zip tracks and
  bikes now show their **real name, author, length and a preview thumbnail** read
  straight from the archive's `.ini` and preview image, plus the **file size** on
  every card. Preview images (often TGA, which browsers can't render) are decoded
  and downscaled to a small JPEG in Rust. **Locked `.pkz` are
  detected and skipped gracefully** — they show a lock badge with just name + size.
  Parsing is lazy per card (list paints instantly) and cached to disk. Backed by a
  new `get_pkz_meta` Tauri command + `pkz` module (`image`/`base64` crates), with
  `size` added to the `InstalledMod` model.

- **MX Bikes Shop downloads**: a new **Shop** tab lets you sign in to
  mxbikes-shop.com and install the tracks you've **already purchased**
  ("All My Downloads") with the same one-click download → extract → place flow and
  "Installed" badge as Browse. Sign-in happens in a real WebView window (your
  password never touches the app); the captured session is persisted so you stay
  logged in across restarts, with a Log out button. Backed by new `shop_*` Tauri
  commands, an authenticated shared `reqwest` client, and a reusable
  `download_and_place` install helper shared with the free catalog.

### Fixed
- **Install destination picker for bike liveries**: the folder list is now
  **scrollable** and no longer overflows the popup, long bike names **truncate**
  instead of cutting off, and it's a **command-style search** — probable bikes
  (matched from the mod name) show under "Probably" at the top, with a search box
  to find any bike.

### Added
- **Start FrostMod without restarting the app**: if FrostMod isn't running, a play
  button appears on the sidebar status pill and in Settings → FrostMod to launch it
  on the spot.

### Added
- **FrostMod is managed in-app**: MXB App now **downloads FrostMod** from its GitHub
  releases, **runs `frostmod.exe`** hidden in the background so it's connected as
  soon as the app opens, and **updates** it — no manual setup. Settings → FrostMod
  shows the installed vs latest version with an Install / Update button and a
  "Run FrostMod automatically" toggle, and a first-run prompt offers to set it up.
  The managed process is stopped on a real Quit. (Injector is Windows-only; the
  manager no-ops elsewhere.)
- **Runs in the background like Discord**: closing the window now hides MXB App to
  a **system-tray icon** (Show / Quit menu) instead of quitting, so it keeps running
  and FrostMod stays connected. **Launches at login** by default. Both are
  toggleable in Settings → **General** ("Keep running in the background", "Launch at
  startup"). Backed by a tray icon + `WindowEvent::CloseRequested` intercept and the
  `tauri-plugin-autostart` plugin; prefs persist in the app config (default ON).
- **"Made with ❄ by Frost"** credit in Settings → About, linking to the author.

### Changed
- **Release assets get clean names**: a CI finalize step renames the ugly
  `MXB.App_0.1.0_x64-setup.exe` to `MXB-App-0.1.0-x64.exe` (and the `.dmg`
  likewise) and repoints `latest.json`, so downloads look trustworthy. Signatures
  are over file content, so self-update still verifies.

## 2026-07-14

### Added
- **Windows install wizard**: the Windows build now ships a branded **NSIS**
  installer (welcome → license → install → finish) instead of a bare bundle.
  Installs **per-user with no admin/UAC** prompt, uses the snowflake app icon, and
  shows the MIT license. Configured in `tauri.conf.json` (`bundle.windows.nsis`);
  MSI dropped from the targets.
- **Auto-update**: the app checks GitHub Releases on launch (quietly) and offers
  **"Restart & update"** via a toast when a newer signed build exists; a manual
  **Check for updates** button lives in Settings → About. Backed by the Tauri
  `updater` + `process` plugins, signed release artifacts (`createUpdaterArtifacts`),
  and a `latest.json` published by CI. Requires the `TAURI_SIGNING_PRIVATE_KEY`
  secret and a published release to take effect.
- **App icon**: a snowflake mark on an icy gradient badge, generated into
  `src-tauri/icons/*` (`.ico`, `.icns`, PNGs) — this is what shows on the
  taskbar/dock and the `.exe`. The in-app UI is unchanged.
- **Platform-adaptive title bar**: on macOS the window now uses native
  decorations with `titleBarStyle: "Overlay"` (new `tauri.macos.conf.json`), so
  it gets real traffic-lights, rounded corners and the native shadow, and our
  custom window buttons are hidden. Windows keeps the frameless custom title bar
  and its Windows-style controls, unchanged.
- README: roadmap entries for **bike + rider liveries** and **auto-update**.

### Fixed
- The product name still read "MXB App by Frost" in `productName` (the name shown
  on the taskbar and the installed `.exe`) and in the window title — both are now
  **MXB App**. Remaining in-app copy that called the app "Frost" (title bar,
  Setup, install/blocked-host text) now says **MXB App**. (FrostMod, the separate
  live-reload tool, keeps its name.)

### Changed
- README tech stack updated to Tailwind + shadcn/ui + lucide + Sonner (was MUI).

## 2026-07-13

### Added
- **UI redesign**: a dark, Apple-clean rebuild of the whole UI on Tailwind +
  shadcn/ui, replacing MUI. A permanent **left sidebar** (Browse / Library /
  Settings) with a live install badge, a persistent **global install indicator**
  and **FrostMod status pill** that survive navigation, and the game path.
- **Settings screen** (new): game folder (change / auto-detect + re-scan),
  appearance (Light / Dark / System theme), FrostMod status + reload, and about.
- **Install popup** on "Add to Library": pick the destination folder (with mod
  counts, remembered per category) and choose a download mirror (default
  pre-selected, browser-only hosts flagged) before installing.
- **Toast notifications** (bottom-right) for install success/failure and
  uninstall, replacing inline alerts.
- **Library actions**: per-mod context menu with Move to folder, **Show in
  Explorer**, and **Uninstall** (moves the file to the Recycle Bin via new
  `reveal_in_explorer` / `uninstall_mod` Tauri commands + the `trash` crate).
- **Mod Detail** right-rail install surface with a live stage chain
  (Resolve → Download → Extract → Place → Reload) and a guided 2-step
  blocked-host flow for browser-only mirrors.
- README release badges: latest release, release date, and total download count
  (dynamic via shields.io, GitHub-backed), plus MIT license and Windows x64
  platform badges. Added a root `LICENSE` file (MIT).
- **FrostMod live-reload integration**: when you add a mod, the app now signals a
  running [FrostMod](https://github.com/Frostn1/frostmod) to re-scan the mods
  folder so new tracks/bikes appear in-game without a restart. Works by setting
  FrostMod's own `Local\FrostModReload` Windows event (the same trigger as
  pressing **R** in its console) — no changes to FrostMod required. The mod
  detail view shows whether FrostMod picked it up live or isn't running, and new
  `frostmod_reload` / `frostmod_running` commands back a manual trigger + status.

- **Right-click actions**: right-clicking a mod in **Browse** offers *Quick
  install*, *Open details*, and *Select*; right-clicking a row in **Library**
  opens the same Move / Show in Explorer / Uninstall menu as the 3-dot button.
- **Quick install**: installs a mod straight from Browse with no detail page and
  no dialog — it resolves the best direct mirror and reuses the remembered (or
  auto-guessed) destination folder, then reports where it landed via a toast.
  Browser-only hosts (MediaFire/Mega) can't install silently and are skipped
  with an explanation.
- **Multi-select + bulk install** in Browse: select mods via the card checkbox or
  the right-click menu, then *Quick install N* from the selection bar
  (with *Select all* / *Clear*).
- **Install queue**: installs still run strictly one at a time, but extra
  requests now queue and drain in order, with a "+N queued" line on the sidebar's
  install card.

### Fixed
- Mod Detail screenshots rendered squashed: the gallery and thumbnail strip are
  flex children of a scrolling column, so they were being **shrunk** instead of
  scrolled and lost their 16:9 height. Pinned them with `flex-none`.
- The **GitHub / Changelog links in Settings** pointed at a non-existent
  `Frostn1/frost` repo — corrected to `Frostn1/mxb-app`, and the About line now
  reads "mxb-app" rather than the old product name.
- MediaFire mods were mis-detected as auto-installable because the host label is
  written "Media Fire" (with a space) — downloads are now classified by **URL**,
  so blocked hosts correctly open in the browser instead of failing.

### Changed
- Navigation moved from top tabs to the left **sidebar**; the theme toggle moved
  from the title bar into Settings → Appearance. **Setup** is now a single step.
- Clearer download UI: one **official one-click** option; other links are labeled
  (a dedicated-**server** build is called out as "not needed for normal play"
  rather than "mirror"); the **Import** step only appears when a blocked host is
  used.
- Enabled **text selection** and added a **Copy** button on error messages.

### Removed
- MUI, Emotion, and all per-component SCSS; the top-tab `Header`, the `Footer`,
  and the old `LoginPage`/theme are replaced by the sidebar shell, Settings, and
  a token-based Tailwind theme.

## 2026-07-12

### Added
- **Release CI** (`.github/workflows/release.yml`): tagging `v*` (or a manual
  dispatch) builds Windows + macOS bundles with `tauri-action` and attaches the
  installers to a draft GitHub Release.
- **Import a file**: for hosts that block in-app downloads, open the download in
  the browser then import the downloaded file — the app extracts and places it
  into the right folder just like a normal install (`import_file` command).
- Download retries and full error-cause reporting on failed installs.

### Fixed
- Diagnosed installs failing with "error sending request for url …":
  **MediaFire's download CDN blocks all non-browser TLS clients** (verified
  across rustls, native-tls/SChannel, curl and Python — only real browsers get
  through). No TLS backend can bypass it, so MediaFire/Mega now fall back to
  browser download + Import. Auto-installable hosts (**Google Drive**, direct
  links) are shown first as the one-click option.

### Changed
- README: added Download, build-status badge, and Releases (how to cut one)
  sections.
- Renamed the app to **MXB App by Frost** (window title, title bar, header).
- Replaced the macOS traffic-light window buttons with **clean Windows-style
  controls** (minimize / maximize / close, red close hover).
- The Library now scans **recursively** and lists every installed `.pkz` with
  its sub-folder, so tracks/bikes nested inside folders show up.
- Kept **rustls** TLS (native-tls's SChannel failed the handshake on Windows).

## 2026-07-06

### Added
- **Browse & search** mods from mxb-mods.com in-app, with category filters,
  **"Load more" pagination**, and a mod detail page with an image gallery.
- **Add to Library**: one-click download → extract → place into the MX Bikes
  folder, with live progress. Resolves MediaFire and Google Drive links
  (including large-file virus-scan confirmation); extracts `.zip`/`.7z`/`.rar`
  and places `.pkz` files.
- Multiple download hosts on a page are shown as a primary "Add to Library"
  button plus per-host mirrors.
- **Bikes** mod type alongside Tracks, via a type switcher in the header;
  Browse, install, and Library are all per-type.
- **"In library" badges** on browse cards and the detail page (fuzzy name match
  against installed files).
- Loading skeletons, an error "Retry" button, and persisted light/dark theme.
- HTTP timeouts (30s API, 15s connect) for resilience.
- Swappable `ModSource` trait in the Rust backend (mxb-mods.com implementation
  via the WordPress REST API + download-page HTML parsing).
- Native folder picker for choosing the MX Bikes path during setup.
- Rust unit tests for REST/HTML parsing and download-link resolution.
- `CHANGELOG.md` and a real `README.md`.

### Changed
- **Upgraded Tauri v1 → v2** (config schema, capabilities/permissions, plugin
  system; `shell` + `dialog` plugins).
- **Converted the frontend from JavaScript to TypeScript** (typed API layer and
  shared types mirroring the Rust structs).
- Rebranded the app to **Frost** (was "MXBMM" / "The MXB App").
- Install placement is generalized to a per-type subfolder (`mods/tracks`,
  `mods/bikes`), configurable in one place in the frontend.
- Config now lives in the OS app-config dir instead of a cwd-relative
  `.config.json`.
- The Library is a proper per-type grid with manual refresh.
- Updated dependencies (MUI 6, React 18.3, Vite 5, current Tauri 2 stack).

### Removed
- Unused dependencies: Mantine (`@mantine/*`, `postcss-preset-mantine`), `axios`,
  and `path-browserify`.
- Dead/broken `src-tauri/src/config.rs` (replaced with a working config module)
  and a stale `.config.json` dev artifact.
