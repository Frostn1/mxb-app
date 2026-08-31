# Changelog

## 2026-08-31 — v0.12.6

### Changed
- Paint sync makes one request where it used to make two: saying where you are rides along
  with asking who else is here.
- The sync heartbeat runs every 3 minutes. A rider joining still pulls within seconds — the
  heartbeat covers someone already there changing their look.
- Sync only says you are on a server when you are on one.
- Usage counts are sent every half hour, and stop for the run if the server has had enough.

## 2026-08-31 — v0.12.5

### Changed
- Paint sync is off by default. Turn it on in Settings → General.
- A sync only reaches into the running game when it has actually installed something.

## 2026-08-31 — v0.12.4 — Paint sync, on every server

### Added
- Paint sync works on any server, not just ours. The app reads the server name out of the
  running game, so a public server syncs the same as a private one and the host installs
  nothing.
- No invite code. The app signs itself up the first time sync runs, the same way voice does.
- Paint sync is on by default, with a switch in Settings → General to turn it off.
- Starting the game from Steam or a shortcut now syncs too. Only the Play button did before.
- A rider joining after you triggers a sync straight away. The app watches the entry list
  and pulls within a few seconds of a new name appearing, instead of waiting out the
  45-second heartbeat.
- A feature's ground height is set on its own row in the track editor, and features can be
  dragged along the height strip.
- A Height/Shape pill on that strip. Height shapes the ground the track runs on; Shape
  shapes the thing built on it. A straight only gets Height — it has ground, not a shape.
- In Shape mode a jump's outline is the jump: drag the top of a tabletop to set its height,
  its far end to set its length, the far side of a double's gap to set the gap, a whoop's
  first crest for spacing and height, its far corner for how many crests. Corners with
  nothing behind them sit there as anchors rather than pretending to be adjustable.
- Handles ride the height curve, so a jump on a hill is grabbed on the hill rather than at
  zero.
- Jumps can be drawn point by point: click to add a point, drag to move it, double-click to
  take it away. A jump drawn this way keeps its points; nothing turns a tabletop into one
  behind your back.
- Anonymous usage counts, so which pages and features actually get used is knowable. The
  app sends a random install id, its version, the OS, the title, a session count and
  counters for named pages and features — never your name, your files or your address.
- A switch in Settings → General turns those off, and the welcome slideshow says it is
  there.

### Changed
- The sync line in the sidebar is no longer behind the experimental toggle.
- The Manage tab is called Race mode, which is what its own help text has always called it.
- Join server only appears when there are servers to list. Until the app can read the
  game's own browser, an empty list behind a button is worse than no button.
- Paint sync has its own Settings page — what it published, what it pulled, your GUID, and
  any paint it declined to overwrite. It used to live on the Servers tab.

### Removed
- The Servers tab, and the experimental toggle that revealed it. Creating and hosting
  servers is out of the app for now. Joining one by address is unaffected and no longer
  hidden.

## 2026-08-31 — v0.12.3

### Added
- The lap list groups by segment: each corner and straight heads a section holding the
  jumps on it, collapsible, with a count when it's shut.
- Adding a feature scrolls to it and flashes the row. It lands in the emptiest stretch of
  lap, which is rarely where you were looking.
- Name, author and location are proper fields rather than headings, and all three end up
  in the track's `.ini`.
- Install writes the `.trh`, a `.map`, the `.ini`, the `.amb` and both UI images. It used
  to write two files. Still missing is the `.rdf` — start gate, pits and cameras — which
  only TrackEd writes.
- Each feature row carries its kind's colour, and hovering a row lights that feature up in
  the 3D view.

### Fixed
- A berm stranded on a straight by a segment edit is moved onto the nearest corner and
  fitted to it. It used to be dropped without saying so.
- Applying a model swap no longer says the bike changes in the running game. FrostMod
  dropped the live re-apply in v0.9.11 because it crashed the game, so the app now tells
  you what actually has to happen: switch bike category away and back in the garage.
  Reselecting the same bike doesn't load the new model.
- FrostMod's own in-game note said to re-select the bike, which doesn't work either. It
  says the same thing as the app now, in FrostMod 0.15.4 — which the app installs for you.

## 2026-08-31 — v0.12.2

### Added
- Settings → FrostMod lists the nine replay camera slots. Open one to see where its keys
  fall, drag a key to move it, change what a key does, and set how the path flies.
- Camera paths can be exported and imported, so one can be passed to someone else.
- Respace by distance, for a constant-speed dolly. A cut keeps the length it was given.
- The panel says when the replay camera can't run on your game build, and why. That reason
  only existed in a log file next to a DLL before.
- FrostMod's eleven new replay camera actions are in the key list.
- The camera itself learned to aim at a rider, cut between shots and fly with a handheld or
  drone rig, in FrostMod 0.15.3 — which the app installs and updates for you.
- The track editor shows the 3D preview beside the lap instead of over it, and paints every
  feature kind its own colour so a row in the list and a lump on the ground match up.
- Features can be added and removed in the editor, and a track's name and author edited.

## 2026-08-31 — v0.12.1

### Added
- Settings → FrostMod now lets you rebind the replay camera editor's keys. The game reads
  the same key at the same moment, so `S` for save also moved the camera for anyone with
  `S` bound to move-backwards; now you can put the action on a key the game doesn't use.
- The hide-overlay key (`F7` by default), which hides everything FrostMod draws for
  recording a replay, is rebindable there too.
- Keys are written into FrostMod's own config, including a plugin-folder install, and take
  effect the next time the editor opens.

## 2026-08-30 — v0.12.0 — MXB Hub, and PSD support in the Designer

### Added

- MXB Hub tab. Browse the shop.mxb-hub.com catalogue and install anything your account owns,
  free mods included. Mods hosted on MediaFire and similar install from here too.
- Protect tab in Studio. Lock your files to specific riders by GUID, one folder per rider.
- The app reads your GUID from the running game, so you no longer have to wait for a server
  to see you connect.
- The Designer opens and exports Photoshop files with layers intact.
- "Stock as base" paints a bike's stock texture into your sheet, so you can add a number and
  save it as-is.
- Mod pages show who made it, with a link to their page on the catalogue.

### Changed

- Picking a model fills in the sheets it needs, instead of opening empty.

### Fixed

- Mirror works on one-piece bodywork like seats and tanks. It used to fail on most of a bike.
- A decal mirrored onto a narrow panel comes out the right size.

## Unannounced — voice chat

Kept out of the release notes on purpose: voice is off by default and has not yet been
tried on a live server with a real grid on it. Both entries fold into the notes of
whichever release turns it on.

### Added

- **Voice chat, on any server, with nothing to set up.** Turn it on, pick a microphone, and
  that is the whole of it: joining a server puts you in voice with everyone else there who
  has the app. There is no second program to install, no account to create, no code to share
  and nothing for the server owner to run — it works the same on a server we host and on one
  that appeared this morning. Your voice goes straight to the other riders rather than
  through us, so it costs nothing to provide and nobody is relaying what you say. Push to
  talk by default, with the mic key you already set. Settings shows who is in the room, who
  is talking, and a mute button for anyone you would rather not hear.

### Changed

- **Voice now works whichever way you joined a server.** It used to start only when the app
  itself launched the game at a server, which left out everyone who picks one from the game's
  own browser — and quietly kept you in the old room if you moved servers without quitting.
  FrostMod knows which server the game is on and now says so, so voice follows you. It also
  knows your race number, which is what will place your voice on the track when proximity
  lands. Riders are grouped by the server's own name, because that is the one thing everyone
  on it sees the same — an address only reaches the people whose app launched the game, and
  half a grid in one room and half in another is the failure that looks like nothing is
  wrong.

## 2026-08-29 — v0.11.3 — A track viewer that draws the whole track

### Added

- **The track viewer draws what stands on the track, not just the shape of it.** Until now a
  track opened as bare ground: the right ruts, berms and jump faces, but nothing standing on
  them, so a supercross floor and a national circuit read much the same. The viewer now draws
  the track's scenery too — the tents and awnings, the hay bales and tyre walls, the banner
  lines and fencing, the trailers in the paddock, and the landscape beyond the track's own
  square — all in the places the track itself puts them. **Objects** in the header turns it
  off again.

  It also marks what a track places but ships no model for: every marshal post, every TV
  camera and every crowd sound source, each as a pin standing on the ground where the track
  states it. That the pins land on the ground rather than near it is the check that the
  scenery is in the right place at all — a track states its marshals' heights, its terrain
  states the ground's, and nothing makes the two agree except reading both correctly.

  It is drawn in the track's own colours, not a flat grey: the `.map`'s surfaces come out
  with it, so the tents are the tents and the dirt is the dirt. Foliage, crowd and fencing
  are cut-outs rather than the solid slabs they would otherwise be — a track's own naming
  says which surfaces carry a cut-out, and drawing those without one turns a treeline into a
  wall and buries the track behind it.

  It arrives in stages rather than all at once. The terrain draws first, then the scenery's
  shape, then the colours on top of it — because pulling a track's map out of its archive is
  nearly all of what a look at one costs, and the shape of the place is worth having on
  screen while the hundreds of megabytes behind its colours are still being read. The header
  says which stage is still running. Both halves are cached, so opening the same track again
  skips the archive entirely.

  Click anything standing on the track and it lights up on its own. The scenery arrives as one
  mesh, but the things in it are separable — an exporter welds a tent to itself and to nothing
  else — so the viewer recovers the pieces a track was built from and names the one you point
  at: how many triangles it is and how big it is in metres. A published track comes apart into
  ten thousand of them, the largest five metres across.

  The ground keeps its own colour while it does. The grain is measured against the sheet's own
  average brightness, so it varies the surface without dragging it darker — tiling a dirt or
  grass sheet raw turned a dry circuit muddy — and only the sheet's luminance is used, never
  its hue. A track with no dirt sheet of its own is left alone rather than tinted with
  whatever else it had.

  The ground keeps its detail when you get close to it. A track states its surface at about a
  third of a metre per sample, height grid and surface picture alike, so anything nearer than
  that was interpolation — a soft brown smear. The track's own ground sheet is now tiled over
  the terrain and multiplied into it, which puts the grain back at whatever distance you care
  to look from. It says what the ground is made of, not what is where.

  And where a track ships one, that grain has relief rather than being a picture of relief.
  Half the tracks measured carry a normal map beside their ground sheet — the sheet that says
  which way the surface faces rather than what colour it is — and it is now tiled with the
  colour it belongs to and lit by the track's own sun, so a rut catches the light on the side
  facing it. Gently: one sheet is standing in for every surface a track has.

  Which sheet gets picked is read as words rather than letters. A track's ground was chosen by
  looking for `dirt` or `sand` anywhere in a name, which on one track picked `logo-dirtmaster`
  — a logo — and on another a sponsor's banner, and a near-flat sheet tiled over a track does
  nothing at all. Names now split into their words however they were written, capitals
  included, a word only counts whole, and a name carrying `logo`, `banner`, `sign`, `marker`
  or the like is not ground however grounded the rest of it sounds. A track may also name its
  riding surface after itself — SandPoint calls it `track-dark` — which is now read as ground
  where nothing better is offered. All sixteen installed tracks find their ground, against
  thirteen before, three of which were finding the wrong thing; nine find its relief, against
  three.

  A track that places props of its own draws its scenery again. The props a `.scr` puts down
  are added to the same mesh as the rest, but they were not being numbered as pieces — and the
  viewer works out where the surfaces begin from the piece count, so a mesh short of ids reads
  past its own end and is thrown away whole. Abydos was sixteen thousand prop triangles short
  and drew nothing at all. Each placed prop is now a piece like any other, which is also what
  makes it something you can point at.

  A track that states no surfaces of its own draws its terrain again. Tiling the ground sheet
  over it read the coordinates three.js sets up for a surface picture — which a track without
  one never has, so the shader failed to build and the terrain drew nothing while its scenery,
  sky and markers all drew fine. The grain now carries its own coordinates and works with or
  without a picture under it.

  A track that states no surfaces at all is drawn in the colour of its ground rather than by
  height. Two of the installed tracks ship no coverage data, so the viewer fell back to an
  elevation ramp — green in the hollows through to white on the high ground, which on a sand
  circuit is not a legend anybody asked for. Where the track's own ground sheet is known, its
  average colour is used instead and the shape reads by its shading alone.

  The ground sheet is cached with the rest. Finding it means inflating candidate sheets to
  check a normal map really is one, and that was happening on every open.

  A track is lit and hazed the way it says it should be. Its ambience file states a sun
  colour, an ambient, and a fog — the viewer used one fixed rig for every track and ignored
  all of it. The haze is thinned to something a whole track can still be seen through: a
  track's own figure is written for a rider looking down a straight, not for a view of the
  entire place at once, so the colour and the relative thickness are the track's while the
  depth it acts over is the view's.

  A track now sits under its own sky. Every one ships a dome and a backdrop and names them in
  its ambience file, and the viewer was throwing both away — a track ended at a hard edge with
  black beyond it. They draw with the track's own picture on them, lit from where the track
  says its sun is.

  What stands on the track is lit from above rather than from below. Every piece of scenery
  was being drawn back to front — the game's meshes are wound the other way round, and the
  view already mirrors them, so reversing them again put every face the wrong way out. Drawn
  double-sided that shows up not as holes but as light: the sun landed on the underside of
  everything. Small props got away with it; a track that surrounds itself with landscape did
  not, and Abydos's dunes drew as a black apron around the circuit. They are dunes now.

  Where a track's surfaces can't be read, its cut-outs are left out rather than drawn wrong. A
  quarter of a map is foliage, crowd and netting — flat cards that are a tree only once an
  alpha channel has cut the tree out of them, and drawn plain they are thousands of standing
  sheets of paper hiding the track behind them. Tracks whose surfaces do read keep every one
  of them.

  Ground colours are drier. A surface id names what the physics does, not what a track looks
  like, and a lawn green on a dry circuit was the loudest wrong thing on the screen — the same
  hues now sit closer to earth, so a surface a track named loosely reads as ground.

  Props can be written back out. A track's `.scr` is the one part of it that states where a
  thing goes in plain text, and the game reads it at load — so it is where anything placed in
  the app ends up. What goes out reads back as what went in, and an existing file is left alone
  unless replacing it is asked for.

  A track carrying no scenery is left alone rather than reported as a failure; some ship none
  at all.

### Changed

- **The log now records which GPU the 3D views are drawing on.** Both viewers draw through
  the graphics card, but a Windows machine with a driver Windows doesn't trust drops them to
  a software renderer instead — same picture, a fraction of the speed — and nothing said so.
  Opening a 3D view now writes the adapter it got into the app log, flagged as a warning when
  it turns out to be software, so "the viewer is slow" can be answered from the log a player
  already sends.

## 2026-08-29 — v0.11.2 — Install a pack one bike at a time, and a preview that fills the window

### Added

- **A bike pack installs as the bikes inside it.** The OEM bike pack is 54 machines and a
  tyre set in one 3.8 GB archive, and until now it arrived as a single row reading "Mods
  folder" — all of it or none of it, with no way to see what was in there. It is now listed
  the way it is built: every bike by its real name and class (*KTM 450 SX-F 2023 · MX1 OEM*),
  its size, and a checkbox. Take the four you race and leave the other fifty. **Select all**
  and **Select none** sit in the header, because a fifty-five-row list is not a list you tick
  by hand.

  Where each piece goes is read from the pack itself rather than guessed at, which matters
  more than it sounds: the tyre set that carries every one of those bikes' wheels is
  described inside its own file exactly like a bike, and filing it under `mods/bikes` on that
  evidence would take the wheels off all 54. A pack the app can't read confidently is left
  exactly as it was — one row, installed whole — rather than split into pieces it would have
  to guess destinations for.

- **A pack you download is shown to you before it installs.** Downloads used to go straight
  to disk, which for a pack meant 3.8 GB of bikes landing without anyone being asked. One
  that turns out to hold several mods now stops and opens the same review sheet a dropped
  file gets. An ordinary single-mod download is untouched — it installs exactly as before,
  with nothing extra to click.

- **The Designer's 3D preview opens fullscreen.** The model sits in a column beside the
  canvas — the right size while you are drawing on it, and far too small when you want to look
  at what you have drawn. The button in its corner fills the window with it, with the same
  tyre, gear and hide toggles it has in the panel; the button again or Escape puts it back.
  The editor is untouched behind it — same sheets, same layers, nothing saved or reloaded to
  take a proper look. Asked for by GalpinMX.

- **Sly** credited on Settings → Supporters.

### Changed

- **`MXB_SAFE_GRAPHICS=1` now works on Windows too.** It takes the GPU out of the app's
  browser, the same lever it already pulled on Linux — the thing to try when a window comes
  up black and stays that way. The app also records how far the page got loading, and which
  graphics settings were in force, so the next report of a blank window can be read straight
  from the log instead of guessed at.

### Fixed

- **A paint that ships a separate file per bike now installs the right one.** Some pages offer
  one download per machine the paint fits — *pitfactory 250f pub* beside *pitfactory 125t pub* —
  and mark **both** as the recommended file. Nothing about that says "different file" to a picker
  built for mirrors, so it took the first one and put the 250's paint in the 125's folder. Those
  pages are now read for what they are: every file is listed rather than folded away, the one for
  the bike you picked is the one flagged, and choosing a different file moves the destination to
  the bike that file is for. One-click installs from Browse follow the same match, so they stop
  quietly grabbing the wrong machine's paint.
- **The review sheet took twenty seconds to open, for anyone with the OEM bikes installed.**
  Every drop reads the bikes already in your folder so it can offer them as destinations, and
  each bike was opened twice over to get its name — 310 ms a bike where 14 ms would do. With
  53 OEM bikes installed that was 18.9 seconds of nothing happening before the sheet drew, on
  every single drop. It is 1.2 seconds now. The same read backs the bike pickers throughout
  the app, so they all get quicker.

- **The 3D viewer no longer hoards a bike's textures each time it redraws one.** Two things
  asking for the same bike at the same moment — a preview panel and a dialog drawing it
  together, or a picker that re-asks before the first answer lands — read and decoded it from
  scratch twice over, and the copy that lost the race left its textures behind with nothing
  able to free them. Three passes over one bike put 200 MB in the texture store and none of
  it came back. Far enough down that road the store starts dropping its oldest textures to
  stay under its ceiling, and the oldest can be the bike you are looking at — which is how
  parts of a bike turn grey for no reason. Two callers that want the same bike now share one
  read: the second waits for the first and takes its answer, so it arrives in milliseconds
  instead of seconds. Found in a player's log.

- **A black window on startup can no longer trap you.** On a cold boot WebView2 sometimes
  takes a long time to draw its first frame — and occasionally never draws one at all. The
  app window appeared anyway, empty and black, and because our title bar is drawn by the
  page there was no close button in it and no title bar to close it from. Alt+F4 only parked
  it in the tray, where the process stayed alive and handed the same dead window back the
  next time you opened the app; Task Manager was the only way out. The window now stays
  hidden until it has actually drawn something, anything that shows it early gets a real
  Windows title bar to close it by, and a window that never drew closes for good rather than
  hiding in the tray. Reported by a player who found it after booting their PC.

## 2026-08-28 — v0.11.1 — Protected model swaps open in 3D

### Fixed

- **Protected model swaps wouldn't open in the 3D viewer.** A model bought from a creator ships
  its mesh sealed, and the viewer handed those bytes straight to the parser — which found no
  mesh header and reported the bike as empty. The result was *"holds no readable mesh"* on a
  model that runs perfectly in game. Bike files are now unwrapped the way gear and paints
  always have been, loose files and packed entries alike.
- **One failure message was blaming cloud sync for everything.** Three unrelated faults reached
  it — a file that never finished downloading, a mesh that didn't decode, and a mesh that read
  but held no parts — and all three sent players hunting through their OneDrive settings. Each
  now says what actually happened, and the failure writes the mesh names, sizes and headers it
  saw to the log, which it never did before.

## 2026-08-27 — v0.11.0 — Pose the rider, and a Designer that handles layers

### Added

- **Pose the rider.** A new **Pose** view in the Studio opens on a preset as it stands — bike,
  model swap, rider, gear and paints — and lets you move the rider's limbs: where the hands sit,
  how far the legs are spread, one leg forward, elbows up, lean in. Quick moves stack, and every
  joint has bend/twist/splay sliders under Torso, Arms, Hands and Legs. Hips, knees, shoulders
  and elbows reach 135°, wrists and collars 70°, the neck and head 45° — enough to fold a leg
  under a bike. The kit on show is the one the Rider tab has, so a look composed next door is the
  look being posed. The pose is remembered per rider profile on this machine, and **Reset**
  returns the model exactly as it was authored.

  **On bike** puts the rider on the machine rather than beside it, worked out rather than
  eyeballed: he settles up the seat towards the tank, leans into the bars, puts both hands on the
  grips — read off the bike's own handlebar, so they land on the bars of whatever is under him —
  and folds his knees round the machine with his boots on the pegs. **Riding position** is
  applied for you the first time a bike appears under an unposed rider, and a bike whose setup
  file names no seat says so on the button rather than guessing a height.

  This reads the skeleton `rider.edf` has carried all along and nobody was using: 98 named bones,
  of which 64 bind the mesh. The file stores no vertex weights — the game rebuilds the binding at
  load — so the app rebuilds it too, from the per-bone boxes the file *does* carry plus the
  distance to the limb each bone actually swings. Helmet, boots and body armour ride along on the
  bones their own `gfx.cfg` names, so the kit follows the pose.

  Preview only, deliberately: MX Bikes takes a rider's posture from its riding style, an
  animation set in `mods/rider/animations`, and nothing here writes to the game.

- **Take hold of the rider and move him.** The Pose view puts a dot on each of the rider's joints
  in the 3D preview — head, back, shoulders, elbows, hands, hips and feet. Grab one and the limb
  swings to follow the cursor the way it does in Pivot: the joint above the dot you are holding is
  the one that turns, so pulling a hand bends the forearm about the elbow and pulling a foot bends
  the shin about the knee. A drag turns in the plane you are looking at, so orbit the camera to
  reach the other way. Push a joint past its stop and it comes to rest on the way to the cursor
  rather than snapping somewhere else, and dragging back to where you started returns the model
  exactly as it was authored. A drag moves at half the cursor's pace, and finer with Shift held.

  A drag writes the same pose the sliders do, so the two mix freely and quick moves still stack on
  top. The sliders are still there for the two things a drag can't say — twist about a bone's own
  length, and an exact number — but the groups start closed now, since the dots are the way in.

- **The Pose tab can take a photograph.** Five backdrops to stand a rider against — studio,
  white, daylight, sunset and dusk, each with its own light and ground — a clean frame that
  hides the grab dots and the on-canvas panels, and a Save photo button that writes a PNG at
  twice the size of the panel it was framed in. Open the preview full screen first for a
  bigger one. Nothing is downloaded for any of it.

- **Mirror a layer to the other side of the bike.** Place a decal on the right shroud, hit
  Mirror, and a copy lands at the same spot on the left one. The place is worked out from the
  model rather than by flipping the sheet — the two flanks are unwrapped wherever the modeller
  put them, so flipping the square lands artwork on the wrong panel about as often as not.
  The copy stays **linked**: move, recolour, retype or reshape the original and it follows,
  until you unlink it.
  - It says no rather than guessing, and says which no it is: both flanks already sharing that
    part of the sheet (so it's on both sides already), a spot on the bike's centre line, a
    part of the model with nothing at its reflection, or no model loaded at all.
  - Where the far side isn't unwrapped as a true reflection, the placement is still made and
    flagged as close rather than exact.

- **The layer handling that was missing.** Duplicate (⌘D), copy and paste (⌘C/⌘V, across
  sheets too), Delete, and arrow-key nudging — one pixel, or ten with Shift.

- **Several layers at once.** Shift-click to add, drag over empty canvas to lasso, ⌘A for the
  lot. Group them with ⌘G and they move, scale and clip as one; Alt-click reaches inside a
  group for a single layer.

- **Snapping while dragging.** Layers catch on the sheet's centre lines and edges, on the box
  of whatever part they're clipped to, and on each other's edges and middles, with a line drawn
  to show what was caught. Hold Alt to place freely.

- **Flip a layer** left-to-right or top-to-bottom, from the inspector or the new right-click
  menu on the canvas.

- **Type a position and size.** X, Y, size and angle now have boxes as well as sliders, and
  they track the drag — placing a plate number no longer means nudging it by eye.

- **Bikes render with their wheels on.** The 3D preview drew the frame, the forks, the
  swingarm and the bars, then stopped — every bike stood on bare fork tips and a swingarm
  stub. The wheels were never in the bike's mesh to begin with: a bike's `gfx.cfg` names a
  tyres mod (`tyres = oem_mx`), the wheel meshes live in that mod, and the axle points they
  hang off have been sitting in the bike's `.geom` all along. Both are read now, so the front
  wheel sits on the fork and the rear on the swingarm, wearing the rim, disc, sprocket and
  tyre they ship with.
  - **A livery that paints the wheels finally shows it.** An OEM bike's stock livery replaces
    the wheels and the chain and nothing else, so picking it changed nothing on screen — and
    the app said so, in a note apologising for the parts it couldn't draw. Those sheets land
    on real parts now.
  - The chain is still left off. It ships as a straight template strip that the game bends
    onto the sprockets as it runs, and drawn where it sits it is a bar standing out of the
    rear wheel.
  - A bike whose tyres mod isn't installed — or whose `.geom` names no axles — renders
    exactly as it did before.

- **Pick which tyres a bike is previewed on.** A bike's `gfx.cfg` names exactly one tyre
  pack, so seeing it on another was impossible — the preview fitted what the file said and
  that was that. A **Tyres** picker now sits beside the livery one in all three previews (the
  Viewer, the Rider tab and the Designer), listing what's installed under `mods/tyres`. It
  substitutes the name the wheels are looked up under and nothing else: no file is renamed,
  no mod is touched, and the bike's own `gfx.cfg` still reads exactly as the game reads it.
  The choice is remembered, and it's one choice — pick it in the Viewer and the Designer
  agrees. Picking a pack that isn't installed leaves the bike on its own rather than taking
  its wheels off.

- **Stand a bike the way you want to see it.** The 3D preview drew every bike in the frame it
  was *authored* in, which is not a stance it ever holds on the ground — a `.geom` carries no
  suspension travel at all, and ride height falls out of physics the viewer doesn't run. So
  bikes stood with the shock apparently collapsed and the rear wheel riding high. The preview
  now knows the bike's own joints and lets you move them: **Rear** swings the swingarm about
  its pivot, **Front** slides the fork up its own raked axis, **Steering** turns the bars and
  the front wheel with them, all in millimetres and degrees of the real thing.
  - **Level wheels** solves the rear for you — both tyres touching the same ground, measured
    at the contact patches rather than the axles, since a 21" front and a 19" rear aren't level
    when their axles are. A bike wearing wheels is drawn that way to begin with, so it stands
    right without anyone touching a slider; **Reset** puts it back as authored.
  - The panel is in the expanded preview and the full-screen viewer. A bike whose `.geom`
    names no mounts has no joints to move and renders exactly as it did before.

- **The bike's pose panel, in the Rider tab.** Rear, Front, Steering and Level wheels were in
  the library's viewer and the expanded preview only; the tab where a look is actually built
  now carries them too, in the panel beside the pickers.

- **The bike now stands next to the rider in the Studio preview.** The 3D panel in Studio →
  Rider only ever drew the rider, so the bike half of a look — its livery and its model swap —
  was invisible until you were in-game. The panel now draws both in one scene, at their real
  sizes, and the Rider tab gains a bike picker with the livery and model-swap slots beside it.
  A preset opened with "View in Rider" brings its bike along, so it arrives fully dressed.
  The Bike / Rider / Both toggle picks what's on screen; either half stays up while the other
  one re-reads, and a bike that won't resolve says so instead of quietly leaving the rider alone.

- **Move the bike and the rider around in the Rider tab.** The pair stood where the viewer put
  them — shoulder to shoulder, a fixed gap apart — which is the one arrangement nobody was
  composing for. A **Placement** panel now moves either model: side, up, forward and turn, in
  metres and degrees, so the rider can stand at the bike's shoulder, sit on it, or face the
  camera with the bike behind them. **Reset** puts the pair back the way it opened.

- **A 3D preview you can drag wider.** The Rider tab's preview was a fixed 420px onto a bike
  and a rider side by side. Drag the handle on its left edge to give it as much of the tab as
  you want — double-click to put it back — and the width is remembered per machine.

- **A bike with no wheels to solve against now stands on its suspension.** "Level wheels"
  needs wheel meshes and axles to measure against; a model that ships neither fell back to the
  authored frame, which carries no suspension travel at all — so the bike stood with its shock
  apparently collapsed. The rear now defaults to 140 mm of drop instead. Bikes the solve *can*
  answer for are unaffected, and **Reset** still puts a bike back exactly as authored.

- **Move a model swap to another bike, or delete it.** Every model set now carries its own
  menu, in the Locker and on the Library's bike cards alike. **Move** asks which bike to send it
  to and, when the model owns liveries, which of those travel with it — off by default, because
  a paint is drawn for one bike's layout and rarely fits another; anything left behind stays put
  rather than being thrown away. **Delete** sends the set to the Trash, so a model you can't
  download again is recoverable, and leaves the bike's liveries alone. Neither is offered for
  the model currently on the bike: its files are loose at the bike root, so moving them would
  take the bike's live model out from under it — switch to another model first.

- **View a model swap in 3D straight from the Library.** Each variant in a bike card's model
  list now carries its own **View 3D**, drawing the bike as that swap would leave it — the
  same preview the Locker offers, without having to go there to find it. Nothing on disk
  moves. Sets with no mesh have nothing to draw and don't offer it.

- **See a bike's model swaps without leaving the Library.** A bike card now carries a
  **models** badge when there is more than one model set installed, and opening it lists them
  in place — the active one ticked, an incomplete set flagged, a "no model" set marked, and a
  file count for the rest. It reads the same vocabulary as the Locker, so a variant looks the
  same wherever you meet it. Deliberately read-only: the Locker stays the one place that moves
  files, so two views can never disagree about which model is live.

### Changed

- **Models reach the 3D view without being spelled out as text.** A mesh's vertices used to
  cross to the viewer as JSON numbers — 5.9 MB of digits for a small bike, and every one of
  them parsed back into a number on arrival. They travel as their own bytes now: **the app
  spends 12.4 ms preparing a bike's mesh instead of 2.2 ms**, and the viewer unpacks it
  **6.8x faster** (31 ms → 4.6 ms), which grows with the model — a detailed bike or a gear
  mesh is several times the size of the one measured. Bikes, rider gear, helmets, the rider
  body and model-swap previews all arrive the same way.

- **A track's 3D view appears about seven times faster.** The fine terrain is a 2048×2048
  grid — four million vertices — and three.js worked out its lighting the general way, by
  walking all 8.4 million triangles and accumulating a normal onto each corner. A height grid
  doesn't need the general way: the ground is a function of x and y, so its slope comes
  straight from the neighbouring samples. Measured on the same mesh, that took building the
  view from 622 ms to 84 ms, with the two agreeing to within 0.02° — the same picture, drawn
  without the wait. Nothing about the terrain's detail changed.

- **Four mods install two at a time instead of one after another.** Downloading was never
  what made a batch slow: a single MediaFire connection measured at 25–34 MB/s, more than a
  typical home line carries, and splitting one file across eight parallel connections was
  worth exactly nothing. What cost time was the line sitting idle between mods, while one
  resolved its link or unpacked. Two now overlap, and the next few links are looked up ahead
  of their turn rather than when their turn arrives.

- **Installing a downloaded mod is faster, and the app stays responsive while it happens.**
  Every byte used to be written to disk three times — the archive, the unpacked copy, then a
  third copy into the mods folder — for files that were deleted moments later. The last of
  those is now a move, so on one drive it costs nothing; a mods folder on a different drive
  falls back to the copy, retries and all. Unpacking also no longer runs on the app's async
  runtime, where a big track pinned a worker for the whole of it. Everything that installs
  from somewhere we don't own — a folder you dropped in — still copies, untouched.

- **MediaFire links resolve about a third of a second quicker.** The app asked MediaFire's
  API for a download link first and scraped the page only if that failed. Across eight real
  tracks the API refused every one, and the scrape rescued all eight — so the first request
  was pure delay. The page is asked first now; the API stays behind it, still the route that
  survives the page being redesigned.

- **Preset bundles, shared files and picked files place the same cheap way downloads do.**
  They all unpack to a folder that is deleted moments later, so their files are moved into
  place rather than copied a second time.

- **A download that goes silent now resumes instead of hanging.** Only the connection was
  given a timeout, so a host that accepted the socket and then stopped sending sat there
  forever: the resume machinery only wakes on an error, and silence never was one. Thirty
  seconds of nothing is now treated as a broken transfer and picked back up where it stalled.
  Uploads are deliberately exempt — their response doesn't arrive until the last byte is sent.

- **The Rider tab's bike picker is searchable.** It was a plain dropdown, which is a long
  scroll past dozens of bikes for a name you already know. It's now the same searchable
  field as the Paint and Model swap slots beside it — type to filter. Unlike those, it
  won't take a name you made up or an empty value, because neither is a bike.

- **The Rider preview starts from the stock model.** With no model swap picked it drew
  whatever swap happened to be on the bike, so the same look rendered differently on
  everyone's machine. It now draws the game's own model unless you pick one — for a bike
  whose files are all packed that was already what you were seeing, so nothing changes there.

### Fixed

- **Opening a bike in 3D was doing half its work for nothing.** Every texture packed inside a
  model was run through a resize on the way in — including the ones that were already the
  size the viewer wants, which is how bike sheets are almost always authored. Resampling a
  1024×1024 sheet to 1024×1024 is pure cost, eight times over on a typical bike. Skipped now,
  the same way loose paints have always skipped it: **opening a bike goes from 201 ms to
  127 ms**, and the sheets are a touch sharper for never having been resampled. Rider gear,
  helmets and model swaps read their textures the same way and all get the same back.

- **A model swap rendered as a plain white bike.** A mesh that ships companion sheets
  (`_n`/`_s`/`_r`) writes a second texture index into each material record, in a field the app
  required to be zero — so every material table was thrown out and every part fell through to
  bare grey. Read back, those indices count a list the app wasn't building either: one that
  includes the companion maps *and* the sheets a mesh declares but never embeds. Both are fixed,
  and a bike whose materials don't use that second slot — every stock bike — is read exactly as
  it was before. The KTM 450's swap goes from nothing bound to all 31 parts on their right
  sheets: the Pro Taper bars, the ARC levers and calipers, the ODI grips, the Hammerhead pedal
  and shifter, and the Polar mount, which is painted by a sheet the mesh never embeds at all.

- **A model swap previewed as a white bike in pieces.** A model set is a mesh and little
  else — the `.geom` that mounts the parts to each other, the `gfx.cfg` and `.hrc`s that say
  which mesh each part uses, and the stock paint all belong to the bike, and on an OEM bike
  they never leave its `.pkz`. The preview skipped that archive the moment the swap brought a
  mesh of its own, so it drew the swap's `model.edf` and nothing else: every part stacked at
  the origin with no texture on any of it. The packed bike now goes underneath every preview,
  with the loose files over it and the swap's over those — the order the game itself reads a
  bike in. The same skip was on the ordinary load path, so an extracted bike whose `.geom`
  stayed behind in its archive rendered unassembled too, swap or no swap.

- **A model swap could carry off the bike's own setup files.** A variant folder holding copies
  of the `.hrc`s, `.cfg` or `.geom` — with no mesh of its own — made the swapper treat those
  files as the model's, so they were parked along with it and the bike was left a mesh with
  nothing to assemble or texture it. They're the bike's, never a model's; only a variant that
  actually brings its own replacement displaces them now. Bikes already recorded that way are
  read correctly without anything having to be repaired on disk.

- **A packed mod with a version in its name told the Designer nothing.** A model installed as
  an archive is found by the `<Model>.pkz` sitting where its folder would be — and that name
  was built by replacing the model's file extension, which a name like `Fox Instinct 2.0 by
  Aeffertz` appears to have. The app went looking for `Fox Instinct 2.pkz`, a file nobody has.
  So a packed bike, helmet or boot whose name carries a version number offered no sheet names
  in the Designer: no "expected names" line, nothing for **Create the sheets this model asks
  for** to create, and no suggested name on a blank sheet — and a sheet named by guesswork
  paints nothing, which is only discovered in game. That boots mod now offers `fox` and
  `fox_n`, as it always should have. The same lookup answers whether a bike exists at all when
  it's installed as a bare `.pkz`, so a model-swap preview for one resolves too.

- **A rider kit or a pair of gloves had no sheet names to start from.** `Rider+` and
  `Rider+RolledUp` ship their `paints/` and `gloves/` folders empty on purpose — the kits
  installed under the stock rider are the ones meant to be worn on them, which is already how
  the preview dresses one. The Designer didn't know that, so painting for either profile began
  with an empty expected-names line and nothing to create sheets from, and gloves had no source
  of names at all. Both now read the stock profile's paints: `rider`, `rider_n`, `rider_r` for
  a kit, `gloves`, `gloves_n`, `gloves_r` for gloves.

- **Picking a rider profile could hang the Designer for a minute and a half.** With no paints
  of its own to read, the app fell back to walking the profile's mesh for texture names — and
  those two files are 67 MB each. Where iCloud or OneDrive had evicted them, asking what a
  profile expects quietly kicked off a 134 MB download and answered 84 seconds later. Sheet
  names are a convenience; an evicted mesh is now left alone and the paints answer instead, in
  about a tenth of a second. The preview still fetches the model when it draws it, where the
  wait buys a picture.

### Removed

- **The second "Goggles" destination.** The Studio offered goggles twice — the pair bought with
  a helmet, and a pair shipped with the rider profile — and two buttons with the same word on
  them is a coin toss over which folder a paint lands in. The helmet's is the one anybody means:
  it's where a goggle mod installs and where every goggle paint in the shop is filed. Gloves,
  which have no twin, are untouched.

## 2026-08-26 — v0.10.2 — Liveries that belong to a model, and a Windows that starts

### Added

- **A model swap can own its liveries.** MX Bikes gives a bike one flat `paints/` folder and
  knows nothing about model swaps, so a Yami mesh running on a KTM offered every KTM livery
  alongside the Yami ones — a long list where most entries were drawn for a mesh that isn't
  on the bike. Each model in the Locker now has a palette button: tick the liveries drawn for
  it and they become the only ones it offers. A livery belonging to a model that isn't on the
  bike is moved out of `paints/`, so **MX Bikes' own paint picker is filtered too**, not just
  the app's. A livery left unticked by every model belongs to none and stays available under
  all of them — so a bike nobody has assigned anything on behaves exactly as before.
  - A livery can be ticked under several models at once. Ownership is recorded rather than
    filed, so that costs no second copy of the file.
  - "Stock" — the model inside the bike's own `.pkz`, which never has a folder — can own
    liveries like any other model. That's what puts a KTM's own liveries on the KTM.
  - The Presets livery dropdown now offers only the liveries that suit the model the preset
    selects, so a preset pairs a model with a livery drawn for it.

- **A Locker repair warning can be dismissed.** The banner for a bike whose setup files an
  old swap moved away sat at the top of the Locker with no way to put it down, so anyone who
  had decided to leave that bike alone read the same three lines every visit. Each one now
  carries a ✕. Hiding is remembered per bike *and* per set of missing files, so the same bike
  breaking a different way still speaks up, and a bike you repair — then break again later —
  warns afresh rather than staying quietly hidden.

## 2026-08-22

### Fixed

- **Liveries that came in with a model pack were invisible.** A model swap that shipped its
  own `paints/` folder left its liveries inside the swap folder once the app filed it away —
  somewhere the game never reads and no scan of ours ever looked. They were installed,
  unusable and unlistable. Opening that model's livery picker now adopts them as its own,
  which is also what makes them work.

- **A livery inside a model swap was filed under the wrong owner.** The Library attributed it
  to the swap folder rather than to the bike, so it landed in a bucket no bike id ever
  matched — and a preset share code built from that bike silently shipped without the livery.

- **MXB App wouldn't start at all on a freshly installed Windows,** closing the moment it
  launched with "the application was unable to start correctly (0xc000007b)" — no window, no
  log, nothing to send in. The app needs Microsoft's Visual C++ 2015–2022 (x64) runtime and
  has since v0.3.2; Windows doesn't ship it, but some other game nearly always installs it
  first, so the gap only shows on a PC that has just been reset. It fails inside Windows'
  loader, before a line of the app's own code runs, which is why the runtime check the app
  already carried could never fire — it was on the wrong side of the door. The installer now
  checks for that runtime and puts it in before it writes the app, and tells you which one is
  missing if it can't.

- **A `msvcr90.dll` in the game folder that the app wouldn't remove is no longer a silent
  crash.** A loose VC9 CRT beside `mxbikes.exe` aborts the game with *"R6034 — An application
  has made an attempt to load the C runtime library incorrectly"* the moment anything
  plain-imports it, and v0.10.1 began taking back the copies this app itself planted. It only
  ever deletes a file whose bytes match a Visual C++ 2008 assembly already on the PC, which is
  what stops it reaching for somebody else's file — but everything it declined to touch it
  swallowed, in three cases that all end with a dead game and no explanation: a copy that came
  from somewhere else (a mod archive extracted into the game folder is the reported one), a PC
  with no VC90 installed at all, where nothing can ever match and even our own copy is
  stranded, and a file the running game is holding open.

- **The app now says so, and offers the fix.** A file it won't delete on its own gets a red
  bar naming it, and a button that renames it to `msvcr90.dll.disabled` — Windows resolves an
  import by exact filename, so the rename is what defuses it, and a file somebody put there on
  purpose survives the decision. Automatic removal is unchanged: still only ever a copy this
  app can prove it made. The press is what settles provenance for everything else. When the
  game is holding the file, the bar says to close it first rather than failing at a button.

- **"Repair runtimes" reports the same thing**, so a repair that installed everything and still
  found the reason the game won't start says that instead of "everything was already in place".

- **An install blocked by antivirus reported a bare "os error 2".** When a security product
  blocks a freshly downloaded mod file in the temp folder, the app's check for it asked the
  wrong question — it tested only whether the file still had a directory entry, which a
  scanner holding the *contents* leaves untouched. The advice written for exactly this case
  never fired, and the install failed with a raw path and an error number that reads as if
  the mod were broken. The check now tries to read the file the same way the copy does, so a
  blocked install names the file, says whether it was removed or locked, and points at the
  folder to add an exclusion for.

- **A share code could point outside the mods folder.** Every path a preset or file-share
  code carries is joined onto your mods root on import, and the first one picks the type
  folder outright — so a code written by hand with `../` in it could write into the game
  folder itself rather than into `mods/`. Nothing this app has ever produced looks like that,
  but a code arrives as text from someone else, so every path is now checked when the code is
  decoded and a bad one is refused at the door rather than failing partway through an install.
  Zip extraction is swept for links that point out of the staging folder — the same sweep 7z
  and rar archives already got — and a filename handed over by a remote server can no longer
  name the staging folder's parent.

- **Paint sync published the wrong bike's livery.** Liveries were matched by filename alone,
  so a rider with `Race.pnt` on two bikes published whichever the folder walk reached first —
  and the file carried that bike's install path, so everyone on the grid saw it land on the
  wrong bike as well. A livery is now resolved under the bike that actually wears it, and a
  livery missing from that bike is reported as missing rather than borrowed from another one.
  The same mismatch made the live-reload watcher watch another bike's file, so a paint edited
  mid-session could refresh late or not at all.

- **Helmet, goggle, boot and protection paints were never shared.** Gear paints that sit
  inside their model's folder were being folded into that folder — correct for a preset zip,
  which carries the folder whole, but a publish uploads paint files one at a time and skips
  folders, so those paints silently went nowhere. Gear paints now publish alongside bike
  liveries; preset bundles still pack the folder once.

## 2026-08-21 — v0.10.1 — Mods you deleted, remembered — and a crash we caused, undone

### Added
- **The Library remembers what you used to have installed.** It only ever showed what was on
  disk right now, so deleting a track erased every trace that it existed — and months later
  there was no way to work out what it had been called. A new **Removed** toggle in the
  Library toolbar shows what the folder used to hold, with each mod's name, author, location,
  length and its thumbnail, all captured while it was still installed. Covers everything that
  was ever in the folder, however it got there: tracks you built yourself and copied in by
  hand are remembered exactly like ones the app downloaded. Where the app was the one that
  fetched it, the row offers to download it again.
- Mods disabled in Manage are listed separately as **Parked by Manage**, so a track that has
  merely been switched off is never mistaken for one that was deleted.
- **Restore puts a deleted mod back.** When the app was the one that removed it, it now notes
  where the Trash put the files, so the row can move them back where they came from. Refuses
  rather than overwrites if something is installed there again.
- **"Find it again" searches every source at once** — mxb-mods and the Shop together — using
  the remembered name, and a result opens straight on its mod page. Sources live in one list,
  so a new one added later shows up here without another button.

- **A paint saved on disk shows up in the running game.** The game reads your look once,
  when the profile loads, so painting used to mean save, alt-tab, reselect your profile,
  look, repeat — and mid-session there was no way to see a change at all. The app now
  watches the `.pnt` files the rider is actually wearing (the bike's paint and font, and
  every piece of gear) and, when one is rewritten, re-runs the game's own look loader in
  place. Same call the Locker and presets already make, gated on the same **Instant
  refresh** setting, so there is nothing new to switch on.
- **Paints that arrive mid-session are applied without a rejoin.** Paint sync already
  installed other riders' liveries while you rode and then left them sitting on disk until
  the next session. They now go into the running game the moment they land.
- Three new supporters credited in Settings → Supporters: RodaksRevivalYT | Black Rifle,
  MintyFlow and Thomas.

- **Share logs.** Settings → Logs already named all three log sets and saved them into one
  zip; the file still had to be got to whoever asked, which is where "send me your logs"
  usually stalls — a save dialog, then a file to find, then somewhere to upload it. **Share
  logs** packs the same archive and uploads it through the same host a shared track goes out
  on, then puts the direct link on the clipboard and leaves it on screen with a Copy button.
  What goes in is what **Save logs…** already packed: the three log sets and the
  `summary.txt` header (app version, OS, game, folders, and what was collected) — never the
  config file, which holds session cookies and shop credentials. The link is public to
  anyone holding it, and the page says so under the field. A bundle big enough to be sliced
  comes back as a numbered list of parts rather than a first link that loses the rest.
- **`summary.txt` now names the installed FrostMod build**, in the saved zip as well as the
  shared one. The loader's log is in the archive and "which build wrote it" is the first
  question anyone reading it has; `version.txt` itself stays out, being one of the files we
  put in that folder ourselves.

- **The app records how the game ended, not just that it ended.** Until now the only thing
  watching MX Bikes was a process-table poll, which can say the game is gone and nothing
  else — a clean quit and a crash on the loading screen looked identical in the log. A
  handle is now held on the running game, so its exit code and true session length survive
  it: a clean quit logs as one, and a crash logs `[session] game CRASHED after 4.1s` with
  the exception named. `STATUS_IN_PAGE_ERROR` is called out by name, because that is what a
  file the game had mapped but could not read looks like.
- **A warning when the mods folder isn't really on disk.** OneDrive, Dropbox and iCloud can
  leave a file looking completely ordinary while its contents still live on a server. MX
  Bikes reads the mods tree during the load screen, memory-mapped, so a placeholder whose
  fetch is slow or refused surfaces there as a crash with nothing in any log to explain it.
  The app now checks the tree when a session starts and names the count, the provider and a
  few of the files. It only ever reads attributes — never opens a placeholder, which is what
  would trigger a download.
- **Bikes you no longer ride can be removed from the Presets picker.** The Bike list is what
  your profile carries, not what's installed — the game adds a column for every bike you have
  ever sat on and never takes one out, so bikes whose mod is long gone kept filling the list
  with nothing in the Library to uninstall. Each row now has a trash can that clears that bike
  and the look saved for it out of `profile.ini`, with the previous file kept beside it as
  `profile.ini.bak`. Nothing installed is deleted, and riding that bike again adds it back.

### Changed
- The two bootstrap scripts are rendered and parsed by PowerShell in CI. They run in exactly one
  place — as user-data on a Windows instance with no console, where every failure path destroys
  the evidence — and they are assembled by string interpolation, so a mis-escaped backtick is a
  broken build nobody can see. A stage the control plane would reject is caught there too.

- The supporters list now renders in the order `supporters.json` is written in, rather than
  alphabetically within each tier — the file is hand-edited, so its order is a decision.

- "Repair runtimes" no longer offers to put `msvcr90.dll` where the game looks for it, because
  there is no such place — nothing app-local short of a full private assembly can satisfy the
  VC9 CRT. It installs the runtimes this PC is short of, both architectures, and clears out
  what older versions of this app left behind. Plugins with a plain `MSVCR90.dll` import go
  back to reporting "MSVCR90.dll was not found"; they were never actually fixed, only given a
  different error.

- Starting FrostMod is logged on Windows, on success and on failure, as it already was on
  Linux and macOS — the platform nearly every report comes from was the one saying nothing.

### Fixed
- **The Library no longer loses the name and picture of a mod that is merely offloaded.**
  A `.pkz` whose bytes live in iCloud or OneDrive reads as an empty archive, so its details
  came back blank. They are now recognised and left for a moment when the file is really
  there. macOS gained the offloaded-file check it previously only had on Windows, which also
  makes the existing "your mods aren't really on disk" warning work there.
- **Uninstalling a mod stored in iCloud no longer fails on macOS.** Deleting went through
  Finder, which refuses a file iCloud has offloaded — *"the item needs to be downloaded"* —
  so with a mods folder under `Documents` and most of it offloaded, uninstall failed on
  nearly every mod. It now goes through `NSFileManager` instead, which handles an offloaded
  file without downloading it first and still puts the mod in the Trash, recoverable.

- **One stuck file could hang a build indefinitely.** Each bike now gets a download budget
  scaled to its own size — the time 100 KB/s would need, and never less than five minutes —
  alongside a stall detector, so a transfer that stops sending costs that one bike rather than
  the whole machine. `--retry` never helped: a connection that stays open and goes quiet is not
  a failure it can see. The flat five-minute cap this started as would have been its own bug —
  the pack's two biggest bikes are 184 MB, so the slower the instance, the more certain it was
  to drop exactly the OEM bikes somebody built the server for.
- **A bike that misses gets a second pass**, and a build that still comes up short records which
  ones went missing instead of reporting the same "installed" as a complete build.
- **A build says how far through the bikes it is.** The step was announced once and then went
  silent for the length of a four gigabyte download, which is indistinguishable from having
  hung — and for three quarters of an hour, it was.

- **MX Bikes crashed with "R6034 — An application has made an attempt to load the C runtime
  library incorrectly", and this app was the cause.** Since v0.9.2 the FrostMod status poll
  copied `msvcr90.dll` out of `WinSxS` into the game folder on every start, meaning to serve
  the plugins with a plain `MSVCR90.dll` import that the redistributable strands. The
  reasoning was that a loose DLL is invisible to side-by-side binding and so could only ever
  help those plugins. Half right, and the wrong half mattered: the VC9 CRT polices this
  itself. `msvcr90.dll` checks at load time that it was resolved through a
  `Microsoft.VC90.CRT` activation context, and a loose copy beside the exe is by definition
  outside one, so it refuses to start and takes the process down with it. The copy was
  therefore never once useful — a plugin carrying a VC90 manifest resolves out of `WinSxS`
  and never looks at it, and a plugin without one finds it and dies. We had turned a plugin
  that quietly failed to load into a modal that killed the game.
- **The app now removes the copy it made.** Deleting the code that placed it does nothing for
  the players already carrying one, so the same poll that used to lay the file down now takes
  it away, and "Repair runtimes" does too. It only ever deletes a `msvcr90.dll` whose bytes
  match a VC90 assembly on this PC — that's where ours came from, and it's what stops us
  reaching for a file somebody else put there on purpose. A `Microsoft.VC90.CRT` folder beside
  the exe is left strictly alone: that arrangement is a real side-by-side identity, it
  satisfies the CRT's check, and it works. If the game is open and holding the file, the next
  start tries again.
- **A loose `msvcr90.dll` no longer counts as "VC90 is present".** It isn't a route to the
  CRT, and counting it let a file we ourselves planted quietly satisfy the check that should
  have been reporting the machine had no runtime at all.

- **Steam's own runtime no longer reports as suspicious.** `tier0_s64.dll` and
  `vstdlib_s64.dll` are loaded into every Steam game from the Steam install, which is
  neither the game folder nor the system folder, so every scan on every machine ended with
  two Suspect findings against a rule list holding no signatures. Two false positives on
  every single run is how people learn to ignore a verdict.
- **Log timestamps are local time.** They were UTC while FrostMod's log is local, so reading
  the two side by side meant holding a timezone offset in your head, and support threads
  were getting it wrong.
- **Opening a mod no longer dead-ends on a Cloudflare refusal.** mxb-mods.com guards its
  rendered pages far more tightly than its JSON API, so the catalog could browse fine while a
  single mod came back "refused the request (403)". The fallback that exists to rescue that
  couldn't: it asked the browser to `fetch()` the page, and a `fetch()` of a challenged URL is
  answered with the check itself — only a navigation clears it. The hidden window now navigates
  to the page and reads it, and the HTTP client asks for a page the way a browser does rather
  than as if a script wanted JSON.
- **The hidden mxb-mods.com window no longer reports itself ready while still on the check.**
  It only asked whether script could run, which is just as true on Cloudflare's "Just a
  moment…" page, so requests were sent from the interstitial and refused.
- **The mod page's error now has the Retry button it tells you to hit**, and says so in your
  own language instead of English.

- **The in-game overlay opens where you can see it.** Once MX Bikes was minimized — which
  exclusive fullscreen does the moment the overlay takes focus — the hotkey placed the
  overlay ~32,000px off the left of the desktop: shown, focused, and invisible, so it read
  as a hotkey that did nothing, and it took focus off the game on its way. A minimized game
  no longer contributes a position, and the overlay is kept inside the bounds of whichever
  monitor the game is on.
- **FrostMod is re-armed when the game restarts.** It was only ever started automatically at
  app launch, so a second race in one sitting ran without it — no live reloads, no model
  swaps, no indication anything was different. The app now notices the game starting,
  whether that came from the Play button, Steam or the desktop.
- **The FrostMod pill says when FrostMod isn't reaching the game.** "Running" only ever meant
  the launcher was up. When the game runs as administrator and MXB App doesn't, FrostMod
  can't get into it at all — no in-game pill — and the app reported it as running the whole
  time. That case is now named, with both ways out of it (run the game without administrator,
  or run MXB App with it).

## 2026-08-18 — v0.10.0 — A Designer that sets itself up, a Downloads page, and tracks in 3D

The first full release since v0.9.1. It folds in everything the v0.9.2 betas carried, so
the terrain viewer, the voice device settings and the rest of that line are all in here.

Worth knowing: **voice chat still doesn't transmit** — this is the device half (mic, output,
levels and the key that opens the mic), with test buttons that work. The codec and the voice
room are still to come, and the Settings section says so. Terrain is read from the game's own
height file and checked against four published tracks, not every track there is; if one comes
out looking nothing like the track you ride, name it in the report.

### Added
- **The Designer sets up its own sheets.** Create the sheets a model asks for in one click,
  with ＋ buttons beside Sheets and Layers. The model's own mesh is asked what it draws, so an
  OEM bike's plastics are on the list instead of only its wheels, and picking a model dropped
  from 19s to 0.6s.
- **A Stock texture underlay** draws the bike's own artwork — the plastics embedded in an OEM
  model — beneath the UV map, so the one sheet with nothing to trace finally has something under
  it.
- **The sheet says where you are.** Hovering names the part, its flank (left or right), and
  whether you're over a face you'll see or an underside you won't; the spot under the pointer
  lights up on the 3D model. Where a bike unwraps both flanks onto one island it says so, with
  the left washed warm and the right cool.
- **Double-click a part to fill the view with it**, rather than aiming at it by wheel and drag.
- **Rectangles, ellipses and lines are editable layers** now, not flattened pixels — each lands selected with handles and stays resizable, its colour, fill-or-outline and pen width still editable.
- **Undo covers the whole editor** — adding, moving, scaling, rotating and deleting layers and
  sheets are all on one timeline, not just the last brush stroke — and **the bucket fills the UV triangle under the press** rather than the whole mesh group, which was flooding panels the press never pointed at. Right-click fills the whole island.
- **The 3D viewer reloads a paint that changes on disk**: re-save, and the model re-dresses
  itself without closing the dialog. Preview a model swap before you switch to it, and take a
  helmet off the rider to fill the frame with a **Gear only** toggle.
- **A Downloads page** — everything you've installed, newest first, grouped by day, with where
  each one landed and which mirror served it, searchable by name, destination or host.
- **Failed downloads are kept** with their error and retry in place through the same queue, and
  a **"Recently added" sort** in the library covers everything already on disk.
- **The download queue opens** into a panel listing the transfer in progress and everything
  behind it, and **any download can be cancelled** — one waiting simply leaves, the one in
  flight is stopped mid-transfer and its partial file cleaned up.
- **See a track's terrain in 3D.** The viewer reads the heightfield out of a track and draws the
  ground, coloured by the surfaces the track names and lit by its own hollows, from the library
  detail view next to View in 3D.
- **Share any track or paint with a code.** Sharing was a preset's privilege; now anything the
  Library lists goes into a code with **Share**, or several at once with **Select**. The other
  end is **Import → Paste a share code…**, and files land back in the folders the sender had them
  in.
- **FrostMod installs and runs on Linux and macOS** — in the game's Proton prefix on Linux, in
  the CrossOver/Whisky bottle on macOS. Reload works from outside the prefix or bottle (a polled
  command file; needs FrostMod v0.13.0 or newer), and the FrostMod Settings section and log row
  now appear on both.
- **"Repair runtimes" in Settings** installs every Visual C++ runtime the PC is short of — both
  32- and 64-bit — and puts `msvcr90.dll` where the game looks for it, elevating when the game
  lives under Program Files. It's always available rather than gated on a detected fault, and
  hands over download links for anything it couldn't finish.
- **Voice chat settings** — pick the microphone you're heard through and the headset everyone
  else comes out of, with a live meter and a test tone, plus a **push-to-talk** key that works
  while the game holds focus and a **toggle-to-talk** mode with a live Mic-open indicator.
- **Your logs, in Settings → Logs.** All three log sets named on one page, with how many files
  and how old the newest is; **Open folder** and **Save logs…** into a single zip that never
  carries the settings file.
- **Mod cards carry a byline**, so two versions of a track are told apart without opening both.
- **Qwest and Kelso** credited on Settings → Supporters.

### Changed
- **Downloads is now part of the Library** — an expandable group under it in the sidebar rather
  than a tab beside it. A failed download still shows its count on the Library row while the
  group is shut.
- **The install picker shows every download**, the recommended one preselected, dedicated-server
  builds badged *Server* and folded under a disclosure rather than dropped. The mirror list opens
  on more than one row, each with the author's own name for the file.
- **The mirrors hint line is gone** from the download dialog — the list already reads as a list
  of the same file.
- **Settings shows one section at a time**, with the left nav grouped into Setup, App, Advanced
  and About.
- **The UV map is on by default**, "Start from a paint…" is offered only while the canvas is
  empty, the sheet list scrolls, and Add image / Add text moved down beside the drawing tools.
- **Back to the game's own model in one click** — every bike gets a **Stock** row in the Locker,
  and choosing it files the loose model away (never deletes) along with the setup files a swap
  brought with it.
- **The ReShade card takes a folder of your own** and names where it looked, kept separate from
  the game folder.
- **The process is called "MXB App"** in Activity Monitor and Task Manager, not the name of the
  Rust crate behind it.

### Fixed
- Quick install queues the whole selection the moment you click — each row marked *Preparing* —
  and names any mod it skips rather than reporting a bare count.
- **MediaFire** resolves through its own API instead of the shape of its page; folder links
  install, and a removed, password-protected, taken-down or bandwidth-exhausted file is each
  named with what to do about it.
- **Double-click to focus a part works**, recognised from the presses themselves rather than a
  `dblclick` WebKit won't deliver under pointer capture.
- A dedicated-server build is no longer installed as if it were the mod, and quick install stops
  guessing on a mod that offers nothing but server files.
- **The 2D sheet is shown the right way up.** It was displayed upside down against the template painters work from — the forks landing top-left where the template has them bottom-left. The flip is the view's alone, so a paint loaded and saved back is byte-identical, and a decal placed upright now lands upright on the bike.
- **Designer:** a paint no longer ships blank sheets — which removed the bike's own normal and
  roughness maps and ran to 350 MB — and blank companion maps aren't offered for creation; a
  collapsed UV triangle no longer answers for the whole sheet; one junk vertex can't switch
  left/right off for a bike; the flank named is the right one; a bike loaded without its `.geom`
  gets no invented sides; a bike archive with no mesh fails instead of loading its stand-in; the
  empty-canvas nag is gone.
- **Gear:** a packaged mod bought from the shop no longer lands a folder too deep — and the Rider
  tab offers to raise ones already buried — and a one-piece helmet or boot renders at all, and
  upright.
- **Importing a full bundle fills the gaps** rather than overwriting mods you already have.
- **Terrain** is read signed (no eleven-metre wall below the datum), drawn the right way round
  rather than mirrored, at roughly four times the detail, with the blind heightfield probe and
  the relief exaggeration both corrected.
- **The AppImage no longer ships its own libwayland** — the cause of the SteamOS white screen —
  and the XWayland workaround that never ran is dropped.
- **Missing-runtime checks:** the MSVCR90 side-by-side case gets a `msvcr90.dll` beside the game
  executable, a game folder carrying its own CRT is no longer called broken, and
  `vcruntime140_1.dll` is now probed.
- **Nothing downloads without a ceiling:** a thumbnail is refused above 32 MB and an embedded
  texture is only decompressed as far as it claims to go, so neither a card image nor a scanned
  model record can pull unbounded memory.

### Notes
- Voice is **off until turned on** — a feature that opens a microphone shouldn't be discovered by
  accident — and every global shortcut is now rebound from one place.
- Buy Me a Coffee donations post to the Discord for money-in events only, carrying a name and
  their note and nothing else — no amount, no email.
- Audio comes from `cpal` pinned to 0.15; the Linux build and CI install `libasound2-dev`.


## Unannounced — server provisioning and paint publishing

Also kept out of the release notes: both need an account on the invite-only control plane, so
these fold into the notes of whichever release opens enrollment up. They join the servers and
paint-sync work described further below.

### Added
- **Servers launch from a prebuilt image.** Every server used to download and install the same
  6 GB — the game, then the whole bike pack — which was the entire wait. That happens once now,
  into a machine image, and each new server writes its own config and starts: two minutes instead
  of fifteen. Building the image is a one-off `POST /v1/images/build`.

### Fixed
- **The idle sweep no longer destroys the machine building the server image.** Builders were left
  out of the query that lists known instances, so the next check terminated them as untracked
  within five minutes of launch. They are listed and skipped at the idle check now, and a build
  whose machine disappears clears itself instead of blocking every later build.
- **A provisioned server can see the bikes it was given.** The pack was downloaded into the game
  folder, but MX Bikes reads mods from PiBoSo's user folder — the files were on disk the whole
  time and the game was never looking there. They now install where the game reads, with the game
  folder linked to the same content.
- **Paints larger than 32 MB no longer fail a publish.** A loadout is checked as a whole, so one
  4K livery meant publishing nothing and looking default to everyone; the limit is now 192 MB and
  anything still over it is left behind and said out loud. Only publishable slots are sent, one
  paint per slot, and a refused publish now carries the control plane's reason.

## Unannounced — a debugger can't ride along

Left out of the release notes on purpose: a hardening measure advertised is a hardening
measure a reverse engineer knows to look for and patch out first. It folds into whichever
release it ships under, unnamed.

### Security

- **Release builds refuse to run under a debugger.** The release profile already pays a lot
  to make the shipped binary hard to read at rest — symbols stripped, fat LTO to dissolve the
  call structure, no debug info — and all of it is undone the moment someone attaches a live
  debugger to the running process: a breakpoint on the paint decrypt, a peek at the
  `cf_clearance` the session is holding, a step through the IPC guard. So the process now
  checks for an attached usermode debugger — `IsDebuggerPresent`/`CheckRemoteDebuggerPresent`
  on Windows, a non-zero `TracerPid` on Linux, the `P_TRACED` flag on macOS — at startup and
  on a slow poll after, and quietly exits if it finds one. The poll matters because attaching
  to a process that is already up is the usual way in, so a one-shot check at launch would
  miss the very thing it is meant to stop. Debug builds are exempt by construction — every
  check is compiled only into release, so `tauri dev` and the test suite stay debuggable.

## Unannounced — servers and paint sync

These ship in the app but are deliberately left out of the release notes: both need an
account on the control plane, which is invite-only, so announcing them would advertise
something almost nobody can use yet. They fold into the notes of whichever release opens
them up.

### Added
- **Create a server from the app.** The control plane launches an EC2 instance, installs
  the dedicated server and the agent on it, and the server appears in the Servers page. The
  app holds no cloud credentials — a desktop binary can be unpacked, and a key inside one
  would let anyone create infrastructure in our account — so the AWS key lives only as a
  Worker secret, scoped by IAM to instances tagged `mxb:managed` in a single region.
- **Cloud servers have a lifecycle.** Booting, ready with an address, a Join button, start and
  stop, a track picker, whether it's in the public list, and how many minutes until it shuts
  itself down. Previously the panel showed a raw instance id and a state string.
- **A booting server now reports its progress, and its failure.** Creating a server launches a
  machine that runs a long script with nobody watching: no console, no key pair, so
  `C:\mxb-bootstrap.log` cannot be read from outside — and the script's own failure trap shuts
  the instance down, which terminates it and destroys the log. A bootstrap that died at minute
  twelve and one still downloading a 2 GB installer were indistinguishable: a server that never
  turned up. The instance now posts each step to the control plane, and posts the tail of its
  transcript *before* shutting down, so a failure leaves an explanation behind.
- **"Create a server" says which step it is on** — `downloading the game`, `extracting the
  game`, `installing the agent` — instead of spinning for a quarter of an hour with nothing to
  show, and shows the reported error if the server gave up.
- **Publish a server you run to the public list, from the app.** Until now `servers` rows
  were hand-written SQL, so running a server and *having anyone able to join it* were
  separate problems — the second one solved by passing an address around privately. A
  server you manage in the app now has an "Add to the server list" button, and it appears
  in everyone's Join a server picker.
- **Nothing is typed to publish it.** The address is the agent's own host joined to the port
  it reports, and the name comes from the server's `.ini`. The only thing asked for is the
  region, which is the one fact no machine on this end can work out.
- **Your paints publish themselves.** Whenever you change your look, the app sends it up a
  second or so later — one publish for a burst of preset-flipping, not one per click. This
  closes the loop: publishing had a command but no button anywhere in the app, so nothing
  was ever uploaded and every roster was empty by construction.
- **Everyone else's paints arrive on their own**, pulled when you launch the game. Joining
  by address syncs that server; pressing Play syncs every server in the registry, since the
  game you're about to pick from the in-game browser never passes through us. Rosters are
  merged and de-duplicated first, so two servers sharing riders is one pass over the disk.
- **A paint-sync panel that says what's missing.** Publishing and syncing both happen in the
  background off actions you didn't ask for, and their only report was a line in the log — so
  "is this working?" had no answer anywhere on screen, and the most common failure looked
  exactly like success. The Servers page now shows, in a sentence each: whether your look is
  published and when it last went up, how many riders' paints you hold, and — when it isn't
  working — what that means and which button fixes it. It updates live as the background work
  runs.
- **A sync line in the sidebar**, alongside the FrostMod one, so the state is visible without
  going to look for it.
- **One-line pairing.** `mxb-agent` prints a code at startup carrying its own address and
  token; paste it into Add a server and both fields fill in. `public_url` in `agent.json`
  overrides the address for hosts behind NAT or a proxy.
- **`mxb://enroll?code=…` links**, so an invite can be clicked instead of transcribed. The
  link only prefills the field — enrolling is still a button you press, because a URL any
  website can open must not be able to spend an invite on its own.
- **`GET /tracks` on the agent**, so setting a track is a list of what that host actually
  has instead of a text box you spell a track name into from memory. Your own PC's library
  can't answer this — the operator's machine and the server box are different installs.
- **`POST /v1/servers` and `DELETE /v1/servers/:id`** on the control plane, with ownership:
  a server is recorded against the account that registered it, only that account can remove
  it, and the hand-seeded rows have no owner so the API can't touch them. Five servers per
  account, and one registration per address.
- **`GET /v1/servers/mine`**, which hands a server's agent token to the account that owns it —
  the only way to drive a box that has no console and prints its pairing code to nobody.
- **`POST /v1/servers/:id/bootstrap`**, authenticated by the agent token the box already holds,
  the same credential and the same refusal as `hello`. Stage names are validated and the stored
  log is capped and kept only on failure.
- **The agent ships as a build artifact**, served from R2 through the control plane. A
  booting instance holds no credentials and has no way to be given any, so that URL is
  unauthenticated by necessity; the bucket itself stays private.
- **A guided-tour step for the Servers page**, and a help hint on it. Every other screen had
  one.

### Changed
- **Nothing runs unattended.** Four separate things have to fail before a server can bill
  indefinitely: a cap of 2 concurrent instances counted *from EC2 rather than our own
  records*; destruction after 20 minutes with nobody connected; termination of any instance
  no database row points at; and a hard maximum lifetime that catches everything else — a
  hung bootstrap, an agent that never started, a failure nobody has thought of.
  Instances also launch with `InstanceInitiatedShutdownBehavior=terminate`, and the
  bootstrap's failure trap shuts the machine down, so a half-built server destroys itself
  rather than sitting idle on the bill.
- **A server is only advertised once we can reach it.** The control plane calls the agent's
  unauthenticated `/health` before publishing; an unreachable one is recorded and stays
  manageable, but is kept out of everyone's picker. A list full of servers nobody can
  connect to is worse than a short list. This can't prove the *game* port is open — that's
  UDP, and a Worker can't send one — so "reachable" means the host answers.
- **Addresses pointing into private space are refused outright**, before any request is
  made. The control plane fetches an operator-supplied URL, which makes it a
  request-forgery surface: `127.0.0.1`, RFC1918, carrier-grade NAT and the `169.254.169.254`
  metadata address are all rejected, as is any URL carrying credentials, a path or a query.
- **Join a server offers the servers we know about**, instead of an empty box wanting an IP
  address. The control plane has held a registry of them since the first migration and
  nothing ever showed it, so the answer to "where do I get the address" was nowhere in the
  app. Typing one is still there, for a server that isn't listed.
- **The server registry is public.** It was bearer-only, which meant the players most in
  need of a list — the ones who have never joined a server and have no address to type —
  were the only ones who couldn't see it. It returns the same name, region and address a
  server browser shows; `agent_url` is still withheld, so no admin API is advertised.
- **Adding a server shows one field, not four.** The pairing code carries the address and
  the token and the host supplies the name, so the manual fields sit behind a disclosure
  rather than implying they all need filling in — and the form now says where the code is
  printed.
- **Adding a server checks it before saving it**, and takes the server's name from the host
  rather than asking you to invent one. A wrong address or token now fails at the form
  instead of becoming a row that never loads.
- **The enroll panel says where an invite code comes from**, with a button through to the
  Discord. Invites are issued by hand and the field previously explained none of that.
- **Your rider name is picked from your MX Bikes profiles**, not typed. It had to match the
  game exactly and nothing checked that it did, which made a silent no-op the most likely
  outcome of enrolling. Typing is still there when no profiles are found.
- **Your GUID is claimed automatically** the first time one of your servers sees you
  connect — the app reads it from the server's log, where the game writes it next to your
  name. You can't read it off your own machine, so asking you to type it never made sense.
  The manual field is still there, one click away, for anyone not running a server.
- **Join a server is behind the Experimental switch**, with the rest of the multiplayer
  work. It rides on an undocumented connect flag, so it shouldn't sit under Play for
  everyone while that's still unconfirmed.
- **Publishing a whole profile costs one library scan, not one per bike.** Resolving a look
  walks `mods/bikes` recursively — every livery installed — and the publish path did that once
  per bike plus once per uploaded file. Loadouts are now planned in a batch against a single
  walk, and the upload takes its bytes from the files already hashed.
- **`GET /v1/fleet` no longer hands every enrolled account the address of every server.** The
  running count stays public to everyone, because that is what the concurrency cap is measured
  against; the instance list is scoped to its owner.
- **The on-screen description of paint sync matches what it does.** It promised automatic
  publishing on every look change before that was true.

### Fixed
- **Your look is published even if you never open the Locker.** Publishing only ever ran off
  a preset apply, so the commonest path there is — enroll, press Play — sent nothing at all,
  and the player appeared to everyone else in default gear with no indication anything was
  wrong. It now publishes when you enroll, when the app starts, when you press Play or Join,
  and whenever a preset is applied.
- **Changing your kit in the game's own garage publishes it.** MX Bikes writes the same
  `profile.ini` the app does; nothing was listening. A watcher on that file now catches it,
  filtered to `profile.ini` alone so the replay and telemetry churn a session produces can't
  set it off. Redundant fires cost nothing: the look is hashed and an unchanged one is never
  sent.
- **Switching Experimental on now starts watching for look changes straight away.** The
  watcher that notices you changing kit in the game's own garage was only ever started when
  the app launched, so turning the feature on left it stopped until the next restart — the
  app kept publishing when you applied a preset or pressed Play, but a change made in the
  garage went unnoticed for the rest of the session. That is the session you have just
  enrolled in, which makes it the worst one to be quietly missing. Switching it back off
  stops the watcher rather than leaving it running.
- **Every bike is published, not just the last one touched.** Storage held one loadout per
  account — `loadout_paints` had no bike dimension at all — so publishing a second bike
  deleted the first. A rider looked right on whichever bike the app last saw and default on
  every other. Both tables are rebuilt keyed by bike, and the app sends them together.
- **The sync no longer overwrites liveries you made yourself.** Any local paint whose name
  matched an incoming one was replaced, including your own artwork. It now records what it
  installs and will only ever replace that; anything else is yours and is kept, and reported.
  Two riders using one file name for different paints installs neither, rather than letting
  whichever roster answered first decide what everyone sees.
- **A server created from the app can now be reached.** Provisioning launched a machine and
  then lost it: the response carried no address and no token, nothing ever filled either in,
  and the row stayed unpublished forever — so the server could not be joined, managed or even
  deleted, only waited out by the idle reaper. The instance now announces itself once its
  agent answers, which fills in its address and puts it in the join picker.
- **A server whose name isn't plain Latin-1 can be created.** EC2 user data was encoded with
  `btoa`, which takes a "binary string" of one character per byte and throws on anything above
  U+00FF. Any server named with an emoji or a non-Latin script failed to launch with
  `InvalidCharacterError` and nothing else to go on. Found by trying to launch a real one.
- **A provisioned server can actually download the game.** The bootstrap fetched the installer
  with `Start-BitsTransfer`, which cannot work where it runs: EC2 user data executes as SYSTEM
  at boot with no interactive session, and BITS refuses with `0x800704DD` — "the user has not
  logged on to the network". Every launch died at that step. It now uses `curl.exe` from
  System32, which streams to disk without a session and retries on its own, and the size is
  checked afterwards so a truncated download fails loudly rather than extracting into nonsense.
  Found by launching a real one, and diagnosed in a single attempt because the instance now
  reports its transcript before shutting down.
- **A provisioned server's agent can read its own config.** `agent.json` was written by hand as
  JSON inside a PowerShell here-string, and the install path interpolated with single
  backslashes — so the file said `"game_dir": "C:\mxb\\game"`, and `\m` is not a valid JSON
  escape. The agent refused to parse it and exited immediately on every server ever
  provisioned. It is now built with `ConvertTo-Json`, which escapes properly, and the file is
  parsed once on the box before the agent is asked to.
- **The idle reaper could never have reaped anything.** It read the agent address from a
  column that provisioning cannot fill in — EC2 assigns the public IP while the instance
  boots, long after the row is written — so every provisioned server looked permanently
  unreachable and was skipped forever. It now takes the address from EC2's own view.
- **Paint sync no longer syncs one hard-coded server.** The Servers page asked for
  `eu-frankfurt-1` by name regardless of where you were riding; it now resolves the server
  you actually joined, matching a registered one by address or falling back to the address
  itself.
- **The agent's track list no longer offers things that aren't tracks.** A folder tracks are
  filed into (`EU`) and the interior of an extracted one (`data`) both came back as
  selectable names, and picking either would have restarted the server into nothing. It now
  uses the same marker files the app's own library scanner keys on, and stops descending
  once it has found a track.

## 2026-08-10 — v0.9.1 — The Designer paints, Play works on a Mac, and SteamOS stops opening to a white screen

On top of v0.9.0 — everything that release added is still the news, and its notes are
repeated below.

Worth knowing: the Mac launch has been tested against a stand-in for Wine, not
against a real bottle, so if Play does something odd on your machine the app log names the
exact command it ran — please send it.

Also worth knowing: the Designer's new UV map has been checked against synthetic meshes, not
against a real bike's geometry — and fitting and clipping a layer to a part both read from it,
so all three are right or wrong together. Hover the sheet with the map on: if the part it names
isn't the panel you're over, that's the thing to report, and it explains the rest.

### Added
- **You can paint on a template.** The Designer could stack images and text on the sheets of an
  unpacked paint, but it could not put down a single pixel — a fade across a shroud or a stripe
  cut to follow a panel seam meant leaving for an image editor, exporting, coming back, and
  losing the live-on-the-model loop that is the point of the tab. It has a tool kit now: a soft
  brush with size, edge and strength, an eraser, a gradient, a fill, and rectangle, ellipse and
  line. They land on the 2D sheet and on the 3D model at the same time, while you drag.
- **A gradient that carries one colour into another.** Drag across the sheet to say where the
  transition happens — the drag is the axis, not the extent, so everything before it is the
  first colour and everything past it is the second. Linear or radial, and it can end at
  nothing instead of at a colour, which is the version a livery usually wants.
- **Painting happens on its own layer.** A "Paint layer" sits in the stack like any other, so it
  gets opacity, blend mode, visibility and stacking for free — and the template underneath is
  never written to, so hiding the layer gets the untouched template back. Picking up a brush
  with nothing to paint into makes one rather than doing nothing.
- **Undo, for strokes.** ⌘Z and ⇧⌘Z step through painting, remembering as much as fits in a
  fixed memory budget — six steps on a 2048² sheet, a couple of dozen on a small one. Layer
  edits are not part of it.
- **Tools have keys.** V, B, E, G, F, R, O and L. Hold Shift for straight lines, squares and
  circles; right-drag still pans, which matters more with a brush than with a logo.
- **The Designer can show you what you're drawing on.** A livery is drawn flat and worn curved,
  and the flat version gives away almost nothing about which rectangle of it ends up on a
  shroud — so a sheet has a **reference underlay** now, and it answers that two ways. **Template**
  lifts the paint you started from *out* of the sheet and shows it faintly underneath to trace
  over; it stops being part of what you save, which is the difference between drawing *from* a
  paint and drawing *over* one. **UV map** reads the model already standing beside the canvas
  and draws its bodywork where it actually lands on the sheet, each piece in its own colour —
  the thing an image editor cannot tell you. Neither is ever composited, staged or saved: they
  live outside the sheet entirely, so a guide cannot end up shipped inside somebody's livery.
  Both sit **underneath** what you're drawing — a ghost, not an overlay — so they show through
  wherever the sheet is still transparent, which is exactly where you need to know which piece
  of bodywork you're about to paint on. The two are meant to be used together: lifting the
  template out is what leaves the sheet transparent enough to see the islands through it, and
  the panel says so when a still-opaque template is burying the reference.
- **Layers resize by their corners.** The selected layer's box has grabbable handles, dragged
  from the centre the same way the inspector's slider scales from it, so the two controls agree
  about where a logo grows from. The slider stays for typing an exact number; the corners are
  for the other 90% of the time. Handles are still drawn on a layer too small to grab one —
  the box is how you see what's selected — but the grab itself steps aside there rather than
  leaving a tiny layer resizable and no longer draggable.
- **A sheet name the model never asks for now says so.** Turning on the UV map for a sheet
  nothing on the mesh binds reports it in the panel instead of drawing an empty overlay, which
  had been indistinguishable from one still loading. It's the same mistake that makes a paint
  load and show nothing, caught before the save rather than after it.
- **Drop a photo onto a specific piece of bodywork.** The UV map could show you where the shroud
  was; it couldn't help you put anything there. Now the model's parts are a list you can pick
  from, and a layer pinned to one gets two things: **Fit to part** places and scales it to cover
  that panel, and the layer is **clipped** to the panel's outline, so a photo grabbed off the
  internet stops at the seam instead of bleeding across the fender next to it. Fit *covers*
  rather than fits inside — a livery panel with the sheet showing through at two edges isn't
  what anyone meant — and the clip is what trims the overspill. It works on a paint layer too,
  which is how you brush freely and still stay inside the shroud. Parts split across several
  mesh nodes under the same name are listed once, biggest first: a bike's shroud is regularly
  three nodes that all call themselves `shroud`, and three entries holding a third of the
  answer each would be a worse menu than none.
- **The sheet says which part you're pointing at.** Hovering names the piece of bodywork under
  the cursor, and outlines it while the UV map is on. The islands say a panel is *there*; only
  the name says which one it is — and it's the fastest way to check the map is landing where the
  bodywork actually is.
- **Play and Join Server work on macOS.** MX Bikes is a Windows game, so on a Mac it runs
  inside a CrossOver, Whisky or Wine bottle — and the app, which had a launcher for Windows
  and one for Linux, simply refused: *"Launching MX Bikes is supported on Windows and Linux
  only."* It launches now, through whichever wrapper owns the bottle the game sits in. That
  bottle is worked out from the install folder itself: every wrapper keeps its fake `C:` drive
  at `drive_c`, so whatever is above it is the prefix, and no wrapper's private layout has to
  be guessed at. CrossOver bottles are started by name, the way CodeWeavers documents, so the
  bottle's own graphics settings apply instead of being skipped; everything else is pointed
  straight at the prefix. Joining a server goes the same way, connect flag and all.
- **The app finds a bottled install by itself.** Setup and Settings only ever looked where a
  *native* Mac game would be — Steam under `~/Library`, mods under `~/Documents` — neither of
  which a Windows game inside a bottle ever touches. A Mac player had to type a path like
  `…/Bottles/MXB/drive_c/…` by hand before anything worked at all. Both detectors now search
  the CrossOver and Whisky bottles on the machine, including a Windows Steam installed inside
  one, and the setup screen's folder hint points at the bottle rather than at `Documents`.
- **The sidebar knows when the game is up on a Mac.** Under Wine the game is an ordinary
  process, so Play now shows "MX Bikes running" instead of quietly starting a second copy.
- **A Wine runner picker in Settings**, for when the automatic choice is wrong or the wrapper
  lives somewhere unusual. It also shows which runner was found and how many bottles the app
  can see — so a bottle it *can't* see shows up as missing before you press Play, not after.
- **"What's new" credits the people who paid for it.** The update notice now names the
  supporters underneath the release it's announcing, and opens the full thank-you list in
  Settings when you click it. The names are read live rather than baked into the build, so
  somebody who started supporting after this version was cut is still thanked by it — and the
  credit is part of the modal itself, not of any one release's entry, so every future update
  carries it without having to remember to. It credits and links; the button that asks for
  anything stays on the Settings page, because an update notice is the wrong place to hold
  out a hat.

### Changed
- **A roster means one grid, not the whole platform.** `GET /v1/roster` took a server and
  ignored it, because nothing recorded where anyone was, so every rider downloaded the paints
  of every other enrolled rider. Each app now reports the server it launched into, and a roster
  returns the riders actually on it. A rider whose app goes quiet for ten minutes drops out.

### Fixed
- **The Mac build starts at all.** It aborted on launch before drawing anything — which would
  have made the Mac work above impossible to reach in the very release that added it. The
  drag-drop fix earlier in this cycle moved the main window out of Tauri's startup and into
  the app's own, so the handler can be switched off under Wine; that needs `"create": false`
  in the window config, and it was set in the base config only. macOS keeps its own overrides
  — the rounded title bar — and that file *replaces* the window config rather than merging
  into it, so the flag was dropped on macOS alone: Tauri opened the window itself, the app
  opened a second one under the same name, and the process aborted on the duplicate. Windows
  never had an override, and Linux's doesn't name the window, which is why only the Mac was
  hit and no beta caught it.
- **The Linux app opens to its interface on SteamOS instead of a white screen.** Our AppImage
  carries Ubuntu 22.04's libwayland next to whatever Mesa the host ships — the pairing the
  AppImage excludelist warns about — and WebKitGTK 2.46 and later abort outright when that
  leaves them unable to create an EGL display (`EGL_BAD_PARAMETER`). The window came up and
  nothing ever painted. AppImage runs on a Wayland session now start through XWayland, which
  never goes down that path; both SteamOS modes qualify, Desktop and Game. Installed
  `.deb`/`.rpm` builds link the host's own libraries, so they're left on native Wayland, and
  an explicit `GDK_BACKEND=wayland` still wins anywhere.
- **`MXB_SAFE_GRAPHICS=1`, for a white screen that survives all that** — one variable that also
  turns off GPU compositing and renders in software. The startup log now records the session
  the app began under and every graphics knob it settled on, so the next report of a blank
  window arrives with something to read.
- **A rider who joins after you now shows up properly.** The pull happened once, at launch, so
  paint sync was lopsided: whoever joined last saw everyone, and whoever was there first never
  saw anyone who arrived afterwards — they had pulled before those riders existed. The app now
  keeps checking for the length of the session and installs what turns up.

## 2026-08-10 — v0.9.0 — A studio for paints, the Shop installing what you bought, and presets that carry their own mods

### Added
- **Share a preset as a complete bundle, not just a code.** A share code carries the *names*
  of what a look is made of, which is no use to someone who owns none of it — they get your
  preset and a list of things to go and find. **Create full bundle** in the Share dialog
  gathers every asset the loadout actually references — bike livery, helmet and its paint,
  goggles, suit, gloves, boots, protection, tyres, even a model-swap variant — zips them into
  a `mods/`-shaped tree with the preset beside them, and puts the download inside the code.
  **Full import** at the other end unpacks the lot into the folders the game reads, so a rider
  with an empty mods folder ends up wearing exactly what you built, first try. Folders are
  resolved as they're gathered, so a bundle carries real files rather than links pointing at
  your disk. A bundle over 200 MB is refused up front, with its size named, instead of after
  the upload.
- **A paint designer, and one Studio instead of three tabs.** Designing a livery meant leaving
  for a web editor, exporting a `.tga`, packing it, launching, looking, and starting over —
  the loop was long enough that most of it happened blind. The **Designer** closes it: start
  from a paint already installed for the model (which is how the sheets arrive named the way
  the mesh binds them), stack images and text on top, drag them around, and **the bike or
  rider beside you wears the drawing as you draw it**. Save writes the packed `.pnt` the game
  reads, through the same destination picker, overwrite prompt and folder rules Paints already
  used. No brushes and no filters — pixels still come from wherever you like drawing them;
  this places them and knows where they go.
- **Designer, Paints and Rider now live under one Studio tab**, switched by a segmented
  control. They were three sidebar entries answering three halves of one question, and the
  sidebar had started listing features rather than places; it's back to seven entries.
- **The Studio works under GP Bikes too.** Building a `.pnt` is the same job for either title —
  same container, same encoder, same folders — and only the 3D preview needs part bindings GP
  Bikes doesn't have yet. So Designer and Paints are both there, the preview says plainly why
  it isn't, and the Rider tab (which *is* the rig) stays MX Bikes-only. Paint destinations are
  read from the game's own rider layout rather than listed in the app, so GP Bikes is offered
  its three — bike livery, helmet, rider kit — instead of MX Bikes' boots and protection
  folders it has no use for.
- **The sidebar collapses to icons**, and the Designer's sheets/layers rail folds away, for
  when the canvas and the model are what you want the width for.
- **A new Paints tab turns image files into paints the game loads.** MX Bikes reads a paint
  as a packed container of compressed texture sheets, which no image editor writes — so
  until now a livery drawn in GIMP or Photoshop had to go through somebody else's converter
  before the game, or this app's 3D preview, would look at it. Pick the sheets (`.tga`,
  `.png`, `.jpg`, `.bmp`, `.webp`), say what they're for, and the app builds the `.pnt` and
  installs it where the game expects it: `mods/bikes/<Bike>/paints`,
  `rider/helmets/<Helmet>/paints` or `goggles`, boots, protection, and a rider profile's
  kit or gloves. Or save it to a folder of your own, to share.
- **Unpack an existing paint into editable `.tga` sheets.** This is how you get a template
  that actually fits the model: the sheets come out named the way the mesh binds them
  (`livery`, `rider`, `shell`…), open in any editor, and go straight back in under the same
  names. Extracted to `Documents\MXB App\Paint Templates\<paint>` and loaded into the studio
  in one step, so the round trip is edit → **Reload from disk** → **Save paint**.
- **Names are the part that decides whether a paint works, so the studio shows them.** A
  `.pnt` supplies textures *by name* and the mesh binds whichever names it asked for — a
  sheet called `livery` lands on the bodywork, the same sheet called `my_livery` lands
  nowhere. The studio reads the names the paints already installed for that model use (from
  their headers — no pixels decoded), lists them, and flags a sheet whose name isn't one of
  them before you save.
- **Preview what you just saved on the real model**, through the viewer the app already
  has: a bike livery on its bike, a helmet or goggle paint on that helmet, a kit on the
  rider.
- **Sheets that aren't a power of two are resized to one, and say so.** MX Bikes is a
  DirectX 9 title and its textures are powers of two throughout; a 1000×1000 export — which
  GIMP will happily make — would otherwise be packed into a file the game refuses, and the
  failure would land in-game rather than in the app.
- **Pick a ReShade preset from Settings.** A new ReShade card lists every preset you have —
  the ones the app installed and any already sitting loose in your game folder — and switching
  between them is one click. There's an "Off" entry that runs no effects, so you can compare a
  preset against the stock look without uninstalling anything.
- **Browse and install ReShade presets like any other mod.** mxb-mods' ReShade Presets
  category is now a tab in Browse. Installing one files the preset where the app can find it,
  and puts any effects and lookup textures the download bundles where ReShade looks for them —
  the usual reason a preset "does nothing" after a manual install.
- **Drop a preset on the app.** A `.ini` or an archive of them is recognised as a ReShade
  preset and goes straight to the right place.
- **The app says when ReShade is installed for the wrong graphics API.** MX Bikes and GP Bikes
  render with OpenGL, and ReShade's installer offers DirectX first — a DirectX install is
  never loaded by the game and looks like ReShade simply not working. The card names that
  specifically, and points at the fix.
- **Presets warn when they need effects you don't have.** A preset that switches on a shader
  that isn't installed renders without it and looks like it did nothing; the card lists what's
  missing instead.

  Getting ReShade itself is still a trip to reshade.me — it asks that its binaries not be
  redistributed, so the app detects it and links out rather than bundling or downloading it.
- **Install what you bought on the MX Bikes Shop, from inside the app.** The Shop tab now has
  two halves — **Catalog**, the store's public listing as before, and **My purchases**, which
  signs in to your own mxbikes-shop.com account and shows everything you've already paid for.
  Cards carry the store's own artwork and author, a product that ships several files (PRO/AMS
  and the like) is one card with a picker rather than several, and anything already in your
  library is badged as installed.
- **Purchases install through the same review sheet a drag-and-drop uses.** The file is
  downloaded with a progress bar, then read to see what it actually contains, and the sheet
  says where each piece will land and warns about collisions before anything is written.
- **An Import button in the Library, for setups drag-and-drop never reaches.** Dropping a
  download on the window stays the fast path, but the OS drop event doesn't arrive
  everywhere — remote desktops and some shells eat it — and where it doesn't, there was no
  other way to install a file you'd already downloaded. **Import → Choose files…** (or
  **Choose a folder…**, for an unpacked track) stages exactly what a drop stages: same
  classification, same review sheet showing where each piece lands, same collision
  warnings, and nothing written until you confirm.
- **Presets can use a custom riding style.** The **Riding style** slot only ever offered the
  two styles the game ships, so a style you had installed could not be picked — you could
  type its name in by hand, but the preset then flagged it as a mod you were missing, never
  packed it into a share code, and Manage was free to park it right before a race. The slot
  now lists what is installed in `mods/rider/animations` alongside the stock `mx` and `sm`,
  a shared preset carries the style with it like it already carries helmets and paints, and
  Manage keeps it. **MX Bikes and GP Bikes both**, which is the point: the two games load
  riding styles from the same folder and record the pick in the same `[riding_style]` line.
- **MX Bikes lists riding styles in the Library**, in the Rider tab under their own heading,
  and offers `mods/rider/animations` when you install one. GP Bikes already did both; MX
  Bikes was treated as if it had no such folder, so a riding style installed there was
  invisible to the app.
- **FrostMod can be stopped from the app.** The sidebar pill and the FrostMod section in
  Settings now offer a stop control while it's running, next to the reload and start ones —
  until now the only way out was Task Manager or quitting the app from the tray. It stops
  FrostMod whoever started it: a `frostmod.exe` left behind by an earlier session, or one
  launched by hand, is no longer out of reach just because this app didn't spawn it. The app
  waits for the process to actually go before saying it stopped, so a FrostMod that survives
  the attempt (running elevated, or as another user) is reported rather than papered over with
  a success message the status pill contradicts a second later. Stopping is a one-off — it
  doesn't touch the "Run FrostMod automatically" setting, so the next app launch behaves as
  configured.
- **"msvcr90.dll was not found" is now something the app explains and fixes.** MX Bikes is
  a Visual C++ 2008 build, and it asks Windows for that runtime by manifest rather than by
  path — which resolves either machine-wide or out of a private copy sitting in the game
  folder. On a PC living on the second one the game launches perfectly while nothing loaded
  from anywhere else can find the runtime, and FrostMod is loaded from somewhere else. The
  result was the worst kind of bug report: the game works, FrostMod doesn't, and Windows
  puts a bare error box over the screen that names a DLL and nothing else. The app now
  checks for that runtime, and for the newer one `frostmod.dll` itself needs, says which is
  missing in plain language, and installs it from Microsoft with one button.
- **The warning goes where it'll be seen.** A bar at the top of the app, not just a line in
  Settings — nothing about the symptom suggests Settings is where to look.
- **A beta now announces itself in the beta Discord channel.** Until now a suffixed tag
  built and published quietly and testers only heard about it if someone told them, because
  the announcement job skipped pre-releases outright. It runs for them now, and the tag
  decides which channel it reaches: a beta posts to the beta webhook, a full release to the
  release one, so testers hear immediately while everyone else still hears about a version
  once — when the updater can actually hand it to them.
  - The beta message says what it is before it says what's in it — a beta build of the
    version it names, which the updater won't offer you, so install it from the links in the
    message. Same fact the release page leads with. It's titled and coloured as a beta, and
    reads the changelog section for the version it's a build of, so it carries that
    release's headline once it's written and simply omits it before then.
  - The beta webhook is its own `DISCORD_BETA_WEBHOOK_URL` secret and the script never falls
    back from one webhook to the other: a beta landing in the channel every player watches
    is worse than a beta nobody announced.

- **Purchases works like the rest of the app now.** The tab you install your bought mods from
  was a bare grid: no way to see what something actually was before installing it, no way to
  find one among sixty, and an "Installed" badge that mostly didn't light up. It now has the
  catalog's search, category pills, sorting (recently purchased, name, or not-installed-first)
  and a full detail page — screenshots, description, author — reached by clicking a purchase's
  artwork, the same as the catalog.
- **Installing a purchase now works exactly like installing from Browse.** Add to Library asks
  where it should go — the same destination picker, with the folders you already use and the
  one you picked last time — confirms first if you already have it, then queues the install and
  reports it on the same card, with the same FrostMod reload. Starting a second install while
  one is running queues it instead of freezing the grid, which is what the purchases tab used
  to do.
- **The "Installed" badge is now a fact rather than a guess.** It used to compare the store's
  product name against your folder names, and those routinely disagree — *2022 ARL MX Round 1*
  ships as `2022.ARL.MX.RD01.pkz` and lands in a folder named after the file, so the badge
  missed. Each install now records what it put where. Anything installed before this keeps the
  old fuzzy match as a fallback.

### Changed
- **Share and Expand look like the buttons they are.** Sharing a preset was one of four
  identical grey glyphs on the card, and the 3D preview's expand control was a tooltip away
  from invisible. Both now keep a background at rest.
- The 3D viewer can be handed textures the backend has never seen, which is what lets the
  Designer's canvas appear on real geometry. Everything else still arrives by token, unchanged.
- Switching between the Studio's tabs no longer throws away what you were doing in the one you
  left.
- Sheets unpacked in Paints can be sent straight to the Designer to draw on, rather than
  unpacked a second time.
- **FrostMod still starts when a runtime is missing.** It's a warning, not a gate: we can't
  tell from outside which PCs manage to inject anyway, and refusing to launch would take
  FrostMod away from anyone the check is wrong about.
- **Opening a paint in the viewer is fast.** A gear paint is tens of megabytes of compressed
  pixels — three 4096² sheets — and every one of them was inflated one after another, then
  copied whole before being downscaled. They now inflate in parallel off a header walk that
  reads the image table without touching a pixel, and the copy is gone. The roughness sheet
  isn't decoded at all: the viewer samples a diffuse and a normal map, so that plane was 67 MB
  allocated, resized, sent to the webview and turned into a mipmapped texture nothing binds.
  Paints are also kept once decoded, so re-opening one or flicking back to it in the picker
  costs nothing.
- **Developer builds no longer run the paint decoder unoptimized.** `cargo`'s default leaves
  every crate at `opt-level = 0` in a debug build, and essentially all of this work happens
  inside `flate2` and `image`. Dependencies now build optimized and our own crate lightly so,
  which is what made a locally-run app take seconds where the shipped one took a fraction of
  one. Release builds are untouched.

### Fixed
- **A dialog keeps its contents inside its own window.** A dialog lays itself out as a grid,
  and a grid column grows to fit whatever inside it refuses to wrap — so a footer carrying a
  third button, or a mod name with no spaces in it, quietly pushed the text box out past the
  dialog's edge. Fixed for every dialog at once rather than the one it was noticed in: a
  crowded footer wraps, and long names break instead of shoving.
- **Dropping paints no longer leaves the review sheet spinning.** Working out what a dropped
  `.pnt` is comes down to one question — which model does it paint? — and the file answers it
  in its texture names: an outfit carries `rider`, a bike livery carries `framecompletemap`.
  Reading those names decompressed every sheet in the file first, which for one 38 MB outfit is
  a fifth of a second and 200 MB of pixels, all of it thrown away unlooked-at. A pack with one
  folder per paint paid that bill for every paint in it, one after another, and never got to
  the end. The names sit in the file's headers, each of which states how far the next one is,
  so they are now read by seeking past the pixels rather than through them: 0.6 ms instead of
  204 ms for that same outfit, and nothing decompressed at all. A sealed paint still has to be
  decrypted whole before it has headers to read, and comes out about five times faster.
- **A drop no longer rescans your mods folder once per row.** The list of bikes a paint could
  go on, and of folders a track could be filed in, was rebuilt for every row in the sheet —
  reopening every installed bike's `.pkz` each time — for lists that cannot change while you
  are looking at them. The folder is now read once per drop and every row shares it.
- **A corrupt `.pnt` is refused instead of being believed.** How many textures a paint holds
  and how big each one is are numbers read out of the file before anything has checked them,
  and both were handed straight to an allocation. A damaged or truncated paint could ask for
  more memory than the machine has, or expand without limit while decompressing. Both are now
  bounded well above anything a real paint carries, so a bad file is an error on that file
  rather than something that takes the app down with it.
- **Protection paints were being saved to a folder the game doesn't read.** Paint Studio wrote
  them to `rider/protection/<model>/paints` — the singular folder an old version of this app
  used, marked un-installable ever since — while the model itself lives in `rider/protections/`
  and every other path in the app installs there. The paint saved, the app said so, and the
  gear turned up unpainted in game. Destinations are now derived from the folders the game
  actually loads, so the two can't drift apart again.
- The Designer's "Start from a paint…" gave no sign it was working while it unpacked, and said
  nothing at all if the paint yielded no sheets.
- **Signing in to the Shop no longer signs you straight back out.** A sign-in would land, show
  the Purchases tab for a second, and drop to the signed-out screen again — every time, for
  anyone whose stored session had gone stale. Your purchases are read out of a hidden browser
  window that stays parked on the store, and nothing was closing that window when the session
  underneath it changed: it kept the page it already had, which — after a failed read, or a
  sign-out — was the store's own login form. So the very first thing a successful sign-in did
  was re-read that form and conclude the session had expired. Signing in now drops the window
  along with the old cookies, and re-loads the purchases page rather than trusting whatever was
  on screen. The same window is also dropped whenever the store answers that you are signed
  out, so one failed read can no longer make every attempt after it fail the same way.
- **The Purchases tab no longer claims you are signed in when you aren't.** The app remembered
  a sign-in in a file that could outlive the browser's cookies, so opening the tab rendered as
  signed in, failed its first read, and flipped to signed out — the same flicker, with no
  sign-in involved. The store telling us you are signed out now clears that record. Your
  Cloudflare pass is deliberately kept, so signing back in doesn't have to sit through a fresh
  challenge; **Log out** still clears everything, as it always did.
- **The MX Bikes Shop sign-in no longer loops on "Verify you are human", and your purchases
  install again.** The store now puts a Cloudflare *managed* challenge in front of every page
  the signed-in half touches — the login form, the purchases list, and the file downloads
  themselves. Only a real browser can clear one of those, by running its JavaScript, and the
  pass it earns can't be handed to an HTTP client, so the app was asking for pages it could
  never be given. Two things came of that: signing in sent you to the purchases page the
  moment your password was accepted, straight into a second challenge, which is the loop; and
  even a sign-in that got through had nothing behind it, because listing and downloading were
  refused the same way.

  The whole signed-in flow now happens inside a real browser window instead. Signing in lands
  on a page Cloudflare doesn't gate, your purchases are read from a hidden window that has
  actually cleared the check, and a bought file is downloaded by the browser itself straight
  into the staging folder — so nothing large is squeezed through JavaScript. Browsing the
  public catalog was never affected and is unchanged.

  Two side effects worth knowing: an install now shows megabytes downloaded rather than a
  percentage, because the browser reports a start and a finish and nothing in between; and
  **Log out** now clears the browser's cookies too, which it has to, so the next sign-in is a
  real one.
- **A sign-in that fails now says so.** After five minutes of getting nowhere the app gave up
  in complete silence — no message, no log line — which is what made a stuck challenge look
  like an app that had simply hung. It now reports the failure and closes the dead window, and
  writes which cookies the window ended up with (names only, never values) so a log says
  whether the challenge ever cleared.
- **Helmets, boots and protection now install into a folder of their own.** A gear mod
  packaged as a `.pkz` — which is how locked mods are distributed — unpacks to a single
  folder, and the app was unwrapping it and dropping the contents straight into
  `mods/rider/helmets`. The game only loads a model as `helmets/<Model>/…`, so the helmet
  became unloadable and invisible: it never appeared in the Rider tab's picker, and the
  `paints` and `goggles` folders it arrived with were listed there in its place.
- **The Rider tab offers to gather a gear mod that was installed loose.** Anything already
  scattered by the above stays broken until it's moved, so the tab now spots it, names the
  folder it will make (taken from the mod's own descriptor) and lists exactly what will
  move before you press Repair. Packaged `.pkz` models and models already filed correctly
  are left alone.
- **"A helmets mod was installed loose" no longer fires on a paint pack.** The repair asked
  to gather anything it found sitting in `mods/rider/helmets` — including a bare
  `paints/`/`goggles/` pair, which is not a scattered helmet at all but liveries for one that
  is already installed. Paints for a packaged helmet have nowhere else to live, since nothing
  can be written inside a `.pkz`. Repairing them built a helmet out of the liveries: a folder
  with no mesh, offered by every picker as a model, with the real helmet's paints filed under
  a name the loader never looks for. The repair now only offers itself when a model is
  genuinely there — a mesh in the gear areas, a descriptor for riding-style animations, which
  ship no mesh and no paints either.
- **Two mods scattered into one folder are left alone instead of fused.** Nothing on disk
  says which mesh, config or livery came from which mod, so gathering them under one name
  produced a model that never existed. Two descriptors in an area root is now a refusal, with
  the reason in the log.
- **A repair that fails part-way puts everything back.** It stopped at the first file that
  wouldn't move — a file the running game holds open, on Windows — leaving the mesh gathered
  and `paints/`/`goggles/` still in the area root, which shows the model *and* both strays in
  the picker and reads as the repair having split the helmet up. It is now all-or-nothing.
- **A packed gear item and a folder of the same name are now one item, not two.** A `.pkz`
  helmet with paints installed loose beside it (which is where the game looks, and where the
  studio writes) only ever showed one side of itself: whichever the loader resolved first.
  A folder holding nothing but paints hid the archive entirely — the picker listed the new
  paint alone and the preview lost the mesh it belonged to. Both are read now, the folder
  winning a name clash because it's what was installed last.
- **An empty paint slot shows the model's own look, not a paint you never chose.** The stock
  helmet came out bronze on the Rider tab while the Library showed that same helmet white
  under "Stock", and picking a helmet mod without naming a paint quietly dressed it in
  whichever paint that mod happened to list first. Leaving a paint slot empty is how a
  loadout says "the model's own look" — but the loader read it as a name it couldn't find,
  and that case deliberately falls back to the first paint in the folder so a renamed livery
  still shows up textured rather than bare grey. The game's own helmet folder happens to list
  `black_yellow` first, and that is the bronze. An empty slot now reaches the texture baked
  into the mesh, which is exactly what the Library means by "Stock" — both ask the model the
  same question now, so the two views can't disagree about it again. A paint that *is* named
  but missing still falls back as it always did, and a mod that ships paints without baking a
  shell texture into its mesh keeps that fallback too, rather than turning up bare.
- **A purchased bike or gear set no longer installs as if it were a track.** The old path
  picked a destination by looking for keywords in the *product name* and otherwise assumed
  tracks, so bikes, paints and gear were filed into a tracks-derived folder silently, with no
  preview and no collision check. Destination now comes from the archive's contents.
- **Retrying a failed install no longer breaks the install.** Downloads are meant to run one
  at a time, but the Retry button on a failure went straight to the installer instead of the
  queue — so a second, impatient click started a *parallel* run of the same mod. Both runs
  used a staging folder named after the mod, and the newcomer wiped it clean on the way in,
  deleting the files the first run was still copying. The failure landed as a bare
  "os error 2", most often on a livery, and clicking Retry again only made it likelier. Retry
  now joins the queue, a mod already installing to the same place ignores a repeat click, and
  every install stages into a folder of its own.
- **A failed install says which file it failed on.** The error reached you as a raw system
  code with no path in it — nothing to act on and nothing to report. It now names the file
  and where it was going, and the failure is written to the log rather than living only in a
  toast you have to catch before it goes.
- **One unreadable file no longer sinks a whole install.** A shortcut whose target has gone
  away, or a file an antivirus pulls out mid-install, used to fail everything that came with
  it. Those entries are skipped and noted in the log; the rest of the mod installs.
- **Installing over a running copy no longer stops at "error opening file for writing".**
  The file it named, `frost.exe`, is the app's own program file, and Windows won't let
  anything overwrite a program image that is still held — which the app's often is, since it
  hides in the tray, starts at login, and launches the installer from inside itself when you
  update in-app. Closing it first isn't enough on its own: the hold can outlive the process,
  by a copy still tearing down or an antivirus reading the image as it exits, which is why
  the installer's own "MXB App is running" prompt never appeared before the failure. The
  installer now clears the name by whatever means Windows allows — delete it, retry for a
  couple of seconds while a dying copy lets go, and failing that rename it aside, which
  Windows permits even for a file in use — then sweeps up what it moved once the new build
  is in place. The uninstaller does the same, since it's the installed build's uninstaller
  that the next version runs, and a failed one is what used to bounce you back to the
  "already installed" page for good.
- **An in-app update can no longer kill its own installer.** Closing the app took the whole
  process tree with it, and the installer the app had just launched was part of that tree.
  It now closes the app alone; the browser processes that the tree kill was there for exit
  with it anyway.
- **Opening MXB App while it's already running shows the window you had, instead of starting
  a second copy.** Closing the window parks the app in the tray rather than quitting it —
  that part is deliberate, it's what keeps FrostMod connected — but nothing stopped the next
  launch from building a whole new app beside it. Open it five times over a day and you ended
  up with five of everything: five windows, five tray icons to go and quit one by one under
  the overflow arrow, five FrostMods. A launch now hands off to the copy already running and
  brings its window forward, the same as clicking the tray icon. Launch-at-login is covered
  too — the instance that started with Windows is the one your first launch of the day
  reveals, rather than the one it stacks on top of.
- **A mods folder you moved with `mxbikes.ini` now works.** Plenty of players keep their
  content somewhere short like `C:\mods` — junctioning one rider paint into six model
  folders needs paths that OneDrive and a deep `Documents` tree can't give you. The app
  couldn't be pointed at such a folder: picking it silently rewrote your setting to the
  drive root, and auto-detection went to `Documents\PiBoSo\MX Bikes` instead, found no
  `mods` inside it, and came up missing gear, paints and bikes with nothing to explain why.
  The folder you pick is now the folder used, whether it's the game folder or the mods
  folder itself.
- **The app reads `mxbikes.ini` to find a relocated mods folder on its own.** The game
  already knows where your content is; setup now asks it instead of guessing. Profiles,
  which don't move with the mods folder, are pinned to where they actually stayed.
- **FrostMod was being handed the wrong folder.** It appends `\tracks` and `\bikes` to what
  the app gives it, and the app was sending the folder one level above — so its track
  manager and model swap were pointed at folders that don't exist. Affected everyone, not
  just relocated setups; it just never announced itself.
- **Settings says which folder mods are actually read from** when that isn't the obvious
  `<picked>\mods`, and warns when the folder isn't there at all.
- **Settings offers Stop for a FrostMod it didn't install.** The button was hidden unless
  the app had put FrostMod there itself — so the one case that most needs a stop, a
  `frostmod.exe` running that the app didn't start, had no button. The backend could
  always kill it; only the button was missing. (The sidebar's stop control was unaffected.)
- **Browse keeps your place when you look at a mod and come back.** Opening a mod used to
  throw away the search you typed, the category you picked, the sort order, every page you'd
  loaded with "Load more" and how far you'd scrolled — Back dropped you at the top of a fresh,
  unfiltered listing, which made working through a long category one mod at a time miserable.
  The grid now sits exactly where you left it, down to the row of cards, and nothing is
  re-fetched on the way back. It survives a trip to Library or Settings too, and the in-game
  overlay behaves the same way.
- **The Linux app opens to its interface instead of a white screen.** On SteamOS the window
  appeared, the title bar drew, and the inside stayed blank with nothing in the terminal to
  explain it. The AppImage carries its own copy of the web engine, and that engine tries to
  hand frames to the graphics driver through a fast path the host's driver answers
  differently than the one it was built against — so it drew nothing at all, silently. It now
  takes the ordinary path by default, which paints on every machine and costs nothing anyone
  will notice on a UI this static. A machine that handles the fast path can still ask for it
  back with `WEBKIT_DISABLE_DMABUF_RENDERER=0`. The startup log now records which path a run
  took, so the next report of a blank window can be read straight from the log.

## 2026-08-09 — v0.8.1 — GP Bikes' mod pictures

A patch on top of v0.8.0 — everything that release added is still the news, and its notes are
repeated below.

### Fixed
- **GP Bikes mods show their pictures in Browse.** Every card in the GP grid came up blank,
  and so did the pictures inside a mod's page. The app fetches thumbnails through its own
  cache rather than letting the page load them directly, and that cache only ever knew
  mxb-mods.com and the store — gpb-mods.com wasn't on the list, so every GP image was refused
  before it was ever requested. The list is now built from the games the app drives, so a
  title's catalog can't be added without its pictures working.
- **GP thumbnails are fetched as GP Bikes' own visitor.** They were going out through the
  store's session, which holds cookies for a different site. The two catalogs keep separate
  sessions on purpose — Cloudflare scopes a clearance to the host that issued it — and images
  now follow the host they came from rather than whichever game is selected.

## 2026-08-09 — v0.8.0 — GP Bikes, the MXB Shop, and a dropzone that takes anything

### Added

- **The app now drives GP Bikes as well as MX Bikes.** First launch asks which game you're
  setting up before anything else, and the game picker lives in Settings from then on;
  picking a title points the whole app — Library, Manage, Presets, Browse and the
  Play button — at that game's folders. The sidebar shows which game you're on. The two
  games keep their folders separately, so
  switching back and forth never asks you to find them again, and a game you open for the
  first time has its folders auto-detected (`Documents\PiBoSo\GP Bikes`, and the Steam
  install under AppID 848050) exactly as first-run setup does. Switching to a game the app
  can't locate lands you on the setup screen, with the switcher still there to get back.
- **GP Bikes' mods tree is read as its own shape.** GP keeps `bikes`, `tracks`, `tyres`,
  `misc` and a `rider` folder laid out differently from MX Bikes': helmets and riding-style
  `animations`, with boots, gloves and protection baked into the rider model rather than
  picked separately. The Library lists riding styles as their own category and no longer
  offers goggles, boots or protection for a game that has no concept of them.
- **Browse serves GP Bikes from gpb-mods.com.** The same catalog client, pointed at the
  matching site — Race and Kart tracks, New Bikes, Liveries, Sounds, Rider Models, Suit
  Paints, Helmets and Helmet Models, plus Plugins, Tools and Menu Backgrounds. Each site
  keeps its own cookie jar, since a Cloudflare clearance only works on the host that issued
  it.
- **FrostMod's live mod reload now works for GP Bikes.** The status panel, the reload button
  and the watch-the-folder auto-reload are all available when GP Bikes is the active game —
  they were hidden before because FrostMod only knew about MX Bikes. The app launches it
  with the game it's driving and the folder that game's mods live in, which is what makes it
  attach to the right process and read the right files. This needs **FrostMod v0.11.0 or
  newer**: v0.10.0 attaches to GP Bikes but reloads using MX Bikes' internals, which crashes
  it, so the app won't run that build there and updates you to one that works.
- **Presets read the slots a profile actually has.** Rather than assuming MX Bikes' fifteen,
  the editor reads the sections out of the profile's own `profile.ini` and shows those. GP
  Bikes profiles get their riding-style slot, and don't get pickers that would write nothing.
- **A Shop tab that browses the mxbikes-shop.com catalog.** The store hands us its whole
  catalog as one JSON document, so the app fetches it once and does the searching, filtering,
  sorting and paging locally — browsing is instant and keeps working with the network down.
  Search by name, creator or category, filter down the store's own category tree (picking a
  parent includes everything under it), sort by recently updated, price or discount, and flip
  on **On sale** to see only what's actually discounted right now. Discounted items show the
  normal price struck through beside the live one with a percentage badge, including the
  awkward case of an item that has both a price *range* (a paint, or a paint plus the PSD)
  and a sale on it. Genuinely free downloads say **Free** rather than "$0.00", which is a
  different thing from the pay-what-you-want items that merely start at zero. The store's
  catalog carries no sale end dates, so the half-hourly conditional refresh is what keeps a
  finished sale from being shown forever — it corrects itself in the background without anyone
  pressing anything, and where an end date ever does appear it's honoured against the clock.
  A catalog served from cache says how old it is, and one that's days old says so in a way you
  can't wave away. This is browse-only: **Buy** opens the product page in your own browser, and
  nothing here installs or purchases anything.
- **Shop items now show the store's own description.** The catalog started carrying them
  today — every one of its 1311 products, screenshots and all — and the detail page shows it
  under **About**, the same way a Browse mod's description reads.
- **The whole window is a dropzone.** Drag anything MX Bikes onto the app — a `.zip`, `.rar`
  or `.7z`, a bare `.pkz` or `.pnt`, an already-extracted folder, or a fistful of all of them
  at once — and it works out what each one is, where it belongs, and shows you the list
  before anything is written. A mixed archive becomes one row per mod rather than one verdict
  for the lot, and a dropped folder is read where it lies instead of being copied twice.
- **Content is identified by what's inside it, not by its name.** A bike by its `.ini` + `.cfg`
  pair, a track by its `.map`/`.trh` files, a sound set by `engine.scl` + `sfx.cfg`, a livery
  by the textures it carries — a paint that covers `rider` is an outfit, one that covers
  `framecompletemap` is a bike livery. Each row says which signal identified it.
- **Rider outfits and gear get a real destination.** An outfit goes to
  `riders/default_mx/paints` and gloves to `riders/default_mx/gloves` — profiles MX Bikes
  always ships, so the paint loads. Helmet, boot and protection paints land on the only model
  of their kind when there is exactly one; with none or several, nothing in the file picks
  between them, so the row asks.
- **Every row can be re-filed before installing.** Identifying content correctly is not the
  same as knowing where you want it kept, so each row offers that category's existing folders,
  its root, and a free-text box for a folder that doesn't exist yet. A bike keeps its own
  folder wherever it's filed — choosing "MX2" means `MX2/<Bike>`, not a bike's configs
  scattered loose.
- **You see what it will replace.** Every row reports how many files it will write and,
  expandable, exactly which existing files it would overwrite — so re-dropping an updated mod
  no longer silently replaces a bike's configs. Nothing installs until you press Install, and
  rows can be unticked individually.
- **Opening a paint in 3D now shows the model it was painted for, wearing it.** Click a
  livery, a helmet paint or a goggle paint and the viewer loads the bike or helmet it belongs
  to and selects that paint in the picker, instead of draping the textures over a stock body
  that was never the shape they were drawn against. A paint whose model isn't installed still
  previews the way it did before.
- **Thumbnails are cached on disk, and downscaled to the size they're drawn at.** Both
  catalogs previously put remote image URLs straight into the page, so every scroll through
  the grid re-downloaded the same images — and neither store offers a thumbnail size, so a
  card roughly 300px wide was being handed a 1000–1280px original weighing about half a
  megabyte. Images now go through an on-disk cache served over the app's own URL scheme,
  which keeps lazy loading and the webview's native image cache intact, and grid thumbnails
  are resized on the way in: a page of cards dropped from ~12 MB to ~2 MB, about 85 KB per
  card instead of 490 KB. Transparency is preserved, and each size is cached separately so
  opening an item still shows the full-resolution screenshot. Capped at 256 MB, evicted
  oldest-first, and restricted to the two catalog domains.
- **Proton Drive downloads.** `drive.proton.me` links are now recognised and labelled as
  Proton Drive, and a mod offering one alongside another mirror prefers the other. Proton
  shares are end-to-end encrypted — the key lives in the part of the URL that never
  reaches the server — so they can't be fetched automatically; picking one now opens the
  guided "download it, then choose the file" flow instead of downloading the web page and
  failing later with "couldn't determine the archive type".
- **Beta builds now say they're beta**, next to the version in Settings → About. The badge
  keys off a semver pre-release suffix, which is the same thing the release workflow uses to
  mark a build as a pre-release, so the two can't disagree.
- **Paint sync.** MX Bikes never transmits custom content: a remote rider renders using
  whatever file on *your* disk happens to match the name they picked, so a full grid shows
  up in default liveries. The game can't tell us what they picked either — its plugin API
  carries rider names, bikes and lap data, and no paint field at all. So the loop is closed
  outside the game. Your app publishes what your rider is wearing; every other app pulls it
  back and installs it. Paints are content-addressed by SHA-256, so twenty riders sharing a
  livery is one stored object and nineteen uploads that never happen, and a second sync
  installs nothing it already has.

  Everyone on the server needs the app for this to work — that's inherent, not a limitation
  we chose.
- **Riders are identified by their MX Bikes GUID**, not just their rider name. A name is
  free text you can change between sessions and two people can pick the same one; a GUID is
  stable per install, and the dedicated server writes it next to the name on every
  connection. The agent reads the server's own log to know who is actually connected —
  which turns out to be a far easier route to a live roster than decoding the live-timing
  UDP feed, since the game's plugin API exposes no GUID for anyone but yourself. Claiming a
  GUID is first-come, so nobody can assert someone else's identity and have their paints
  served under it. Rider-name matching stays as the fallback until a GUID is supplied.
- **A Servers tab that manages the dedicated servers you run.** Start, stop and restart the
  game on a host, see whether it's up, how long it's been up, how many times it came back on
  its own, and switch the track — all without an RDP session or a shell. Each server is added
  with its agent address and token, and its status refreshes while the tab is open.
- **`mxb-agent`, a supervisor that runs on the game host** (`server-agent/`). It **owns** the
  `mxbikes.exe` process rather than managing whatever happens to be running, and that
  ownership is what makes the rest work: exit detection comes from the child handle instead
  of polling the process table, so a server that crashes is **brought back automatically**;
  "restart" isn't a race between a kill and someone else's respawn; and a reboot that starts
  the agent starts the game with it. A deliberate stop is suppressed, so the watcher doesn't
  fight you by reviving a server you meant to shut down.

  The app talks to this agent, never to a cloud provider — a desktop app that shipped
  provider credentials could create infrastructure on any machine it ran on. What it holds
  instead is a bearer token for one server you already administer.

  Settings changes patch the server's `.ini` in place, exposing only `track`, `name` and
  `maxclient`, and the game is restarted afterwards because it reads its `.ini` only at
  startup.

  Note the agent speaks plain HTTP, so its token crosses the network in clear. Terminate TLS
  in front of it, or keep it on a private network, before exposing it to the open internet.
- **Join a server straight from the app.** A new *Join a server* button in the sidebar takes
  a server address and starts MX Bikes already connected to it, rather than launching the
  game and leaving you to find the server in the in-game WORLD list. The game has carried an
  undocumented `-directconnect` flag all along — PiBoSo documents only `-dedicated` and
  `-clientport` — which reads the address from the argument that follows it. A bare host gets
  the dedicated server's default port, 54210.

  The address is validated in the Rust command rather than in the UI, because it ends up on
  the game's command line: anything that could smuggle a second argument past us — embedded
  whitespace, a leading `-` — is rejected there, so a pasted address can't quietly turn into
  a different flag. The connect flag is only read at startup, so asking to join while MX
  Bikes is already running says so instead of appearing to work.
- **An Experimental switch in Settings**, off by default, gating both of the above. They
  talk to a live service and write files other players uploaded, so they're opt-in rather
  than something you find by accident. `MXB_EXPERIMENTAL=1` turns them on for a single run
  without touching your saved settings — and the switch says so rather than looking stuck.

### Changed

- **The Library's 3D button says what it does.** The bare square on each row is now a
  labelled **View in 3D**.
- **Settings spells out which folder to pick.** The app wants your MX Bikes folder — the one
  holding `mods` and `profiles` — not the `mods` folder inside it, which is one click deeper
  in the picker and easy to land on. The setting now says so, and picking `mods` by mistake
  quietly resolves to the folder above it (Settings says which one it took) rather than
  leaving you with a library that scans nothing and a path that looks perfectly reasonable.
- **Features that don't apply to GP Bikes are hidden rather than half-working.** FrostMod
  is a compiled MX Bikes plugin, so its sidebar panel and settings section are hidden for
  GP Bikes and instant profile refresh is shown disabled with the reason. The 3D preview
  (Locker and Rider) and Manage are MX Bikes only for now; the overlay and the guided tour
  follow the same gating, so neither offers a view the main window doesn't.
- **Existing MX Bikes setups are untouched.** A `config.json` written before this release
  opens as the MX Bikes config it always was, with the same folders; preset share codes are
  unchanged. Applying a preset no longer creates `profile.ini` sections that the game
  doesn't use.
- The signed-in "All My Downloads" page moved to `MyDownloads.tsx` and the `shop` route now
  goes to the new catalog. That feature is intact and still hidden from the sidebar.
- **Placement decides once, then acts.** Split `place_mod` into `plan_placement` (decide) and
  an apply step driven by a single enumeration of the files to write, so the destination shown
  in the review sheet is by construction the destination written to disk. Existing placement
  behaviour and all of its tests are unchanged.
- **Identifying a `.pkz` no longer reads it.** The question is answered from the archive's
  file list alone; it used to parse the `[info]` block and decode and rescale the preview
  image, which on a large track was seconds of work before anything appeared on screen.
- **Staging directories are per-operation.** They used to be one path per process, wiped on
  entry — safe only while installs were strictly serial. A staged drop awaiting review would
  have had its files deleted by the next one.
- **The Shop opens faster.** Descriptions are 3.1 MB of the catalog's 4 MB, and every one of
  them was being cleaned up for display the moment the catalog loaded — before the grid could
  paint, for 1311 products of which you open maybe two. That work now happens when you open an
  item.
- **The game picker moved out of the folder setting.** MX Bikes / GP Bikes now sits in its own
  **Game** card at the top of Settings, above the folders it scopes, instead of inside the
  card whose own title it changes. Builds that know one title don't show it at all.
- **Switching to a game you haven't set up says what to do.** The folder row read a bare "Not
  set" next to a **Change…** button; it now reads *Select a folder for GP Bikes* next to
  **Set…**.
- **FrostMod builds that aren't safe on the current game are no longer offered for it.**
  FrostMod v0.10.0 is the first that attaches to GP Bikes, but its mod reload runs MX
  Bikes' internals there and takes the game down the first time it's used. On GP Bikes the
  app now requires v0.11.0 or newer, updates to it automatically, and says so instead of
  starting a build that would crash. MX Bikes is unaffected — every FrostMod ever released
  was built for it.
- **"Join a server" is behind the Experimental toggle.** It launches the game with an
  undocumented connect flag, so it belongs with the rest of the unfinished multiplayer surface
  rather than being something you find by accident.

### Fixed

- **GP Bikes suits go where GP Bikes reads them.** Every rider install landed in `mods/rider`
  itself — the game never opens that folder, so a suit you'd just installed was nowhere in
  the game. Suit paints now default to `riders/<your rider model>/paints`, rider models to
  `riders`, helmet paints to `helmets/<model>/paints` and riding styles to `animations`,
  which is exactly the set the game's loader reads and nothing else.
- **The rider picker offers your game's folders, not the other one's.** GP Bikes was being
  shown MX Bikes' `boots`, `protections`, gloves, goggles and `default_mx` rider — none of
  which exist there — while `riders` and `animations` were missing entirely. Each title now
  lists its own, and the folder you last picked is remembered per game.
- **A suit paint knows which rider model it's for.** gpb-mods files suits under the model
  family they fit — Modern 1, Modern 2, MGP 21 — and that now picks the installed model,
  instead of falling back to whichever folder you used last.
- **Quick-install stops dumping rider mods in the root.** One-click and bulk installs from
  the Browse grid never asked the rider logic at all, so a helmet, kit or suit installed that
  way ignored every gear folder. Both games.
- **A dropped GP suit isn't filed as boots.** GP's suits carry the boots' textures because
  the boots are part of the rider model, and the drop classifier read that as a boot paint —
  aiming it at a folder GP Bikes has no loader for. Suit, outfit and arm textures now read as
  the rider's body, and gear hints for folders your game doesn't have are ignored.
- **A dropped protection paint goes to `protections`.** The drop route was still aiming at
  the old singular spelling the game stopped reading.
- **Protection is drawn the right way up.** Every piece in the slot rendered on its side: the
  viewer assumed protection was authored the way the rider's body is, and it isn't — it takes
  the same up-axis as a helmet, turned a quarter turn about it. Read off the meshes rather
  than assumed, and checked against the game's own chest protector and neck brace, a tactical
  vest, a Leatt, long hair and two chains. The library's quick-view had the same fault and now
  faces a piece forward too.
- **Installed protections show up at all.** The game keeps them in `mods/rider/protections`;
  the app looked in — and installed to — `protection`, so nothing you dropped in the game's
  folder was offered in the picker, and nothing the app installed was visible to the game.
  New installs go to the game's folder; the old one is still read, so anything already there
  keeps working.
- **A protection set wears all of its pieces.** The game's own "Full" is a chest protector
  *and* the neck brace worn with it, and only one of the two was drawn — a different one from
  one run to the next. Every piece a mod declares is now drawn, each bound to its own mesh's
  textures.
- **Stock protection isn't a grey shape any more.** "Full" and "Neck" ship no paint of their
  own, and the loader had nothing to dress them in; they now wear the texture baked into their
  mesh, the same way a paintless mod already did.
- **Chains hang where they belong.** A mesh whose geometry is a single group was placed twice,
  which put a chain 35 cm below the rider and off to one side — most of the protection
  category is chains. Gear now lands exactly on the bounds its own file states.
- **A protection that ships sealed files loads out of a `.pkz` too**, not only out of a folder.
- **The expanded 3D preview gives the model the window.** The title bar was taking half of it,
  because the dialog laid its two rows out as an even grid.
- **Paint folders shared between models with a junction or symlink are read again.** If you
  keep one set of liveries and point several rider models at it — `mklink /J` on Windows, a
  symlink on Linux and macOS — the app walked straight past the shared folder and showed
  nothing in it, while the game loaded it fine. The only way to use the app was a copy of
  every rider and glove paint per model. It now follows the link, so one folder can serve
  all six of your rider models, and the paint is listed under each of them.

  The cause is the same everywhere it bit: a junction is a *link*, and a directory listing
  reports it as one rather than as a folder, so a scan that trusted the listing never
  looked inside. That affected every content scan, not just paints — the Library, Manage,
  the mod-detail panel, preset bundles and paint sync all shared it.
- **A content folder that lives on another drive and is linked into place is scanned.** The
  split layout — `mods\tracks` (or `bikes`, or `rider`) junctioned to somewhere with room
  for it — used to come up empty.
- **Auto-reload notices changes made behind a link.** A recursive watch stops at a
  junction, so dropping a paint into the shared folder never pulsed FrostMod. The folders
  those links point at are now watched too, and a change in one names every mod pointing
  at it.
- **Bundling a preset whose gear folder contains a link no longer fails.** The linked
  folder's contents are copied into the bundle as real files, since the far end of your
  junction doesn't exist on the machine that opens it.

Extracted archives are deliberately left alone: a link inside a download you didn't make is
still refused, which is what stops an archive writing outside the folder it unpacks into.

## 2026-08-08 — paints preview on their own model, and helmets bind their goggles right

### Added
- **Opening a paint in 3D now shows the model it was painted for, wearing it.** Click a
  livery, a helmet paint or a goggle paint and the viewer loads the bike or helmet it belongs
  to and selects that paint in the picker, instead of draping the textures over a stock body
  that was never the shape they were drawn against. A paint whose model isn't installed still
  previews the way it did before.

### Changed
- **The Library's 3D button says what it does.** The bare square on each row is now a
  labelled **View in 3D**.
- **Settings spells out which folder to pick.** The app wants your MX Bikes folder — the one
  holding `mods` and `profiles` — not the `mods` folder inside it, which is one click deeper
  in the picker and easy to land on. The setting now says so, and picking `mods` by mistake
  quietly resolves to the folder above it (Settings says which one it took) rather than
  leaving you with a library that scans nothing and a path that looks perfectly reasonable.

### Fixed
- **Helmets whose goggles came out wearing the helmet's paint.** The Bell Moto 10 packs are
  the clear case: shell, tear-off, lens and goggle frame each ended up in the wrong texture,
  the goggle worst of all. Three faults behind it, all in how a model's textures are counted
  and matched:
  - A helmet needn't ship the sheet its paints replace, and the Moto 10 doesn't — it names
    it and leaves the pixels to the `.pnt`. That slot was going uncounted, so every piece
    was drawn from its neighbour's texture.
  - A one-character texture name (the Oakley pack calls its goggle sheet `O`) was skipped
    entirely, sliding everything after it by one.
  - Which pieces are the goggles was decided from their names, and a helmet names its
    goggle group after the goggle it ships — `Armega`, `Airbrake` — not "goggles". It is now
    decided by which paint supplies the texture the piece is drawn from, which is the mod's
    own answer.
- **Pieces no paint covers keep their own look.** A tear-off film, a visor, anything an author
  baked into the mesh and left out of the `.pnt` used to have the shell's paint stretched
  across it.
- **A "Stock" option that showed a helmet in the wrong texture.** It's offered only where the
  mesh really carries the sheet that side's paints replace.
- **A config written by an older build lost track of which game was active.** Builds that
  predate multi-game support read the config fine but rewrite it without `activeGame` and
  `games` — so running one once (a downgrade, or the shipped app alongside a newer build)
  erased the choice while leaving the folder pointing at that game. Defaulting to MX Bikes
  there would drive a GP Bikes folder as an MX Bikes one; the game is now re-derived from
  the folders instead — the install folder's executable if there is one, otherwise the
  user folder's name.
- **Switching to a game you don't have installed opened an empty dashboard instead of
  the setup screen.** The app adopted `Documents\PiBoSo\<game>` whether or not that
  folder existed, and a non-blank folder reads as "configured" — so you got a working UI
  scanning nothing. A folder is now only adopted if it's really there, a saved folder
  that has since been deleted or moved is re-detected rather than trusted, and setup says
  so plainly when it can't find one instead of silently returning you to the same screen.
- **The UI named MX Bikes while driving GP Bikes** — "Launch MX Bikes", "MX Bikes is
  running", the install-folder settings and the overlay's status line among them. Strings
  now say which game is actually active.
- **Switching games left the previous game's content on screen.** Library, Manage and the
  rest load their data when they first appear, so switching titles swapped the folders
  underneath them without refreshing anything — most visibly Manage, which kept listing
  the MX Bikes mods. Switching now restarts those views from scratch.
- **Pages named the wrong site.** "View on mxb-mods.com", the missing-download note and
  the tour all said mxb-mods.com regardless of which catalog was actually being browsed.
  They now name the site they link to.
- **"FrostMod will hot-reload the track list" was shown for GP Bikes,** which has no
  FrostMod build and so was never going to reload anything. That title now gets the
  instruction that's actually true — restart the game to pick the new content up. The
  guided tour likewise no longer walks through steps for features the active game hides.
- **The mxb-mods.com fetch window is properly hidden now.** When Cloudflare refuses the app's
  own downloader, Browse re-runs the request inside a WebView parked on the site — and that
  window was built one pixel wide and thrown 32,000 pixels off the desktop rather than hidden,
  because a hidden window was thought to risk having its timers throttled. Off the desktop
  still leaves a real window behind, though: the system lists it, and it can be surfaced with
  no titlebar to close it by. It's built hidden outright now. The throttling that was being
  avoided isn't reachable that way — the webview inside keeps its own visibility, and that is
  what the browser engine reads — so Browse behaves exactly as before, minus the window.
- **A hostile archive can no longer write outside the staging folder.** `.7z` extraction
  joined entry names to the destination without filtering `..`; `.7z` and `.rar` extractions
  are now swept for escapees (symlinks included), which deletes them and fails the install.
- **A destination can no longer climb out of the MX Bikes folder.** Path segments are checked
  for `..` before any write, and the resolved targets are verified to sit under `mods/`.
- **A dropped bike folder keeps its own folder.** A bike shipped without a `paints/` subfolder
  had its `.ini`/`.cfg` scattered loose into `mods/bikes` instead of `mods/bikes/<Bike>/`.
- **The screenshots inside those descriptions were broken frames.** The store's CDN turns away
  any request that doesn't come from the store itself, and the app's window asks under its own
  name, so every embedded shot was refused where it stood. They now come through the same
  on-disk image cache the grid and gallery already use, which fetches them from the app's own
  side rather than the page's: they load, they're resized to the width they're actually drawn
  at, and they're kept rather than re-fetched every time you open the item. The store's second
  image host is included, which a few dozen products embed from.
- **A link inside a description replaced the entire app with a website.** These descriptions
  carry real links — the store's alone hold about 1250, to Discord, YouTube and the store —
  and the app window has no address bar or Back button to escape with. They now open in your
  own browser and leave the app where it was. Browse descriptions had the same fault and are
  fixed with it.
- **The overlay's empty frame left sitting over the game.** Clicking back into MX Bikes with
  the overlay open stopped drawing the panel but kept its window there — a box over the game
  that still swallowed clicks. The overlay now closes itself when you click away, to the game
  or to anything else, and the hotkey brings it straight back. Its own file picker doesn't
  count, so importing a mod from the overlay no longer risks closing it.
- **FrostMod now reads the mods folder of the game you're actually playing.** It was told
  which game to attach to but not where that game's mods live, and its own default was MX
  Bikes' folder whatever it attached to — so on GP Bikes its track manager, inactive-tracks
  store and model swap all worked against MX Bikes' folders. The app knows the real folder
  (including a moved one), so it now sends it.
- **Switching game restarts FrostMod.** FrostMod reads which game to watch once, at launch,
  and the app skips starting one when a FrostMod is already running — so switching to GP
  Bikes left it waiting for MX Bikes forever while the status pill still read "running".
  This also clears a FrostMod the app didn't launch, which is the case that made it look
  like nothing was wrong.
- **Helmet and boots paint lists showed one source and hid the rest.** A gear model can carry
  its paints in three places — packed inside its `.pkz`, loose in a folder of the same name
  beside it, or shipped with the game — and the picker stopped at whichever it found first.
  So a mod installed as a `.pkz` offered only the paint pack you'd added next to it and none
  of its own, a mod installed as a folder offered only that folder, and the stock helmet and
  boots offered nothing the game ships at all. Every source is now merged into one list, with
  a paint that appears in more than one offered once.
- **The Presets tab never asked a model what paints it carries.** It listed only what the
  library scan found loose on disk, which is why the same helmet offered a different set of
  paints in Presets than it did in Rider. Both tabs now read the same list, so they can't
  disagree, and the "slots reference a mod that is not installed" count is taken from that
  same list rather than contradicting the dropdown above it.
- **A paint the picker offered could render as a different one.** With a model installed both
  as a `.pkz` and as a folder, the viewer opened one of the two and quietly fell back to some
  other paint when the name you picked lived in the other. It now looks in both.
- **Boots on the wrong legs.** Which foot went on which side was decided by the two boot
  meshes' lateral centres, which differ by about a centimetre and a half — each foot's own
  asymmetry about the mirror plane it was copied across, not a left-right layout. That made it
  a coin toss settled by how each author happened to build the mesh, so some boots came out
  mirrored, buckles facing inward. The side is now read from the mesh's own node names
  (`boot_l` / `boot_r`), with the old measurement kept for meshes that don't say. The preview
  and the on-body render share the rule, so a boot can't look right in one and wrong in the
  other.

### Notes

- Instant profile refresh stays MX Bikes only. Unlike FrostMod's reload, it calls a
  hardcoded `mxbikes.exe` function offset, and there's no GP equivalent yet.

### Security

- A paint carries the destination it should be written to, and that path arrives from
  another player. It's validated twice — once by the service and again in the app before it
  becomes a real path — because only the second check actually protects a disk. Anything
  with a separator, a `..`, a drive letter, a control character or a non-`.pnt` extension is
  refused outright rather than sanitised: a path we'd have to rewrite is one we don't
  understand. Downloaded bytes are checked against their digest before being written.

## 2026-08-08 — v0.7.1 — Browse works for blocked players, the model-swap crash is gone, and the Rider preview wears a real rider

### Added
- **Helmets, Boots and Protection tabs in a preset's race content.** Next to Tracks and
  Always keep, three panes list the gear models you have installed so you can tick the spares
  worth keeping reachable — picking them by hand instead of leaving it all to what the preset
  happens to name. Anything unticked steps aside for the race; the preset's own gear is kept
  for you either way.
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
- **The install picker works out which bike a paint is for.** mxb-mods files every livery
  under a category per bike it fits ("2023 KTM 450 SX-F OEM") — far more precise than the post
  title it used to guess from. Those categories are now read off the post and matched against
  your bikes, so the right one is preselected under "Probably" instead of whichever bike you
  painted last. Checked against the whole catalog: the correct bike comes first for all 107
  OEM models.
- **OEM bikes can be picked as a destination at all.** Their files live inside the game's
  locked archive, so nothing of them is on disk until they're painted and the picker never
  listed them — a paint for one had to have its bike id typed by hand. Bike folders and the
  bike ids in your profile are now offered alongside packaged `.pkz` bikes.

### Fixed
- **Browse loads for players mxb-mods.com was refusing.** Some players got nothing but
  "mxb-mods.com refused the request (403)", and until now the app's answer was to open a
  check window, let Cloudflare's challenge clear, and reuse the cookie it earned. A tester's
  log proved that can't work: the challenge cleared in about a second, the cookie was sent
  correctly, and the site refused us anyway — Cloudflare ties that cookie to the exact
  browser connection that earned it, and the app's own downloader isn't that. So the app now
  moves the *request* into the browser instead of moving the cookie out of it. When
  mxb-mods.com refuses the fast path, the same request is re-run inside a hidden window on
  the site's own page and everything carries on from there, for the rest of the session. The
  visible "Checking with mxb-mods.com…" popup is gone entirely — nothing appears on screen.
  Mod downloads are untouched: they come from MediaFire, Google Drive and MEGA, never from
  mxb-mods.com.
- **Retry works more than once.** Closing the old check window parked it in the tray instead
  of destroying it, which left its name taken, so every attempt after the first failed to
  open one and Retry quietly did nothing for the rest of the session. The shop login window
  had the same fault. Only the main window parks in the tray now.
- **The game no longer crashes to desktop when you pick a bike after swapping a model or
  applying a preset.** Since 0.7.0, swapping a bike's model asked FrostMod to re-apply the
  bike so the new mesh showed without the class-switch away-and-back. FrostMod v0.9.9 does
  that by replaying a bike-load call it captured earlier — using a descriptor the game had
  already finished with, never checking the object that call writes into, and swallowing
  the resulting fault so the game carried on with a half-swapped machine. Nothing looked
  wrong at the time; the crash landed on the *next* bike selected by hand, which is why it
  read as "choosing a bike crashes the game" rather than as anything to do with the swap.
  The app no longer sends that request to any FrostMod below v0.9.11 — the release that
  stops replaying — and a FrostMod whose version it can't read counts as one to leave
  alone, where before an unreadable version was given the benefit of the doubt. Model swaps
  and presets still apply in full; the swapped model now appears when you re-select the
  bike in the garage, and the toast says so instead of promising a live refresh.
- **A preset that swaps a model no longer reports a live refresh it didn't get.** The
  apply toast was derived purely from the paint/gear reload, which never touches the mesh —
  so a preset carrying a model swap said "refreshed live in-game" while the bike on screen
  kept its old bodywork. It now says the paints are live and names re-selecting the bike as
  what shows the model.
- **Race mode now clears the rider gear too.** Applying a race preset narrowed the bikes and
  tracks the game could see and left the rider alone — every helmet, every boot, every
  protection set and every gear livery you have installed still showed up in the game's
  pickers, including the four hundred liveries sitting under the one helmet the preset
  actually names. Manage only ever moved archives and extracted tracks, on the reasoning that
  a loose `.pnt` costs nothing to mount; true, but mounting was never the point — a preset
  that names one paint means the others should be out of sight. Race mode now takes rider gear
  models and liveries out of the way alongside the rest, keeps exactly what the preset names,
  and brings them all back on Restore all.
- **Protection mods now show what they actually look like.** The protection slot is the
  busiest and least conventional one on mxb-mods — chains, necklaces, hoodies, bibs,
  backpacks, hair, the odd pickaxe — and unlike helmets and boots those mods bake their
  look straight into the mesh instead of shipping a `.pnt`. The preview only ever looked
  for a paint, so five of eight real mods pulled off the site came out as a featureless
  grey shape. A piece with no paint to wear now wears the texture its own mesh carries,
  each part bound to its own: a chest protector's shell and straps, or a chain's four baked
  maps, land where the model says they go instead of all sharing one.
- **A protection is drawn at the size it was made.** Every piece was being scaled to a
  fixed fraction of the rider and re-centred on his chest, which inflated a thin necklace
  to the proportions of a full vest and threw away the offset a chain or a hood is
  deliberately authored to hang at. Protection is modelled in the rider's own frame, so
  it's now placed as authored — its own size, its own offset — and the mount an `.hrc`
  names is honoured.
- **Mods that ship sealed loose files load.** A protection folder whose files are sealed
  the way a `.pkz` seals its entries used to fail outright with "no gear mesh found"; those
  now read like any other. Gear also follows the `gfx.cfg` → `.hrc` chain to find its mesh,
  so a mod that names it for the piece (`neckbrace.edf`, `pickaxe.edf`) resolves instead of
  relying on a guess from filenames, and stock gear whose folder doesn't use the slot's own
  mesh name no longer comes up empty.
- **Textures baked in Substance or Blender bind to the right mesh.** Maps exported under
  the toolchain's names — `Vest_Normal` beside `Vest_BaseColor` — were counted as looks in
  their own right, and since material indices count that list, every texture after one slid
  onto the wrong part. That's why the Tactical Vest wore its pouch's normal map.
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
- **The Rider preview no longer goes quiet when it fails to update.** If resolving the
  rider hit an error — a missing profile, a gear file the loader couldn't read — the Rider
  tab caught it and did nothing with it. The previous model stayed on screen, deliberately,
  so the preview never blanks; but with no error anywhere that is indistinguishable from a
  pick that genuinely changed nothing, and it made a real fault read as "changing this slot
  does nothing". A failed resolve now raises a toast with the reason, leaves a badge on the
  preview for as long as what you're looking at is out of date, and writes the error to the
  console. The toast fires once per distinct message rather than once per pick, since a
  persistent fault is re-hit on every slot edit.
- **Browse's Cloudflare check works more than once per session.** The clearance window
  closed into the tray instead of closing, which left its name registered for the life of
  the process — so the handshake succeeded the first time and every attempt after it failed
  with "a webview with that label already exists", leaving Retry doing nothing at all for
  the rest of the session. Only the main window parks in the tray now; the clearance check
  and the shop login, which had the same latent fault, close for real, and a rebuild waits
  for the old window to finish tearing down rather than racing it.
- **A paint with one unreadable texture no longer leaves the whole model untextured.** The
  viewer waited for a fixed number of textures to arrive and one that failed to load never
  arrived, so the count never completed and every part stayed grey. A texture that can't be
  read is now skipped on its own, and the rest of the paint still shows.
- **A paint dropped on a bike's root folder no longer vanishes.** MX Bikes only loads liveries
  from `<Bike>/paints/`, but the picker also offers the bike's root (where sounds and model
  swaps go) and would happily install a `.pnt` there — the install reported success and the
  paint never appeared in game. It's now redirected into that bike's `paints/`.
- **The install progress steps read as English again.** The Resolve → Download → Extract →
  Place → Reload chain printed its raw translation keys (`modDetail.stageResolve`) in every
  language — the labels were the only `TKey` in the app rendered without going through `t()`.
  All five locales already had the strings.
- **Tracks default to a folder you already use instead of the root.** Once the tracks library
  has any folder, the first one is preselected rather than dumping another `.pkz` loose at the
  root. Applies to Browse installs and to MX Bikes Shop purchases, which always went to the
  root.

### Changed
- **The 3D viewer stops eating the machine.** Every texture a preview showed was compressed
  to a PNG and then base64'd into a text blob before it could cross to the viewer — seconds
  of every core per bike, and a multi-megabyte string that then lived in the Rust cache, the
  message to the frontend and the browser heap all at once. Worse, each paint carried its
  own copy of the model's base textures, so a bike with several liveries held the same
  pixels over and over. The pixels now stay put and the viewer is handed a reference,
  fetching the raw bytes over a binary channel only for the paint it is actually drawing.
  Nothing is encoded, nothing is turned into text, and a paint costs a handful of bytes
  instead of a copy of the bike.
- **The viewer no longer redraws a parked model 60 times a second.** Nothing in the scene
  moves on its own, but the canvas was rendering continuously anyway, shadows and all — and
  the Rider tab keeps one on screen for as long as that page is open. It now draws only when
  something actually changes: you move the camera, a texture arrives, the model is reframed.
  Rendering also stops oversampling to twice the screen's pixels for a preview-sized model.
- **Switching paint no longer rebuilds the bike.** Changing livery threw away every part's
  geometry and re-uploaded the whole model to the graphics card just to swap which image it
  wore. Geometry and paint are now built separately, so a paint change only changes the
  paint.
- **The viewer's caches have a ceiling.** Parsed meshes were kept for the life of the app
  and never released, and the bike cache emptied itself wholesale on its seventh entry —
  throwing away the model you were looking at along with the rest. Both now keep the most
  recently used and drop only the coldest, and a dropped bike releases its texture pixels
  with it.
- **Protection is no longer hidden by default in the Rider tab.** It was, back when it
  rendered as a grey blob spanning the whole torso.
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
- **Release binaries are stripped and hardened against reverse-engineering.** Added a
  `[profile.release]` that strips the symbol table from every platform's artifact, builds with
  fat LTO + a single codegen unit (functions inlined/merged so a decompiler can't recover clean
  structure), and aborts on panic (no unwind tables). `opt-level` stays at 3 so the viewer's hot
  paths aren't sacrificed. On Windows this also means no PDB is produced. Self-update is
  unaffected — the updater signs file content after the build.
- **Sensitive endpoint paths no longer appear in a `strings` dump.** Wired in `obfstr` and
  XOR-obfuscated the runtime-built API paths and upload/download URLs (WordPress REST/admin-ajax
  paths, pixeldrain upload, Google Drive resolver). Public host bases stay as-is — they're
  visible in network traffic regardless and are bound into `const` config.
- **Source-level format hints kept out of the public repository.** Simplified the `flate2`
  dependency note and stripped the implementation comments from the `.pnt` decoder, so the
  committed source no longer documents on-disk container internals. Comment-only change —
  code, tests, and behavior are unchanged.

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
  - Online swapping is intentionally **out of scope** — the server is authoritative on
    what a joined client may change; this is offline/local only.
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

- **Riders are identified by their MX Bikes GUID**, not just their rider name. A name is
  free text you can change between sessions and two people can pick the same one; a GUID is
  stable per install, and the dedicated server writes it next to the name on every
  connection. The agent reads the server's own log to know who is actually connected —
  which turns out to be a far easier route to a live roster than decoding the live-timing
  UDP feed, since the game's plugin API exposes no GUID for anyone but yourself. Claiming a
  GUID is first-come, so nobody can assert someone else's identity and have their paints
  served under it. Rider-name matching stays as the fallback until a GUID is supplied.

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

- **Riders are identified by their MX Bikes GUID**, not just their rider name. A name is
  free text you can change between sessions and two people can pick the same one; a GUID is
  stable per install, and the dedicated server writes it next to the name on every
  connection. The agent reads the server's own log to know who is actually connected —
  which turns out to be a far easier route to a live roster than decoding the live-timing
  UDP feed, since the game's plugin API exposes no GUID for anyone but yourself. Claiming a
  GUID is first-come, so nobody can assert someone else's identity and have their paints
  served under it. Rider-name matching stays as the fallback until a GUID is supplied.

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
