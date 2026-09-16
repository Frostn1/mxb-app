# Building a track from a real place

This is how you turn a real motocross track, or any piece of real ground, into a track you
can ride in MX Bikes. You do not need to know anything about maps or GIS. You do need to
know roughly where the place is.

The short version:

1. Find the place and check whether anyone has surveyed it properly.
2. Fetch its ground. One button, about a megabyte, a few seconds.
3. Trace the lap on the aerial photo, because only you can tell which ribbon of dirt is the
   current track.
4. Hand the two files to the track builder.

Steps 1, 2 and 4 are the computer's job. Step 3 is yours, and this guide explains why it has
to be, so you are not left wondering whether the app is just being lazy.

---

## What you get, and where it goes

Everything for one place lives in one folder. On Windows that is:

```
%LOCALAPPDATA%\com.frost.mxbikes\places\<place-name>\
```

On macOS it is `~/Library/Application Support/com.frost.mxbikes/places/`, and on Linux
`~/.local/share/com.frost.mxbikes/places/`. The app makes the folder; you never have to.

Inside, for a place called Ironman Raceway:

| File | What it is | Size |
|---|---|---|
| `ironman-raceway.dem.tif` | The ground. Heights in metres, one number per cell. | about 1 MB |
| `ironman-raceway.imagery.png` | The aerial photo of the same patch, for tracing against. | about 0.6 MB |
| `ironman-raceway.hillshade.png` | The ground drawn as if lit by low sun, so you can see shapes. | about 0.3 MB |
| `ironman-raceway.lap.json` | The lap you traced. Made in step 3. | a few KB |
| `place.json` | Where all of it came from, when, and under what licence. | 1 KB |

**The two files that matter to the track builder are the `.dem.tif` and the `.lap.json`.**
Everything else is there to help you make the second one.

A place folder is about 2 MB. You can keep dozens without noticing. If you want them gone,
delete the folder or use Forget in the app, and that is the end of it.

### Why it is 2 MB and not 315 MB

Worth knowing, because older guides to this sort of thing will tell you to download survey
tiles. The United States publishes its elevation in 10 km by 10 km tiles, and the tile
covering Ironman Raceway is **315 MB**. You would download all of that to use 470 m of it.

This app does not do that. It asks the survey's own server for exactly the plot you want,
and gets back about **1 MB in about 1.5 seconds**. We checked the two against each other
cell by cell over the same ground: the biggest difference anywhere was **1.0 cm**, and the
typical difference was **0.57 cm**. For scale, the survey's own noise on flat ground is
about 2.4 cm, so the difference is smaller than the measurement error. It is the same data.

---

## Step 1: find your track's coordinates

You can type a name into the app and it will look it up. That works for anywhere that is on
the map by name, which includes most established circuits.

If the name does not find it, or finds the wrong thing, get the coordinates yourself:

- On **OpenStreetMap** (openstreetmap.org), right-click the spot and choose "Show address",
  or read them out of the URL after you zoom in. The URL ends with something like
  `#map=17/40.00800/-86.92910`, and those last two numbers are what you want.
- Any mapping site will give you a latitude and longitude. You are only reading a coordinate
  off the screen, which is a fact about the world and not anybody's copyrighted work.

Paste them into the app as `40.008, -86.9291`. Latitude first, longitude second. Longitude is
negative in the Americas and west of Greenwich; latitude is negative south of the equator.

**Aim at the middle of the circuit, not the car park or the entrance gate.** The plot is
centred on the point you give, so if you aim at the gate you get half a track and half a
field.

---

## Step 2: check what has actually been surveyed

Press Check coverage before you fetch anything. This costs one tiny request and it is the
single most useful thing in the whole process.

It tells you what the survey actually has at that spot. For Ironman Raceway it answers:

```
1 m    IN_Indiana_Statewide_LiDAR_2017_B17, flown 2017-03-03 to 2020-04-11
10.3 m USGS 1/3 arc-second (the fallback underneath it)
```

That is a good answer. It means a plane flew over with a laser and measured the ground every
metre.

A bad answer looks like this:

```
10 m   only. No LiDAR has been flown over this spot.
```

**This check exists because the servers will lie to you if you do not ask.** Every one of
these services will happily accept a request for 1 m cells over ground it only holds at
10 m or 30 m, and hand back a picture with 1 m cells in it. The numbers in between the real
measurements are invented by smoothing. It looks exactly like data. It is not data. Your
jumps will be gone, your ruts will be gone, and the track will ride like a bedsheet draped
over a hill.

The app refuses to pretend. If the finest thing available is coarser than 3 m it says so in
as many words before you spend anything.

### If you are asking the American service yourself

Worth knowing if you go round the app and query USGS directly, because it will mislead you.
The coverage endpoint is:

```
https://elevation.nationalmap.gov/arcgis/rest/services/3DEPElevation/ImageServer/identify
  ?geometry={"x":<easting>,"y":<northing>,"spatialReference":{"wkid":<epsg>}}
  &geometryType=esriGeometryPoint&returnCatalogItems=true&returnGeometry=false&f=json
```

It answers with a list, and **most of what comes back is not survey data**. The service keeps
zoomed-out copies of itself for drawing maps quickly, and those copies are in the list too,
reporting cell sizes like 75 m, 150 m, 300 m and upwards. Read the list naively and you will
conclude your venue has nothing better than 75 m and give up, while 1 m LiDAR sits in the
same response further down.

The real entries are the ones that have a `DEM_Type` field. The zoomed-out copies do not.
So: ignore everything without `DEM_Type`, then take the smallest `LowPS` of what is left.

A real answer looks like this:

```
LowPS 1      Name "IN_Indiana_Statewide_LiDAR_2017_B17"   DEM_Type 1
             StartDate 20170303   EndDate 20200411
LowPS 10.31  Name "n41w087"                               DEM_Type 1
             title "USGS 1/3 Arc Second n41w087"
```

Two real surveys, 1 m over 10.3 m, and the 1 m one is the answer. Anything with a `Name`
that looks like `Ov_i02_L01_R0000005B_C0000001A.tif` is a drawing copy, not a survey.

And sometimes `identify` returns **nothing but** drawing copies: 75 m, 150 m, 300 m and
coarser, with no survey in the list at all, over ground that has 1 m LiDAR. It looks
authoritative and it is not. When that happens, ask the catalogue directly instead:

```
https://elevation.nationalmap.gov/arcgis/rest/services/3DEPElevation/ImageServer/query
  ?geometry=<envelope>&geometryType=esriGeometryEnvelope
  &spatialRel=esriSpatialRelIntersects
  &outFields=Name,LowPS,HighPS,ProductName&returnGeometry=false&f=json
```

At Ironman that returns the 1 m `IN_Indiana_Statewide_LiDAR_2017_B17` alongside the 10.3 m
and 30.9 m fallbacks, which is the truth.

**So: never conclude a place has no good survey from a single coarse answer.** Check the
catalogue before you give up on a venue.

The app does this filtering for you. This is only here so that if you go direct, you do not
get the wrong answer and abandon a venue that is perfectly well covered.

### The other thing to check: terrain or surface

Two kinds of elevation model exist and the difference matters enormously.

- **Terrain model**, also called DTM or bare earth. Trees and buildings have been stripped
  out, and what is left is the dirt. **This is what you want.**
- **Surface model**, also called DSM or DOM. The first thing the beam hit. A wood is a solid
  20 m plateau with a flat top. A start tower is a pillar. A tent is a hill.

Every source the app fetches from automatically is a terrain model. The worldwide 30 m
fallback is a surface model, and the app says so every time it mentions it.

---

## Step 3: choose how big a plot to fetch

The default is 470 m square, which is what the track generator uses out of the box. For a
real circuit that is usually **too small**, and here is why that matters more than it looks.

### Bigger plots are not just roomier, they are more honest

The track's terrain is stored as a fixed grid of **2049 by 2049 samples**, no matter how big
the plot is. So the plot size decides how far apart those samples land:

| Plot | Distance between samples | What happens to 1 m survey data |
|---|---|---|
| 470 m | 0.23 m | stretched 4.4x, so about 4 samples in 5 are invented |
| 800 m | 0.39 m | stretched 2.6x |
| 1200 m | 0.59 m | stretched 1.7x |
| 2000 m | 0.98 m | stretched 1.0x, essentially one sample per real measurement |

Upsampling does not add detail. It smoothly guesses the space between real measurements. So
a smaller plot does not give you a more detailed track; it gives you the same detail spread
thinner, with more guesswork in between.

A full motocross lap is normally **2000 m to 2350 m**, and a lap that long needs somewhere in
the region of a 1000 m to 1500 m plot to fit. That is the right setting for a real circuit,
and it costs about **6 MB** instead of 1 MB, which is nothing.

Measured example: Ironman Raceway at 470 m holds 23.5 m of relief and only part of the
circuit. The same place at 1200 m holds 29.4 m of relief and the whole venue.

**Rule of thumb: make the plot big enough that the whole lap fits with room to spare, and
never smaller than you need.**

### Plots are square

The app always fetches a square, centred on your point. If you are cutting a plot by hand
from a file you downloaded yourself, cut it square too: take the longer side of your lap's
extent, add a margin on both ends, and use that for both axes. A rectangle that is 510 m
wide and 373 m tall cannot have a 470 m square taken out of it, and you will find that out
late.

---

## Step 4: fetch

Press Fetch. One request for the ground, one for the aerial photo, and that is all. Nothing
downloads in the background, nothing downloads while you type, nothing downloads when you
open the panel. These are public services paid for by somebody else's taxes and the app only
asks when you ask it to.

When it lands, look at the hillshade picture before you go any further. You are checking
three things:

- **Can you see the track?** Corners, jump faces and berms should show up as shapes. If the
  picture is a smooth blur with no track in it, you got coarse data. Go back to step 2.
- **Is the relief sensible?** The app tells you the lowest and highest ground in the plot.
  Under about 1 m means you have a car park or a lake. Over about 80 m across 500 m means you
  have a mountainside, which may well be correct but is worth a second look.
- **Are there holes?** Blank patches mean the survey has gaps there, usually over water.

---

## Step 5: trace the lap, and why this part is yours

Here is the thing the feasibility work found, and it is the reason this cannot be automatic.

**A motocross venue that is rebuilt every year holds every layout it has ever had in the
elevation data, all at once, all equally real.** The bulldozer moves the dirt but the shape
of last year's corner stays in the ground. Ironman shows at least three overlapping
generations of track. Nothing in the elevation tells you which one is this season's, because
as far as the laser is concerned they are all just dirt.

The aerial photo solves it instantly. The lane in use is the one worn down to bare dirt, with
tyre lines in it. The old ones have grass growing back. A human sees this in about two
seconds. So you trace it, on the photo, and the app keeps your points.

### How to trace well

- **Follow the middle of the worn lane, not the edge of the graded corridor.** The track
  builder fits the racing line to your points and samples the ground underneath them. Trace
  the outside edge and you will put the racing line up the berm.
- **Put points close together through corners** and sparsely down straights. Every 1 m to 3 m
  round a corner is plenty; a straight needs two points.
- **Go round the way the track is ridden.** The start direction comes from your first two
  points, so a lap traced backwards builds a track that runs backwards. If you genuinely do
  not know which way a circuit runs, say so rather than guess, and mark it.
- **Close the lap.** Do not repeat the first point at the end; just tick Closed.

### What if there is no aerial photo?

Openly licensed aerial photography at a resolution you can pick a lane out of exists for the
United States, France and the Netherlands, and that is close to the whole of it. Elsewhere
you will be tracing on the hillshade alone, which is harder but not impossible, especially
for a purpose-built circuit whose berms and jump faces are obvious in the shape.

We do not use Google, Apple or Bing imagery, and never will. Their photography is licensed
for looking at inside their own products. Tracing a lap off it would put a derivative of
their pictures inside a track you hand to other people, and that is not a mess anyone wants
to be in.

---

## Step 6: hand it over

The trace is saved as `<place>.lap.json` next to the elevation. Give the track builder the
two paths and it does the rest.

The file is deliberately simple, and you can read or edit it in any text editor:

```json
{
  "version": 1,
  "kind": "mxb-lap-trace",
  "name": "Ironman Raceway",
  "crs": "EPSG:26916",
  "units": "m",
  "closed": true,
  "default_width_m": 6.0,
  "start_index": 0,
  "points": [[506051.3, 4428647.5], [506060.1, 4428646.2, 8.0]],
  "dem": { "...": "where the ground came from, when it was flown, and its licence" }
}
```

Points are in the elevation file's own coordinate system, easting first, in metres. A point
can carry its own width as a third number; without one it uses `default_width_m`.

If you hand-edit one of these, **write each key once**. It is tempting, when you are not sure
whether a reader wants `default_width_m` or `defaultWidthM`, to put both in and be safe. That
makes it worse: a JSON object with two keys pointing at the same field is rejected outright by
a strict reader, so a file that tried to satisfy everyone satisfies no one. The app writes
snake_case, and reads either.

The `dem` block records what the ground is, when it was surveyed, and its licence, and that
travels with the track. This matters more than it sounds: Ironman's survey was flown between
2017 and 2020, and the venue is rebuilt every year, so a track built from it is **a dated
snapshot and not today's Ironman**. Six months from now somebody will want to know which
vintage they are looking at, and the file will tell them.

---

## Which countries this works for

### Fetched automatically, in the app

All four tested with a real download, not taken from documentation.

| Where | Cells | Ground | Notes |
|---|---|---|---|
| **France** | **0.5 m** | bare earth | IGN LiDAR HD. Falls back to RGE ALTI at 1 m where LiDAR HD has not flown yet. |
| **Netherlands** | **0.5 m** | bare earth | AHN. Nationwide. |
| **United States** | 1 m | bare earth | USGS 3DEP, but only where it has been flown. Check coverage first. |
| **England** | 1 m | bare earth | Environment Agency National LiDAR. Scotland and Wales run separate programmes. |

France and the Netherlands are **better than the United States**, which surprises people.
0.5 m cells hold roughly twice the fine detail of 1 m cells, and that shows up directly in
whether a rut survives the trip.

### Confirmed working, but you fetch the file yourself

These have open, working, no-account services, and they are not wired into the app yet. Paste
the address into a browser, save the GeoTIFF it gives you, and use **Import a GeoTIFF** in
the app. An imported file is exactly as good as a fetched one; the app does not care where it
came from.

| Where | Cells | Service |
|---|---|---|
| Switzerland | **0.5 m** | swissALTI3D, at `data.geo.admin.ch` |
| Norway | 1 m | `hoydedata.no`, three services split by longitude |
| Germany: North Rhine-Westphalia | 1 m | `wcs.nrw.de` |
| Germany: Brandenburg | 1 m | `isk.geobasis-bb.de`, slow, allow 20 seconds |
| Germany: Mecklenburg-Vorpommern | 1 m | `geodaten-mv.de` |
| Belgium: Flanders | 1 m | `geo.api.vlaanderen.be` |
| Poland | 1 m | `mapy.geoportal.gov.pl`, slow, about 25 seconds |
| Czechia | 2 m | `ags.cuzk.gov.cz` |
| Spain | 5 m | `servicios.idee.es`, national only. Several regions publish better. |

Germany has no single national service, so it is one per state, and only three states are
confirmed so far. Scotland, Wales and Wallonia all publish LiDAR but were not reachable at
the addresses tried.

### Nowhere on this list

Two honest answers, in order of preference.

**First, look for your own country's mapping agency.** Many more countries publish open
LiDAR than are listed above, and the list grows. Search for your country plus "open data"
plus "DTM" or "digital terrain model". What you want is a **GeoTIFF**, **north up**, **1 m
or finer**, and **terrain not surface**. Save it and import it.

**Second, the worldwide fallback: Copernicus GLO-30.** Free, no account, covers the whole
world between 60 degrees south and 84 degrees north. And it is **30 m cells and a surface
model**, which is to say it has trees and buildings baked into the ground.

Be clear about what that means for a motocross track. At 30 m, one cell is wider than most
jumps are long. A tabletop is not in there. A berm is not in there. A rut is not in there,
not even slightly. What you get is the shape of the hillside the track sits on, and nothing
else. That is genuinely useful if you are building a track on rolling ground and want the
hill to be real. It is no use at all for reproducing an actual circuit.

If 30 m is all you have, the honest approach is to use it for the land and design the track
itself, rather than pretending you are reproducing one.

---

## What resolution actually buys you

Roughly, a feature needs about three cells across it before it exists in any meaningful way.

| Cell size | What survives |
|---|---|
| 0.5 m | Ruts, tyre lines, the shape of a berm face, jump lips |
| 1 m | Jump faces and landings, berms, the shape of a corner. Fine ruts are marginal. |
| 2 m | Jumps as lumps. Berms mostly gone. |
| 5 m | The track as a shallow trough in the field. No features. |
| 10 m | You can see there is a hill. |
| 30 m | The hill, and nothing else. |

One measured example of the difference between 0.5 m and 1 m, taken over the same 470 m of
French ground: the 0.5 m survey carried **0.033 m** of height change between neighbouring
cells on average, and the 1 m survey **0.015 m**. The finer data holds about **2.2 times** the
fine relief. That is not a labelling difference, it is more track.

---

## Licences, and what you must do about them

Every source here is either public domain or openly licensed. None of it costs money and none
of it needs an account. Some of it needs **credit**, and that credit has to travel with
anything you release.

The app records the obligation in `place.json` and carries it into the trace file, so it is
attached to the work rather than living in your memory. But you are the one publishing the
track, so it is on you.

| Source | Licence | What you must say |
|---|---|---|
| **USGS 3DEP** (US elevation) | Public domain, US Government work | Nothing required. Crediting USGS is polite. |
| **USGS NAIPPlus** (US imagery) | Public domain, US Government work | Nothing required. |
| **AHN** (Netherlands) | CC0 1.0 | Nothing required. |
| **IGN** (France) | Licence Ouverte / Open Licence Etalab 2.0 | Credit required: `© IGN — Licence Ouverte / Open Licence Etalab 2.0` |
| **Environment Agency** (England) | Open Government Licence v3.0 | Credit required: `Contains public sector information licensed under the Open Government Licence v3.0. © Environment Agency copyright and/or database right.` |
| **PDOK Luchtfoto** (Netherlands imagery) | CC BY 4.0 | Credit required: `Luchtfoto: PDOK / Beeldmateriaal Nederland, CC BY 4.0` |
| **Copernicus GLO-30** | Copernicus open terms | Credit required, and the full ESA/Airbus notice. The app stores it. |
| **OpenStreetMap** (the place search) | ODbL | Only coordinates are used, which are facts rather than a substantial extract, so nothing travels into your track. Crediting OpenStreetMap is polite. |

The simplest way to honour all of this is a line in your track's readme naming the source of
the elevation and the imagery. Copy the strings out of `place.json`; they are exact.

Two things that are never acceptable, regardless of how the question is phrased:

- **Google, Apple and Bing imagery or elevation.** Not for tracing, not for reference, not
  "just to check". Their terms do not permit it and there is no version of this that is fine.
- **Anything behind a login or a click-through licence**, unless you personally accepted terms
  that permit redistribution of what you derive from it.

---

## When something goes wrong

**"Every cell in that plot is empty."** The service answered but has no survey there. Common
in France, where the 0.5 m programme is still rolling out, and near coastlines. Move the plot
a few hundred metres, or use a coarser source.

**"Only 10 m cells here."** No LiDAR has been flown. See the fallbacks above.

**"That GeoTIFF doesn't say where on the earth it is."** An imported file with its
georeferencing stripped out. Re-export it from wherever you got it as a plain north-up
GeoTIFF.

**"That GeoTIFF is rotated."** The app only handles north-up grids. Re-export it north up.

**"The server took too long."** Some services are genuinely slow. Poland takes about 25
seconds and Brandenburg about 15. Try again, or try a smaller plot.

**The hillshade is a smooth blur.** Coarse data upsampled. Go back to the coverage check.

**The track in the photo does not match the shapes in the hillshade.** Expected, and it is
the whole reason you trace by hand. The photo and the survey were taken years apart, and the
venue was rebuilt in between. The photo shows what is there now; the elevation shows every
layout at once. Trust the photo for where the lap goes and the elevation for what the ground
does.

---

## The worked example

`Ironman Raceway`, Crawfordsville, Indiana. 40.008 N, 86.9291 W.

- Survey: USGS 3DEP 1 m, `IN_Indiana_Statewide_LiDAR_2017_B17`, flown March 2017 to April
  2020, public domain.
- 470 m plot: 1,049,883 bytes, 1.5 seconds, 23.5 m of relief, holds part of the circuit.
- 1200 m plot: 6,555,579 bytes, 29.4 m of relief, holds the whole venue.
- Imagery: USGS NAIPPlus, public domain.

**The trace shipped with this example is provisional.** It was placed by eye off the aerial
photo and it has known gaps: the south straight and the east hairpin are certain, the
west and north-west sections are a best guess between several parallel graded lanes, and the
direction of travel is unverified. Those segments are labelled in the file itself rather than
only mentioned here. Treat it as a demonstration that the pipeline works end to end, not as
an accurate reproduction of Ironman Raceway.
