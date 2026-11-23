# Where open elevation and aerial imagery come from

**Every endpoint below was tested on 2026-09-16** by fetching a real bounding box from this
machine and inspecting what came back: dimensions, pixel scale, georeferencing, bit depth,
nodata and actual elevation values. Sizes and timings are measured, not estimated.

**Services change.** Addresses move, layers get renamed, licences get revised. Treat anything
here as known-good on that date and re-probe before relying on it. Two of the endpoints in
this document replaced earlier ones that had already gone dead, so this is not hypothetical.

If you are a rider trying to build a track, you want
[real-place-tracks.md](real-place-tracks.md) instead. That is the how-to; this is the
reference behind it. Four of these sources are wired into Studio and fetch on a button; the
rest are here so you can download the file yourself and import it, which the guide explains.

---

Everything marked **tested** below was fetched with a real bounding box. Everything marked
**read** is documentation only and has not been exercised.

Baseline already in hand before this work: USGS 3DEP (US, 1 m, ImageServer), AHN (Netherlands, 0.5 m,
WCS), Copernicus GLO-30 (worldwide, 30 m, DSM).

---

## 1. Summary table — elevation

DTM = bare earth (what we need). DSM = surface model, trees and buildings in the ground (useless for a track).

| Country | Best open source | Res | DTM/DSM | Protocol | Keyless | Tested | Works |
|---|---|---|---|---|---|---|---|
| **France** | IGN LiDAR HD MNT (fallback RGE ALTI) | **0.5 m** / 1 m | DTM | WMS GetMap | yes | yes | **yes** |
| **Switzerland** | swissALTI3D | **0.5 m** | DTM | STAC + COG | yes | yes | **yes** |
| **Scotland** | SRSP National LiDAR Programme | **0.5 m** | DTM | S3 + COG | yes | yes | **yes** |
| Netherlands | AHN (already shipped) | 0.5 m | DTM | WCS 2.0.1 | yes | prior | yes |
| **England** | EA National LiDAR Composite | 1 m | DTM | WCS 2.0.1 | yes | yes | **yes** |
| **Wales** | Welsh Gov LiDAR 2020-2023 | 1 m | DTM | WFS + blob | yes | yes | **yes** |
| **Norway** | Kartverket NHM DTM | 1 m | DTM | ArcGIS ImageServer | yes | yes | **yes** |
| **Germany (NRW)** | Geobasis NRW DGM1 | 1 m | DTM | WCS 2.0.1 | yes | yes | **yes** |
| **Germany (BB)** | LGB Brandenburg DGM | 1 m | DTM | WCS 2.0.1 | yes | yes | **yes** (slow, 15 s) |
| **Germany (MV)** | GeoBasis-DE/M-V DGM | 1 m | DTM | WCS 2.0.1 | yes | yes | **yes** |
| **Belgium (Flanders)** | DHMV II | ~1 m | DTM | WCS 2.0.1 | yes | yes | **yes** (gotchas) |
| **Poland** | GUGiK NMT GRID1 | 1 m | DTM | WCS 2.0.1 | yes | yes | **yes** (ASCII only, 25 s) |
| **Czechia** | ČÚZK DMR 5G | 2 m | DTM | ArcGIS ImageServer | yes | yes | **yes** |
| **Spain (Navarra)** | IDENA MDT 2M | 2 m | DTM | WCS 2.0.1 | yes | yes | **yes** |
| **Spain (national)** | CNIG MDT05 | 5 m | DTM | WCS 2.0.1 | yes | yes | **yes** (ASCII for floats) |
| Germany (national) | BKG DGM1 | 1 m | DTM | WCS | — | yes | **no** — HTTP 403 `NOACCESS_SERVICE` |
| Belgium (Wallonia) | SPW MNT | 1 m | DTM | ArcGIS | — | yes | **no** — 404 on path tried |
| Spain (Catalonia) | ICGC MET2m | 2 m | DTM | WMS | yes | yes | **no** — returns RGB picture, not data |
| Spain (Andalucía) | REDIAM | — | — | — | — | yes | **no** — endpoints dead |
| Italy, Austria | regional only | — | — | — | — | partial | **not found** |
| Denmark, Sweden, Finland | national portals | 0.4-1 m | DTM | — | **no** | no | account-gated (read) |
| Worldwide | Copernicus GLO-30 | 30 m | **DSM** | COG on S3 | yes | prior | yes (coarse) |

## 2. Summary table — aerial imagery

Imagery is the weak point of the pipeline outside the US and NL; these all beat 1 m/px.

| Country | Source | Res | Protocol | Keyless | Tested | Works | Viewed |
|---|---|---|---|---|---|---|---|
| **Switzerland** | SWISSIMAGE 10 cm | **0.10 m** | STAC + COG | yes | yes | **yes** | no |
| **France** | IGN BD ORTHO V3 | **0.20 m** | WMS GetMap | yes | yes | **yes** | **yes** |
| **Germany (NRW)** | Geobasis NRW DOP | **0.20 m** | WMS GetMap | yes | yes | **yes** | **yes** |
| **Belgium (Flanders)** | Orthofotomozaïek | 0.23 m | WMS GetMap | yes | yes | **yes** | **yes** |
| **Poland** | GUGiK ortofotomapa | 0.24 m | WMS GetMap | yes | yes | **yes*** | **yes** |
| **Spain** | PNOA máxima actualidad | 0.25 m | WMS GetMap | yes | yes | **yes** | no |
| England | EA Vertical Aerial Photography | 0.125 m | — | — | yes | **no** — 404 on slug tried | — |

\* Poland's imagery licence **expressly excludes automated harvesting**. See the licence table.

## 3. Licence and attribution table

Quoted from each service's own `Fees` / `AccessConstraints` capabilities fields unless noted.
**This is the part a rider is legally on the hook for.**

| Source | Licence | Required attribution (use verbatim) | Read from |
|---|---|---|---|
| England EA | Open Government Licence v3.0 | `© Environment Agency copyright and/or database right 2022. All rights reserved.` | https://environment.data.gov.uk/dataset/13787b9a-26a4-4775-8523-806d13af58fc |
| Scotland SRSP | OGL v3 ("all data ... unless otherwise specified") | no verbatim string given; use standard OGL form naming Scottish Government / SRSP | https://remotesensingdata.gov.scot/about + AWS Open Data registry |
| Wales | Open Government Licence | no verbatim string given; standard OGL form naming Welsh Government / NRW | https://datamap.gov.wales/maps/lidar-data-download/metadata_detail |
| France IGN (ortho, RGE ALTI, LiDAR HD) | Licence Ouverte / Open License (Etalab) | `IGN` — service declares Attribution "Institut national de l'information géographique et forestière", ign.fr | CSW record `ID=IGNF_BD-ORTHO` on data.geopf.fr; use-limitation reads "Aucune contrainte" |
| Netherlands AHN | **CC0 1.0 — public domain, NO attribution required** | none | AHN WCS AccessConstraints: `otherRestrictions; Geen beperkingen; http://creativecommons.org/publicdomain/zero/1.0/deed.nl` |
| Switzerland swisstopo | free incl. commercial, source reference required | `©swisstopo` (or "Federal Office of Topography swisstopo") | https://www.swisstopo.admin.ch/en/terms-of-use-free-geodata-and-geoservices |
| Germany NRW | **Datenlizenz Deutschland – Zero 2.0 — NO attribution required** | none | WMS/WCS Fees: `"Datenlizenz Deutschland – Zero" (https://www.govdata.de/dl-de/zero-2-0). Jede Nutzung ist ohne Einschränkungen oder Bedingungen zulässig.` |
| Germany Brandenburg | dl-de/by-2-0 | `© GeoBasis-DE/LGB, dl-de/by-2-0, Daten geändert` | WCS AccessConstraints, which gives this exact example |
| Germany M-V | attribution, **"deutlich sichtbar"** | `© GeoBasis-DE/M-V <year of last data delivery>` | WCS AccessConstraints |
| Belgium Flanders | free, terms **by reference** — not named inline | unknown; page not read in full | AccessConstraints points to vlaanderen.be gebruiksrecht page |
| Poland (elevation) | no restrictions | Fees `none`, AccessConstraints `none` | NMT WCS capabilities |
| Poland (imagery) | free **except automated harvesting** | see restriction | ORTO WMS AccessConstraints: `Wykorzystanie usługi nie podlega żadnym ograniczeniom z wyłączeniem automatycznego pobierania i kolekcjonowania obrazów (tzw. harvesting).` |
| Spain CNIG (MDT + PNOA) | CC BY 4.0 | `CC BY 4.0 scne.es` | both WCS and WMS AccessConstraints read exactly `CC BY 4.0 scne.es` |
| Spain Navarra | CC BY 4.0 | `Servicio proporcionado por el Gobierno de Navarra.` (this exact Spanish sentence) | IDENA WCS AccessConstraints |
| Czechia ČÚZK | **CC BY 4.0 — UNVERIFIED first-party** | `© ČÚZK` (from the service's own copyrightText) | ImageServer JSON `copyrightText`; CC BY 4.0 claim only from data.europa.eu, not ČÚZK itself |
| Norway Kartverket | **CC BY 4.0 — NOT fully verified for this product** | `©Kartverket` | kartverket.no terms page (general free products; does not name the elevation model) |

Two rows above are honestly shaky: **Czechia** and **Norway**. Both are almost certainly fine, but I could
not get a first-party statement naming the specific product. Confirm before shipping either in a release.

**Two licence traps worth naming:**
- Switzerland's STAC collection reports `"license": "proprietary"`. That is wrong in spirit — swisstopo's
  own terms permit commercial use with a source reference. An automated licence check reading the STAC
  field would wrongly reject Switzerland.
- Poland's imagery is the only source here with a real restriction. One-shot fetch on an explicit user
  action is defensible; a crawler, prefetch or cache sweep is expressly excluded.

## 4. NODATA — eight different behaviours across fifteen services

This is the detail most likely to put a cliff or a 32 km hole in somebody's track.

| Sentinel | Services |
|---|---|
| `-9999` | France LiDAR HD MNT, Switzerland, Germany NRW, Germany Brandenburg, Belgium Flanders, Wales |
| `-99999` | France RGE ALTI HIGHRES — **same server as LiDAR HD, one extra nine** |
| `-32767` | Scotland |
| `-3.4028234663852886E38` (negative float max) | England EA |
| `+3.4028234663852886E38` (**positive** float max) | Netherlands AHN, Spain Navarra |
| no GDAL_NODATA tag at all | Norway, Czechia, Germany M-V |
| none seen | Poland (ASCII), Spain (both paths) |

**Rule:** read TIFF tag 42113 when present, but never depend on it — three services ship no tag.
Additionally reject anything outside roughly -500 to 9000 m, and reject both float-max magnitudes
regardless of sign. A naive "below -1000 means void" test sails straight past positive float max;
a "below -9000" test sails straight past Scotland's -32767, which sits inside any plausible elevation band.

## 5. The two silent-poisoning traps

Both produce a file that passes every structural check and contains no usable elevation.

1. **A "data" request that returns a rendered picture.** Poland's elevation WCS with `FORMAT=image/tiff`
   returns HTTP 200 and a valid 470x470 GeoTIFF with correct ModelPixelScale and ModelTiepoint — and it is
   8-bit RGB, a hillshade render, not elevation. Catalonia's MET2m WMS does exactly the same. Two independent
   services is a pattern, not a coincidence. **Test: check BitsPerSample is 32 and PhotometricInterpretation
   is not RGB.** Poland's real data comes from `FORMAT=image/x-aaigrid`.
2. **An error page claiming to be a TIFF.** Gateway 403/404/504 responses arrive with image content-types.
   **Test: first two bytes must be `II` or `MM`.**

## 6. Per-source detail with exact working requests

### France — IGN LiDAR HD MNT, 0.5 m DTM
```
https://data.geopf.fr/wms-r/wms?SERVICE=WMS&VERSION=1.3.0&REQUEST=GetMap&LAYERS=IGNF_LIDAR-HD_MNT_ELEVATION.ELEVATIONGRIDCOVERAGE.LAMB93&STYLES=&CRS=EPSG:2154&BBOX=408205,6806602,408675,6807072&WIDTH=940&HEIGHT=940&FORMAT=image/geotiff
```
200, 3,534,913 B, 3.1 s, 940x940 float32, scale (0.5,0.5), tiepoint = (bbox minX, bbox maxY), EPSG:2154,
nodata **-9999**. BBOX is minX,minY,maxX,maxY, easting first even in WMS 1.3.0 — no axis swap.
Fallback where LiDAR HD has not flown: `LAYERS=ELEVATION.ELEVATIONGRIDCOVERAGE.HIGHRES` (RGE ALTI, 1 m,
national), **nodata -99999**. Never use the `_MNS_` (surface) or `_MNH_` (canopy height) siblings.
`FORMAT=image/x-bil;bits=32` gives a raw headerless float32 BIL, exactly W*H*4 bytes, if you'd rather skip TIFF.

**Measured proof the layer choice is real:** same box, sampled 940x940 both ways, mean absolute elevation
step between adjacent cells was 0.0327 m for LiDAR HD vs 0.0147 m for RGE ALTI. LiDAR HD carries 2.2x the
fine relief — it is true 0.5 m data, not 1 m upsampled.

### France — BD ORTHO, 20 cm imagery
```
https://data.geopf.fr/wms-r/wms?SERVICE=WMS&VERSION=1.3.0&REQUEST=GetMap&LAYERS=HR.ORTHOIMAGERY.ORTHOPHOTOS&STYLES=&CRS=EPSG:2154&BBOX=408205,6806602,408675,6807072&WIDTH=2350&HEIGHT=2350&FORMAT=image/jpeg
```
200, 781,621 B, 11.4 s (JPEG). GeoTIFF is 16,568,023 B and PNG 12,233,238 B for the same pixels —
**JPEG is 21x smaller**, and you already know the bbox. MaxWidth/MaxHeight **5010**.
Verified visually: cars, lane markings and pedestrian crossings all resolvable.
Géoplateforme rate limit: **40 req/s per IP** on WMS-Raster, then HTTP 429 for 5 s.

### England — EA National LiDAR Composite, 1 m DTM
```
https://environment.data.gov.uk/spatialdata/lidar-composite-digital-terrain-model-dtm-1m/wcs?SERVICE=WCS&VERSION=2.0.1&REQUEST=GetCoverage&COVERAGEID=13787b9a-26a4-4775-8523-806d13af58fc__Lidar_Composite_Elevation_DTM_1m&SUBSET=E(346505,346975)&SUBSET=N(153582,154052)&FORMAT=image/tiff
```
200, 3,445,187 B, 2.6 s, 470x470 float32 uncompressed, tiled 464x464. EPSG:**27700**, axis labels `E` `N`,
easting first, no SUBSETTINGCRS needed. **Uses TIFF tag 34264 ModelTransformation, not 33550/33922** —
a reader looking only for pixel-scale + tiepoint sees an unplaced image. nodata **-3.4028234663852886E38**.
Sanity check: Cheddar Gorge returned 24.13-159.06 m, 134.9 m of relief in a 470 m box.
Coverage ~99% of **England only**; surveys flown 6 Jun 2000 - 2 Apr 2022, so data can be 20+ years old.

Note the host: `environment.data.gov.uk/spatialdata/<slug>/wcs` works.
`environment.data.gov.uk/image/rest/services/...` (the ArcGIS-looking path) returns `{"error":{"code":400,"message":"Invalid URL"}}`.

### Scotland — SRSP National LiDAR Programme, 0.5 m DTM
```
https://srsp-open-data.s3.eu-west-2.amazonaws.com/?list-type=2&prefix=lidar/national-lidar-programme/dtm/27700/gridded/&max-keys=6
https://srsp-open-data.s3.eu-west-2.amazonaws.com/lidar/national-lidar-programme/dtm/27700/gridded/NR5808_50cm_DTM_ScotlandNationalLiDAR.tif
```
200, 5,299,313 B, 1.9 s. 817x2000 float32, scale (0.5,0.5), tiepoint (158591.5, 609000.0), LZW,
EPSG:27700, nodata **-32767**. Ranged GET returns **206**. Public bucket, no AWS account, no signing.
Key is constructible from BNG: `<100 km square><E km 2 digits><N km 2 digits>_50cm_DTM_ScotlandNationalLiDAR.tif`.
**Two traps:** tiles are *cropped to the data extent*, so read the tiepoint — this one starts at E 158591.5,
not on the km line, and is 817 px wide not 2000; and coverage is partial *inside* a tile — only 62% of cells
were valid here. Fall back to `phase-1/`..`phase-6/` (1 m, 10 km tiles) where NLP is absent. Use `dtm/`, never `dsm/`.

### Wales — Welsh Government LiDAR 2020-2023, 1 m DTM
```
https://datamap.gov.wales/geoserver/ows?service=WFS&version=2.0.0&request=GetFeature&typeNames=geonode:welsh_government_lidar_tile_catalogue_2020_2023&outputFormat=application/json&srsName=EPSG:27700&bbox=316000,176000,316500,176500,EPSG:27700
https://dmwproductionblob.blob.core.windows.net/lidar-zips/2020-22/dtm/wg_del_1_222381_20200322dtm.tif
```
Catalogue: 200, 2,869 B, 4 tiles matched. Tile: 200, 2,040,165 B, 2.4 s, **1000x1000 float32, scale (1,1),
tiepoint (222000, 382000)** — clean full 1 km tile on the km line, Deflate, EPSG:27700, nodata **-9999**,
100% valid. Ranged GET **206**. 22,473 tiles, ~70% of Wales.
The blob URL **cannot** be constructed from coordinates (it encodes delivery number and flight date), so the
catalogue query is mandatory — but it's under 3 KB. `CQL_FILTER=british_gr%3D%27SH2281%27` also works.
**The catalogue carries the flight date per tile** — nothing else here does, and a rider wants to know
their track was surveyed in 2020 and not 2003.
Do *not* use `geonode:nrw_lidar_tile_catalogue_archive`: 1998-2000s, 2 m, ZIPs, and most rows have an
empty `dtm_url` with only a DSM.

### Switzerland — swissALTI3D 0.5 m DTM + SWISSIMAGE 10 cm
```
https://data.geo.admin.ch/api/stac/v1/search?collections=ch.swisstopo.swissalti3d&bbox=7.44,46.94,7.45,46.95&limit=3
https://data.geo.admin.ch/ch.swisstopo.swissalti3d/swissalti3d_2019_2600-1199/swissalti3d_2019_2600-1199_0.5_2056_5728.tif
```
Elevation: 200, 15,683,822 B, 0.57 s, 2000x2000 float32, scale (0.5,0.5), tiepoint (2600000, 1200000),
LZW, tiled 128x128, EPSG:2056 (LN02 heights, 5728), nodata **-9999**. Ranged GET **206**.
```
https://data.geo.admin.ch/api/stac/v1/search?collections=ch.swisstopo.swissimage-dop10&bbox=7.44,46.94,7.45,46.95&limit=2
https://data.geo.admin.ch/ch.swisstopo.swissimage-dop10/swissimage-dop10_2018_2600-1199/swissimage-dop10_2018_2600-1199_0.1_2056.tif
```
Imagery: 200, 66,456,324 B, 3.2 s, **10000x10000 RGB, scale (0.1, 0.1)**, JPEG-in-TIFF, tiled 256x256,
ranged GET **206**. The finest imagery in this report by a factor of two.

**Their STAC is half broken.** `/collections/<id>/items?bbox=...` and `POST /search` both return HTTP 500
with a raw Postgres error (`column stac_api_item.cf_standard_name does not exist`). **`GET /search` works.**
So does `/collections/<id>/items/<id>` and `/items?limit=N` without a bbox.
Tile ids are 1 km, `<E/1000>-<N/1000>`, but **the year varies per tile and differs between the elevation and
imagery collections for the same tile** (2019 vs 2018 here) — you cannot construct the filename from
coordinates alone, and you must query each collection separately.

### Norway — Kartverket NHM DTM, 1 m (drop-in ArcGIS, identical shape to USGS)
```
https://hoydedata.no/arcgis/rest/services/NHM_DTM_25833/ImageServer/exportImage?bbox=262325,6649208,262795,6649678&bboxSR=25833&imageSR=25833&size=470,470&format=tiff&pixelType=F32&f=image
```
200, 1,049,839 B, 2.0 s, 470x470 float32, scale (1,1). Service JSON: pixelSize 1, pixelType F32,
serviceDataType `esriImageServiceDataTypeElevation`, maxImage 4096x4096. **No nodata tag.**
Three zone services: `NHM_DTM_25832`, `NHM_DTM_25833`, `NHM_DTM_25835` — pick by longitude.
Avoid `NHM_DTM_TOPOBATHY_*` (fills water with sea-floor depths) and `NHM_DOM_*` (surface model).
Service root `https://hoydedata.no/arcgis/rest/services?f=json` enumerates.

### Czechia — ČÚZK DMR 5G, 2 m (also drop-in ArcGIS)
```
https://ags.cuzk.gov.cz/arcgis2/rest/services/dmr5g/ImageServer/exportImage?bbox=-859089,-1014733,-858619,-1014263&bboxSR=5514&imageSR=5514&size=235,235&format=tiff&pixelType=F32&f=image
```
200, 263,524 B, 1.5 s, 235x235 float32, scale (2,2), elevations 384.21-456.54 m. **No nodata tag.**
EPSG:**5514** (S-JTSK Krovak East North) — **coordinates are negative in both axes**; extent is
xmin -904703, ymin -1227414, xmax -431605, ymax -935118. maxImageWidth 15000 but maxImageHeight only 4100.
Host is `ags.cuzk.gov.cz`; the old `ags.cuzk.cz` 400s.

### Germany — per-state, there is no working national service
BKG's `sgx.geodatenzentrum.de/wcs_dgm1` returns **HTTP 403 `NOACCESS_SERVICE`**. Germany must be a table of states.
All three below: WCS 2.0.1, `SUBSET=x(min,max)&SUBSET=y(min,max)` easting first, `FORMAT=image/tiff`,
plain TIFF not multipart, 1 m float32.
```
NRW:  https://www.wcs.nrw.de/geobasis/wcs_nw_dgm?SERVICE=WCS&VERSION=2.0.1&REQUEST=GetCoverage&COVERAGEID=nw_dgm&SUBSET=x(346397,346867)&SUBSET=y(5685250,5685720)&FORMAT=image/tiff
BB:   https://isk.geobasis-bb.de/ows/dgm_wcs?SERVICE=WCS&VERSION=2.0.1&REQUEST=GetCoverage&COVERAGEID=bb_dgm&SUBSET=x(377789,378259)&SUBSET=y(5806167,5806637)&FORMAT=image/tiff
MV:   https://www.geodaten-mv.de/dienste/dgm_wcs?SERVICE=WCS&VERSION=2.0.1&REQUEST=GetCoverage&COVERAGEID=mv_dgm&SUBSET=x(308836,309306)&SUBSET=y(5963556,5964026)&FORMAT=image/tiff
```
NRW 200, 362,201 B, 1.6 s, EPSG:25832, nodata -9999, grid x 278000-536000 / y 5560000-5828000.
BB 200, 884,620 B, **15.5 s** (needs a long timeout), EPSG:25833, nodata -9999.
MV 200, 884,720 B, 1.8 s, EPSG:25833, **no nodata tag**; also offers `mv_dgm5`/`mv_dgm25`, use `mv_dgm`.
NRW imagery (20 cm): `https://www.wms.nrw.de/geobasis/wms_nw_dop?...&LAYERS=nw_dop_rgb&CRS=EPSG:25832&...&FORMAT=image/jpeg`, MaxWidth 5000.
Failed states from here: Bayern 404, Sachsen 403, Rheinland-Pfalz 403, Schleswig-Holstein 404, Berlin 404.
Thüringen returned caps (20,900 B) but no CoverageId I could parse — worth another look.

### Belgium (Flanders) — DHMV II, ~1 m DTM. Two real gotchas.
```
https://geo.api.vlaanderen.be/el-dtm/wcs?SERVICE=WCS&VERSION=2.0.1&REQUEST=GetCoverage&COVERAGEID=EL.GridCoverage.DTM&SUBSET=y(51.207889,51.212111)&SUBSET=x(5.326632,5.333368)&FORMAT=image/tiff&SCALESIZE=x(470),y(470)
```
200, 1,052,343 B, ~1.6 s, 470x470 float32, nodata **-9999**, multipart/related envelope.
- **An Azure WAF returns HTTP 403 whenever the query string contains a `http://` URL**, so
  `SUBSETTINGCRS=http://www.opengis.net/def/crs/EPSG/0/31370` is blocked outright. Percent-encoding does not
  help. Subset in the native CRS with no SUBSETTINGCRS.
- **The coverage is on a geographic grid, EPSG:4258, axis labels `y x` with latitude first.** Offset is
  1.3737840783946e-05 deg on both axes, so pixels are ~0.96 m E-W and ~1.53 m N-S at Flemish latitudes —
  not square in metres. Without SCALESIZE a 470 m box came back 490x307.
Flanders imagery (0.23 m) is a *different* service that does accept EPSG:31370 directly, and is
**hard-capped at 2048 px**: `https://geo.api.vlaanderen.be/OMWRGBMRVL/wms?...&LAYERS=Ortho&CRS=EPSG:31370&WIDTH=2048&HEIGHT=2048&FORMAT=image/jpeg`

### Poland — GUGiK NMT, 1 m. Use ASCII, not TIFF.
```
https://mapy.geoportal.gov.pl/wss/service/PZGIK/NMT/GRID1/WCS/DigitalTerrainModel?SERVICE=WCS&VERSION=2.0.1&REQUEST=GetCoverage&COVERAGEID=DTM_PL-EVRF2007-NH&SUBSET=x(477022,477492)&SUBSET=y(720477,720947)&SUBSETTINGCRS=http%3A%2F%2Fwww.opengis.net%2Fdef%2Fcrs%2FEPSG%2F0%2F2180&FORMAT=image/x-aaigrid
```
200, 4,686,529 B, **25.6 s**. multipart/related; skip to the `ncols` line. Header then full-decimal floats.
EPSG:2180. **`FORMAT=image/tiff` is the trap described in section 5** — it returns an 8-bit RGB picture with
correct georeferencing. 25 s and 4.7 MB of ASCII per plot is the worst cost/benefit here; warn the user.
Poland imagery: `.../PZGIK/ORTO/WMS/StandardResolution?...&LAYERS=Raster&CRS=EPSG:2180&WIDTH=2000&HEIGHT=2000&FORMAT=image/jpeg`
(2350 px returns 404 with an empty body). **Harvesting restriction — see licence table.**

### Spain — national 5 m, plus Navarra 2 m
```
https://servicios.idee.es/wcs-inspire/mdt?service=WCS&version=2.0.1&request=GetCoverage&coverageId=Elevacion25830_5&subset=x(343460,343930)&subset=y(4424686,4425156)&format=application/asc
```
**`format=image/tiff` returns int16** — 1 m vertical quantisation, a flat box came back as integers 364..372.
Unusable for jump faces. `format=application/asc` gives the same data with millimetre decimals (369.058...).
Coverages are `Elevacion<EPSG>_<res>`, res ∈ {5, 25, 200, 500, 1000} m; EPSG ∈ {25828-25831, 4258, 4326, 4083}.
```
https://idena.navarra.es/ogc/wcs?service=WCS&version=2.0.1&request=GetCoverage&coverageId=IDENA.WCS__ELEVAC_Ras_MDT_2M&subset=E(610479,610949)&subset=N(4740647,4741117)&format=image/tiff
```
Navarra: 200, 230,818 B, 2.1 s, 235x235 float32, EPSG:25830, **axis labels E/N** (`x` is rejected with
`InvalidAxisLabel`), uses **ModelTransformation 34264**, nodata **+3.4028234663852886E38 (positive float max)**.
Use `_MDT_2M`; `_MDS_2M` is the surface model.
Spain imagery (25 cm): `https://www.ign.es/wms-inspire/pnoa-ma?...&LAYERS=OI.OrthoimageCoverage&CRS=EPSG:25830&...&FORMAT=image/jpeg`, MaxWidth 4096.

**Spain's regional 1-2 m is a dead end for auto-fetch.** Catalonia's ICGC publishes MET2m but only as
pictures (WMS `image/tiff` returns RGB 8-bit, 9,178 B for 235x235 where float32 would be 220,900). Their
data WCS offers only `icc:met5`/`icc:met15`, refuses GeoTIFF ("The only one allowed is ArcGrid"), and then
returns **HTTP 200 with zero bytes** for ArcGrid. Andalucía's REDIAM and ideandalucia endpoints are dead.

---

## 7. What to wire first, and what stays manual

**Tier 1 — wire now, highest value per unit of work.**
1. **France** (0.5 m DTM + 20 cm imagery, one endpoint, one projection). The easiest big win: one host,
   one projection, both rasters over the same bbox.

   *On "best-served": Switzerland is plainly better on paper — SWISSIMAGE at 10 cm over swissALTI3D at
   0.5 m beats France's 20 cm over 0.5 m. France leads only on ease of integration, and (as of this
   report) on being the one actually wired in.*
2. **England** (1 m, big player base, one projection you then reuse for Scotland and Wales).
3. **Norway** and **Czechia** — both are ArcGIS `exportImage`, byte-identical in shape to USGS 3DEP.
   These are table rows, not code.
4. **Switzerland** (0.5 m + 10 cm) — needs the STAC lookup, but both collections share one query shape.

**Tier 2 — straightforward, each needs a little glue.**
5. **Scotland** and **Wales** — both reuse EPSG:27700; Scotland is a constructible S3 key, Wales needs one
   cheap WFS query. Both give the UK full coverage.
6. **Germany NRW** — the biggest German MX region, plain WCS, 1 m elevation + 20 cm imagery, and
   dl-de/zero means no attribution obligation at all.
7. **Belgium Flanders** — worth it for Lommel alone, but budget for the WAF and the non-square geographic grid.

**Tier 3 — works, but each has a cost.**
8. Germany Brandenburg / M-V (slow, or attribution-heavy), Poland (25 s and a format trap, plus a harvesting
   restriction on imagery), Spain (5 m elevation only — but 25 cm imagery makes it worth listing).

**Manual download only.** Denmark, Sweden and Finland all publish excellent 0.4-1 m open LiDAR but gate the
API behind a free account or key, which fails the "rider presses a button" test. Spain's regional 1-2 m
(CNIG PNOA-LiDAR, ICGC Catalonia) is download-portal only. Italy, Austria and Wallonia I did not find an
open service for. All of these are fine for the manual `place_import_dem` path, which works identically
once the file is on disk.

**Not worth pursuing.** Germany's national BKG DGM1 (403, access-gated), Catalonia's MET2m (picture only).

---

## 8. Method notes and caveats

- Every "works" row was fetched with a real bbox and the returned raster inspected: dimensions, pixel scale,
  tiepoint or ModelTransformation, bit depth, nodata tag, and the actual min/max elevations. Where a number
  looked suspicious I cross-checked against known terrain (Cheddar Gorge for England's relief).
- Imagery marked "viewed" I actually looked at, not just byte-counted. France, NRW, Flanders and Poland
  were viewed; Switzerland and Spain were measured only.
- This machine's DNS resolver failed intermittently throughout, returning connect failures for hosts that
  had worked minutes earlier while raw IPs stayed reachable. All later requests were made by resolving
  through DNS-over-HTTPS first. **If a probe suddenly fails on a host that worked before, retry before
  concluding the service is dead** — I nearly wrote off several working services this way.
- The two unverified licence rows (Czechia, Norway) are the only known gaps in section 3.
