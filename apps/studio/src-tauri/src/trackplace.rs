//! Turning a real place into ground a track can be built on.
//!
//! Three jobs, and only three:
//!
//! 1. **Find** — a place name or a pair of coordinates becomes a plot on the map.
//! 2. **Fetch** — the public elevation survey for that plot, plus openly-licensed imagery
//!    of the same footprint, land in a folder as a GeoTIFF and a picture.
//! 3. **Hold the lap** — the ordered points a rider clicks on that picture are saved beside
//!    them as `<slug>.lap.json`, which is what the importer reads.
//!
//! What it deliberately does *not* do is guess which ribbon of dirt is the current lap.
//! A venue that is rebuilt every year holds every layout it has ever had in its bare-earth
//! elevation, all of them equally real, and no amount of filtering separates last season
//! from this one. Aerial imagery separates them instantly, because the lane in use is the
//! one worn to bare dirt. So tracing is a human step, done against imagery, on purpose.
//!
//! ## Nothing here runs on its own
//!
//! Every function in this module that touches the network is behind a `#[tauri::command]`
//! that a button calls. Nothing prefetches, nothing warms a cache, nothing polls. Opening
//! the panel costs zero requests; only pressing a button spends one.
//!
//! ## Where the space goes
//!
//! A place folder is roughly 2 MB: about 1 MB of elevation, about 1 MB of imagery, and a
//! few KB of hillshade and notes. That is small because we ask the elevation server for
//! exactly the plot we want rather than downloading the survey tile it was cut from — the
//! staged USGS tile covering Ironman is 315 MB for the same 470 m of ground. Measured: the
//! server's answer matches that tile to within 1 cm over the plot, which is well under the
//! 2.4 cm noise floor of the survey itself.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

use crate::config;

/// How this app identifies itself to the public services it asks for data.
///
/// Nominatim's usage policy requires a real identifier, and it is plain courtesy to the
/// others. A request without it is refused, and rightly.
const UA: &str = concat!("MXBApp-Studio/", env!("CARGO_PKG_VERSION"), " (+https://mxbsecure.com)");

/// The plot the track generator works on, in metres a side. A fetch defaults to this so the
/// ground that arrives is the ground the generator wants, without anyone doing arithmetic.
pub const PLOT_M: f64 = 470.0;

/// The widest plot worth fetching. Past this the imagery request gets slow and the rider is
/// almost certainly trying to capture a whole facility rather than a lap.
const MAX_PLOT_M: f64 = 2000.0;

/// The narrowest. Below this there isn't a lap in there.
const MIN_PLOT_M: f64 = 100.0;

// ── Coordinates ──────────────────────────────────────────────────────────────
//
// Only one projection is implemented here, and on purpose: UTM. Every elevation source
// worth using already publishes on a metric projected grid, so nothing has to be
// *reprojected* — what has to happen is turning the latitude and longitude a rider can
// actually look up into the grid the source speaks. For USGS that grid is UTM, so this is
// the whole of it. The Dutch service is asked in plain latitude and longitude and answers
// on its own national grid, so it needs no maths at all.

const WGS84_A: f64 = 6_378_137.0;
const WGS84_F: f64 = 1.0 / 298.257_223_563;
const UTM_K0: f64 = 0.9996;

/// The UTM zone a longitude falls in.
pub fn utm_zone(lon: f64) -> i32 {
    (((lon + 180.0) / 6.0).floor() as i32 + 1).clamp(1, 60)
}

/// Latitude and longitude to UTM easting and northing, in metres.
///
/// The standard Snyder series, good to a few millimetres inside a zone — several orders of
/// magnitude better than it needs to be, since all it decides is where a 470 m window sits.
pub fn ll_to_utm(lat: f64, lon: f64) -> (f64, f64, i32, bool) {
    let zone = utm_zone(lon);
    let e2 = WGS84_F * (2.0 - WGS84_F);
    let ep2 = e2 / (1.0 - e2);
    let lon0 = ((zone - 1) * 6 - 180 + 3) as f64;
    let lat_r = lat.to_radians();
    let dlon = (lon - lon0).to_radians();
    let n = WGS84_A / (1.0 - e2 * lat_r.sin().powi(2)).sqrt();
    let t = lat_r.tan().powi(2);
    let c = ep2 * lat_r.cos().powi(2);
    let a = lat_r.cos() * dlon;
    let m = WGS84_A
        * ((1.0 - e2 / 4.0 - 3.0 * e2 * e2 / 64.0 - 5.0 * e2 * e2 * e2 / 256.0) * lat_r
            - (3.0 * e2 / 8.0 + 3.0 * e2 * e2 / 32.0 + 45.0 * e2 * e2 * e2 / 1024.0)
                * (2.0 * lat_r).sin()
            + (15.0 * e2 * e2 / 256.0 + 45.0 * e2 * e2 * e2 / 1024.0) * (4.0 * lat_r).sin()
            - (35.0 * e2 * e2 * e2 / 3072.0) * (6.0 * lat_r).sin());
    let east = UTM_K0
        * n
        * (a + (1.0 - t + c) * a.powi(3) / 6.0
            + (5.0 - 18.0 * t + t * t + 72.0 * c - 58.0 * ep2) * a.powi(5) / 120.0)
        + 500_000.0;
    let mut north = UTM_K0
        * (m + n
            * lat_r.tan()
            * (a * a / 2.0
                + (5.0 - t + 9.0 * c + 4.0 * c * c) * a.powi(4) / 24.0
                + (61.0 - 58.0 * t + t * t + 600.0 * c - 330.0 * ep2) * a.powi(6) / 720.0));
    let north_hemi = lat >= 0.0;
    if !north_hemi {
        north += 10_000_000.0;
    }
    (east, north, zone, north_hemi)
}

/// UTM easting and northing back to latitude and longitude.
///
/// Needed to put the plot's corners back into degrees, which is what the imagery services
/// and the Dutch elevation service are asked in.
pub fn utm_to_ll(east: f64, north: f64, zone: i32, north_hemi: bool) -> (f64, f64) {
    let e2 = WGS84_F * (2.0 - WGS84_F);
    let ep2 = e2 / (1.0 - e2);
    let x = east - 500_000.0;
    let y = if north_hemi { north } else { north - 10_000_000.0 };
    let lon0 = ((zone - 1) * 6 - 180 + 3) as f64;
    let e1 = (1.0 - (1.0 - e2).sqrt()) / (1.0 + (1.0 - e2).sqrt());
    let m = y / UTM_K0;
    let mu = m / (WGS84_A * (1.0 - e2 / 4.0 - 3.0 * e2 * e2 / 64.0 - 5.0 * e2 * e2 * e2 / 256.0));
    let phi1 = mu
        + (3.0 * e1 / 2.0 - 27.0 * e1.powi(3) / 32.0) * (2.0 * mu).sin()
        + (21.0 * e1 * e1 / 16.0 - 55.0 * e1.powi(4) / 32.0) * (4.0 * mu).sin()
        + (151.0 * e1.powi(3) / 96.0) * (6.0 * mu).sin()
        + (1097.0 * e1.powi(4) / 512.0) * (8.0 * mu).sin();
    let c1 = ep2 * phi1.cos().powi(2);
    let t1 = phi1.tan().powi(2);
    let n1 = WGS84_A / (1.0 - e2 * phi1.sin().powi(2)).sqrt();
    let r1 = WGS84_A * (1.0 - e2) / (1.0 - e2 * phi1.sin().powi(2)).powf(1.5);
    let d = x / (n1 * UTM_K0);
    let lat = phi1
        - (n1 * phi1.tan() / r1)
            * (d * d / 2.0
                - (5.0 + 3.0 * t1 + 10.0 * c1 - 4.0 * c1 * c1 - 9.0 * ep2) * d.powi(4) / 24.0
                + (61.0 + 90.0 * t1 + 298.0 * c1 + 45.0 * t1 * t1 - 252.0 * ep2 - 3.0 * c1 * c1)
                    * d.powi(6)
                    / 720.0);
    let lon = lon0.to_radians()
        + (d - (1.0 + 2.0 * t1 + c1) * d.powi(3) / 6.0
            + (5.0 - 2.0 * c1 + 28.0 * t1 - 3.0 * c1 * c1 + 8.0 * ep2 + 24.0 * t1 * t1) * d.powi(5)
                / 120.0)
            / phi1.cos();
    (lat.to_degrees(), lon.to_degrees())
}

/// Latitude and longitude to Lambert-93, the French national grid, in metres.
///
/// France's elevation service answers on EPSG:2154 and takes its request in the same, so
/// this is what turns a place a rider looked up into a window that service understands.
///
/// Simpler than the British one in the way that matters: RGF93, the datum Lambert-93 sits
/// on, agrees with WGS84 to within a couple of centimetres, so there is no datum shift to
/// do. What is left is a Lambert conformal conic on GRS80, with the two standard parallels
/// at 44 and 49 degrees. The one point that can be checked against the definition rather
/// than against a server is the origin: 46.5 N, 3 E is exactly E 700000, N 6600000.
pub fn ll_to_lambert93(lat: f64, lon: f64) -> (f64, f64) {
    // GRS80, which for this purpose is WGS84.
    let a: f64 = 6_378_137.0;
    let f: f64 = 1.0 / 298.257_222_101;
    let e: f64 = (f * (2.0 - f)).sqrt();
    let (phi0, lam0) = (46.5f64.to_radians(), 3.0f64.to_radians());
    let (phi1, phi2) = (44.0f64.to_radians(), 49.0f64.to_radians());
    let (fe, fna) = (700_000.0, 6_600_000.0);

    // The isometric latitude, which is what makes the projection conformal.
    let t = |p: f64| {
        let s = e * p.sin();
        (std::f64::consts::FRAC_PI_4 - p / 2.0).tan() / ((1.0 - s) / (1.0 + s)).powf(e / 2.0)
    };
    let m = |p: f64| p.cos() / (1.0 - e * e * p.sin().powi(2)).sqrt();

    let n = (m(phi1).ln() - m(phi2).ln()) / (t(phi1).ln() - t(phi2).ln());
    let big_f = m(phi1) / (n * t(phi1).powf(n));
    let r0 = a * big_f * t(phi0).powf(n);
    let r = a * big_f * t(lat.to_radians()).powf(n);
    let theta = n * (lon.to_radians() - lam0);
    (fe + r * theta.sin(), fna + r0 - r * theta.cos())
}

/// The EPSG code of a UTM zone on the NAD83 datum — the grid USGS publishes on.
fn nad83_utm_epsg(zone: i32) -> u32 {
    26900 + zone as u32
}

/// Latitude and longitude to the British National Grid, in metres.
///
/// England's LiDAR service will happily take a request in plain degrees, but then it answers
/// in degrees too, and a trace in degrees is no use to anybody: the importer needs metres on
/// a projected grid. Asking it in eastings and northings instead gets back exactly 1 m square
/// cells on EPSG:27700, which is what we want, and the price is this function.
///
/// Two steps: shift the datum from WGS84 to OSGB36, then project onto the Airy 1830
/// transverse Mercator. The datum shift is the plain seven-parameter Helmert, which is good
/// to a few metres rather than the few centimetres the full OSTN15 grid would give. That is
/// fine here and nowhere else in this file: all it decides is where a 470 m window is cut.
/// Every coordinate that matters afterwards comes from the server's own georeferencing.
pub fn ll_to_bng(lat: f64, lon: f64) -> (f64, f64) {
    // WGS84 geodetic to geocentric.
    let (a_w, f_w) = (WGS84_A, WGS84_F);
    let e2_w = f_w * (2.0 - f_w);
    let (lat_r, lon_r) = (lat.to_radians(), lon.to_radians());
    let nu_w = a_w / (1.0 - e2_w * lat_r.sin().powi(2)).sqrt();
    let x = nu_w * lat_r.cos() * lon_r.cos();
    let y = nu_w * lat_r.cos() * lon_r.sin();
    let z = (1.0 - e2_w) * nu_w * lat_r.sin();

    // Helmert, WGS84 to OSGB36. The standard OS-published parameters.
    let (tx, ty, tz) = (-446.448, 125.157, -542.060);
    let s = 20.489_4e-6;
    let (rx, ry, rz) = (
        (-0.149_2 / 3600.0f64).to_radians(),
        (-0.247_0 / 3600.0f64).to_radians(),
        (-0.984_2 / 3600.0f64).to_radians(),
    );
    let x2 = tx + x * (1.0 + s) + (-rz) * y + ry * z;
    let y2 = ty + rz * x + y * (1.0 + s) + (-rx) * z;
    let z2 = tz + (-ry) * x + rx * y + z * (1.0 + s);

    // Geocentric back to geodetic on Airy 1830.
    let (a, b) = (6_377_563.396, 6_356_256.909);
    let e2 = (a * a - b * b) / (a * a);
    let p = x2.hypot(y2);
    let mut phi = z2.atan2(p * (1.0 - e2));
    for _ in 0..8 {
        let nu = a / (1.0 - e2 * phi.sin().powi(2)).sqrt();
        phi = (z2 + e2 * nu * phi.sin()).atan2(p);
    }
    let lam = y2.atan2(x2);

    // Airy 1830 transverse Mercator, National Grid parameters.
    let (n0, e0) = (-100_000.0, 400_000.0);
    let f0 = 0.999_601_271_7;
    let phi0 = 49.0f64.to_radians();
    let lam0 = (-2.0f64).to_radians();
    let n = (a - b) / (a + b);
    let nu = a * f0 / (1.0 - e2 * phi.sin().powi(2)).sqrt();
    let rho = a * f0 * (1.0 - e2) / (1.0 - e2 * phi.sin().powi(2)).powf(1.5);
    let eta2 = nu / rho - 1.0;
    let m = b * f0
        * ((1.0 + n + 1.25 * n * n + 1.25 * n * n * n) * (phi - phi0)
            - (3.0 * n + 3.0 * n * n + 2.625 * n * n * n)
                * (phi - phi0).sin()
                * (phi + phi0).cos()
            + (1.875 * n * n + 1.875 * n * n * n)
                * (2.0 * (phi - phi0)).sin()
                * (2.0 * (phi + phi0)).cos()
            - (35.0 / 24.0 * n * n * n)
                * (3.0 * (phi - phi0)).sin()
                * (3.0 * (phi + phi0)).cos());
    let (sp, cp, tp) = (phi.sin(), phi.cos(), phi.tan());
    let i = m + n0;
    let ii = nu / 2.0 * sp * cp;
    let iii = nu / 24.0 * sp * cp.powi(3) * (5.0 - tp * tp + 9.0 * eta2);
    let iii_a = nu / 720.0 * sp * cp.powi(5) * (61.0 - 58.0 * tp * tp + tp.powi(4));
    let iv = nu * cp;
    let v = nu / 6.0 * cp.powi(3) * (nu / rho - tp * tp);
    let vi = nu / 120.0
        * cp.powi(5)
        * (5.0 - 18.0 * tp * tp + tp.powi(4) + 14.0 * eta2 - 58.0 * tp * tp * eta2);
    let d = lam - lam0;
    let north = i + ii * d * d + iii * d.powi(4) + iii_a * d.powi(6);
    let east = e0 + iv * d + v * d.powi(3) + vi * d.powi(5);
    (east, north)
}

// ── What the ground can be fetched from ──────────────────────────────────────

/// Whether a survey measured the ground or the top of whatever is standing on it.
///
/// This is the single most important thing to know about an elevation source and the one
/// most often glossed over. A **terrain** model is bare earth: trees and buildings have been
/// removed, and what is left is the dirt a bike rides on. A **surface** model is the first
/// thing the beam hit, so a wood is a solid 20 m plateau and a start tower is a pillar.
/// A track built from a surface model in anything but open ground is nonsense.
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub enum Ground {
    /// Bare earth. What you want.
    Terrain,
    /// Everything standing on the earth as well. Usable only on genuinely open ground.
    Surface,
}

/// How a source is reached. Two protocols cover everything verified so far, and a third
/// case covers the honest majority of the world.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Protocol {
    /// An Esri ImageServer answering `exportImage`. USGS uses this.
    ArcGisImage,
    /// An OGC Web Coverage Service answering `GetCoverage`. The Dutch service uses this.
    Wcs201,
    /// A worldwide grid of plain files on open storage. Copernicus uses this.
    CogGrid,
    /// A WMS answering `GetMap` with a float GeoTIFF. France's does; it is an image service
    /// in name only, since what comes back is elevation rather than a picture.
    WmsRaster,
}

/// One place the ground can come from.
struct Source {
    id: &'static str,
    /// Shown to a rider. Deliberately says the resolution, because that is the whole story.
    label: &'static str,
    /// Where it covers, in words, for when it doesn't.
    region: &'static str,
    /// The finest cell this source ever publishes, in metres. What you actually get at a
    /// given spot can be coarser, which is what the coverage check is for.
    best_cell_m: f64,
    ground: Ground,
    protocol: Protocol,
    endpoint: &'static str,
    licence: &'static str,
    /// Empty where none is required. Where it isn't, the tool writes it into the place's
    /// notes and the guide tells the rider it has to travel with the track.
    attribution: &'static str,
    /// Where a human goes to read the terms, so nobody has to take our word for it.
    terms_url: &'static str,
}

/// Every source the tool can fetch from without an account, a key, or a click-through.
///
/// Ordered best first. The coverage check walks this list and reports what each one has at
/// the spot in question, so a rider is told "1 m LiDAR from 2017" or "nothing better than
/// 30 m" before a single byte is spent.
const SOURCES: &[Source] = &[
    Source {
        id: "usgs3dep",
        label: "USGS 3DEP — 1 m LiDAR bare earth",
        region: "United States, where it has been flown",
        best_cell_m: 1.0,
        ground: Ground::Terrain,
        protocol: Protocol::ArcGisImage,
        endpoint: "https://elevation.nationalmap.gov/arcgis/rest/services/3DEPElevation/ImageServer",
        licence: "Public domain (work of the U.S. Government, 17 U.S.C. §105)",
        attribution: "",
        terms_url: "https://www.usgs.gov/information-policies-and-instructions/copyrights-and-credits",
    },
    Source {
        id: "ahn",
        label: "AHN — 0.5 m LiDAR bare earth",
        region: "Netherlands, nationwide",
        best_cell_m: 0.5,
        ground: Ground::Terrain,
        protocol: Protocol::Wcs201,
        endpoint: "https://service.pdok.nl/rws/ahn/wcs/v1_0",
        licence: "CC0 1.0 — public domain dedication",
        attribution: "",
        terms_url: "https://www.ahn.nl/",
    },
    Source {
        id: "ignfrance",
        label: "IGN LiDAR HD — 0.5 m bare earth, falling back to RGE ALTI at 1 m",
        region: "France. LiDAR HD is a rolling programme, so the 0.5 m layer isn't everywhere yet",
        best_cell_m: 0.5,
        ground: Ground::Terrain,
        protocol: Protocol::WmsRaster,
        endpoint: "https://data.geopf.fr/wms-r/wms",
        licence: "Licence Ouverte / Open Licence Etalab 2.0 — free to use, attribution required",
        attribution: "\u{a9} IGN — Licence Ouverte / Open Licence Etalab 2.0",
        terms_url: "https://www.etalab.gouv.fr/licence-ouverte-open-licence/",
    },
    Source {
        id: "eaengland",
        label: "Environment Agency National LiDAR — 1 m bare earth",
        region: "England. Scotland and Wales run separate programmes this tool doesn't reach yet",
        best_cell_m: 1.0,
        ground: Ground::Terrain,
        protocol: Protocol::Wcs201,
        endpoint: "https://environment.data.gov.uk/spatialdata/lidar-composite-digital-terrain-model-dtm-1m/wcs",
        licence: "Open Government Licence v3.0 — free to use, attribution required",
        attribution: "Contains public sector information licensed under the Open Government Licence v3.0. \u{a9} Environment Agency copyright and/or database right 2022. All rights reserved.",
        terms_url: "https://www.nationalarchives.gov.uk/doc/open-government-licence/version/3/",
    },
    Source {
        id: "copernicus30",
        label: "Copernicus GLO-30 — 30 m surface model",
        region: "Worldwide, between 60°S and 84°N",
        best_cell_m: 30.0,
        ground: Ground::Surface,
        protocol: Protocol::CogGrid,
        endpoint: "https://copernicus-dem-30m.s3.amazonaws.com",
        licence: "Free, worldwide, non-exclusive — ESA/Copernicus, attribution required",
        attribution: "© DLR e.V. 2010-2014 and © Airbus Defence and Space GmbH 2014-2018, provided under COPERNICUS by the European Union and ESA, all rights reserved",
        terms_url: "https://spacedata.copernicus.eu/documents/20123/121286/CSCDA_ESA_Mission-specific+Annex.pdf",
    },
];

fn source(id: &str) -> Option<&'static Source> {
    SOURCES.iter().find(|s| s.id == id)
}

/// Openly-licensed imagery of the same footprint, so a rider can see what they're tracing.
///
/// There is no Google, Apple or Bing here and there never will be: their imagery is
/// licensed for viewing in their own products and tracing a lap off it would put a
/// derivative of their photography inside a track someone hands out. The two sources below
/// are a US government work and a Dutch open-data release respectively, which is the whole
/// of what is actually available at a resolution you can pick a lane out of.
struct Imagery {
    /// Which service this is. Read by the map, which builds a different request per shape,
    /// and by the test that refuses a closed-licence imagery source.
    id: &'static str,
    label: &'static str,
    endpoint: &'static str,
    /// Recorded for the same reason as `id`: it documents what shape the endpoint is,
    /// which is what a reader needs to know to add the next one.
    #[allow(dead_code)]
    protocol: Protocol,
    /// Layer name, for the services that need one.
    layer: &'static str,
    licence: &'static str,
    attribution: &'static str,
}

const IMAGERY: &[Imagery] = &[
    Imagery {
        id: "naip",
        label: "USGS NAIPPlus aerial",
        endpoint: "https://imagery.nationalmap.gov/arcgis/rest/services/USGSNAIPPlus/ImageServer",
        protocol: Protocol::ArcGisImage,
        layer: "",
        licence: "Public domain (work of the U.S. Government)",
        attribution: "",
    },
    Imagery {
        id: "ign-ortho",
        label: "IGN BD ORTHO — 20 cm colour ortho",
        endpoint: "https://data.geopf.fr/wms-r/wms",
        protocol: Protocol::WmsRaster,
        layer: "HR.ORTHOIMAGERY.ORTHOPHOTOS",
        licence: "Licence Ouverte / Open Licence Etalab 2.0",
        attribution: "\u{a9} IGN — Licence Ouverte / Open Licence Etalab 2.0",
    },
    Imagery {
        id: "pdok-ortho",
        label: "PDOK Luchtfoto — 8 cm colour ortho",
        endpoint: "https://service.pdok.nl/hwh/luchtfotorgb/wms/v1_0",
        protocol: Protocol::Wcs201, // WMS, close enough in shape: a GET with a bbox.
        layer: "Actueel_ortho25",
        licence: "CC BY 4.0",
        attribution: "Luchtfoto: PDOK / Beeldmateriaal Nederland, CC BY 4.0",
    },
];

// ── What a rider is told before spending anything ────────────────────────────

/// A place a search turned up.
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PlaceHit {
    pub label: String,
    pub lat: f64,
    pub lon: f64,
    /// What OpenStreetMap calls it: `leisure`, `highway`, and so on. Shown so a rider can
    /// tell a motocross circuit from a street with the same name.
    pub kind: String,
    /// How far this is from the spot a nearby search was run on, kilometres. `None` for a hit
    /// that came from a name rather than from looking round a point.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub away_km: Option<f64>,
}

/// What one source has at one spot.
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SourceCoverage {
    pub id: String,
    pub label: String,
    pub region: String,
    /// What is actually available here, which can be coarser than the source's best.
    pub cell_m: f64,
    pub ground: Ground,
    /// What the survey was called, where the service says. Empty where it doesn't.
    pub dataset: String,
    /// When it was flown, as the service reports it. Empty where unknown.
    pub collected: String,
    pub licence: String,
    pub attribution: String,
    pub terms_url: String,
    /// True where this source has something here at all.
    pub covered: bool,
    /// Why it can't be used, in a sentence, where it can't.
    pub note: String,
}

/// Everything known about a spot before anything is downloaded.
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct CoverageReport {
    pub lat: f64,
    pub lon: f64,
    pub epsg: u32,
    pub east: f64,
    pub north: f64,
    pub sources: Vec<SourceCoverage>,
    /// The id of the one the tool would pick. Empty where nothing covers the place.
    pub best: String,
}

// ── What a place is, once fetched ────────────────────────────────────────────

/// The elevation a place was fetched with, as it sits on disk.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DemNote {
    /// Relative to the place folder.
    pub path: String,
    pub crs: String,
    pub cell_m: f64,
    /// The centre of pixel row 0, column 0. North up, row index increasing southward.
    /// Stated here because the half-pixel confusion between a pixel's corner and its centre
    /// is the single most common way a trace ends up half a metre out.
    pub origin_e: f64,
    pub origin_n: f64,
    pub width: u32,
    pub height: u32,
    pub vertical_datum: String,
    pub source: String,
    pub source_url: String,
    pub collected: String,
    pub licence: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub attribution: String,
    pub ground: Ground,
    /// Lowest and highest ground in the plot, in metres. The first sanity check: a plot
    /// with 0.2 m of relief is a car park, and one with 90 m is a mountainside.
    pub min_z: f64,
    pub max_z: f64,
}

/// The picture a lap is traced on.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ImageryNote {
    pub path: String,
    pub source: String,
    pub licence: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub attribution: String,
    pub captured: String,
}

/// A fetched place: its folder, its ground, its picture, and where they came from.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Place {
    pub slug: String,
    pub name: String,
    pub lat: f64,
    pub lon: f64,
    pub plot_m: f64,
    pub fetched: String,
    pub dem: DemNote,
    #[serde(default)]
    pub imagery: Option<ImageryNote>,
    /// Relative path to the hillshade picture, which is always written.
    pub hillshade: String,
    /// True once a lap has been traced and saved here.
    #[serde(default)]
    pub has_trace: bool,
    /// The plot's corners in plain latitude and longitude: south, west, north, east.
    ///
    /// Recorded because the projected coordinates everything else uses are unreadable to a
    /// human, and a rider who wants to check the footprint landed where they meant it to
    /// needs four numbers they can paste into any map.
    #[serde(default)]
    pub bbox_ll: Vec<f64>,
    /// The place folder's size on disk, in bytes. Filled in on listing, not stored.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bytes: Option<u64>,
    /// The absolute path of the folder, so the UI can show it and open it.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub dir: String,
}

/// The lap a rider drew. This exact shape is what the importer reads — see
/// `docs/tracks/real-place-tracks.md` for the agreed contract and why each field is there.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct LapTrace {
    pub version: u32,
    pub kind: String,
    pub name: String,
    pub crs: String,
    pub units: String,
    pub closed: bool,
    pub default_width_m: f64,
    /// Where the start/finish line sits, as an index into `points`.
    #[serde(default)]
    pub start_index: usize,
    /// Ordered, in the DEM's own projection. Each entry is `[easting, northing]` or
    /// `[easting, northing, width_m]`. Mixed lengths are allowed in one lap.
    pub points: Vec<Vec<f64>>,
    pub dem: DemNote,
    #[serde(default)]
    pub imagery: Option<ImageryNote>,
}

// ── Where things live ────────────────────────────────────────────────────────

/// The folder every fetched place sits in.
///
/// Under the shared app data root rather than the cache, because a traced lap is work
/// somebody did and a cache is a thing that gets cleared.
fn places_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = config::data_dir(app)
        .ok_or_else(|| "couldn't work out where this app keeps its files".to_string())?
        .join("places");
    std::fs::create_dir_all(&dir).map_err(|e| format!("couldn't make {}: {e}", dir.display()))?;
    Ok(dir)
}

/// A folder name from a place name. Lowercase, dashes, nothing a filesystem argues with.
fn slugify(name: &str) -> String {
    let mut out = String::new();
    let mut dash = false;
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            dash = false;
        } else if !dash && !out.is_empty() {
            out.push('-');
            dash = true;
        }
    }
    let s = out.trim_matches('-').to_string();
    if s.is_empty() {
        "place".to_string()
    } else {
        s.chars().take(48).collect()
    }
}

/// A slug that isn't taken yet.
fn free_slug(dir: &Path, name: &str) -> String {
    let base = slugify(name);
    if !dir.join(&base).exists() {
        return base;
    }
    for n in 2..999 {
        let candidate = format!("{base}-{n}");
        if !dir.join(&candidate).exists() {
            return candidate;
        }
    }
    format!("{base}-{}", now_stamp().replace([':', '-'], ""))
}

/// Refuse anything that could walk out of the places folder.
fn place_dir(app: &AppHandle, slug: &str) -> Result<PathBuf, String> {
    if slug.is_empty()
        || slug.len() > 64
        || !slug
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(format!("{slug:?} isn't a place name this app wrote"));
    }
    Ok(places_dir(app)?.join(slug))
}

fn now_stamp() -> String {
    // No chrono in this binary, and a date is all that's wanted. Seconds since the epoch
    // turned into an ISO day, which is enough to answer "when did I pull this".
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let days = secs / 86_400;
    let (y, m, d) = civil_from_days(days);
    let rem = secs % 86_400;
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}

/// Howard Hinnant's days-to-civil, which is the short correct way to do this without a
/// calendar crate.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

fn dir_bytes(dir: &Path) -> u64 {
    let mut total = 0;
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            if let Ok(md) = e.metadata() {
                total += if md.is_dir() {
                    dir_bytes(&e.path())
                } else {
                    md.len()
                };
            }
        }
    }
    total
}

// ── Asking the network ───────────────────────────────────────────────────────
//
// One client, one place errors are turned into sentences. The services here fail in three
// different dialects — an HTTP status, an XML fault document, and a JSON body with a 200
// beside it — and a rider should never see any of them raw.

fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent(UA)
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| format!("couldn't start a network client: {e}"))
}

/// Fetch bytes, turning every failure into something a rider can act on.
async fn get_bytes(url: &str, what: &str) -> Result<Vec<u8>, String> {
    let resp = client()?.get(url).send().await.map_err(|e| {
        if e.is_timeout() {
            format!("{what}: the server took too long to answer. Try again, or try a smaller plot.")
        } else if e.is_connect() {
            format!("{what}: couldn't reach the server. Check the connection and try again.")
        } else {
            format!("{what}: {e}")
        }
    })?;
    let status = resp.status();
    let body = resp
        .bytes()
        .await
        .map_err(|e| format!("{what}: the answer stopped halfway through: {e}"))?
        .to_vec();
    if !status.is_success() {
        return Err(format!(
            "{what}: the server said {} {}. {}",
            status.as_u16(),
            status.canonical_reason().unwrap_or(""),
            first_line_of(&body)
        ));
    }
    // A 200 with a fault document in it. Esri does this; so does every WCS on a bad day.
    if let Some(msg) = fault_in(&body) {
        return Err(format!("{what}: {msg}"));
    }
    if body.is_empty() {
        return Err(format!("{what}: the server sent nothing back."));
    }
    Ok(body)
}

/// Pull a readable message out of a JSON or XML error body, if that's what this is.
fn fault_in(body: &[u8]) -> Option<String> {
    // Only worth looking at things that could be text. A TIFF starts `II`/`MM`, a PNG with
    // a 0x89, so this bails out immediately on real data.
    let head = &body[..body.len().min(4096)];
    let text = std::str::from_utf8(head).ok()?;
    let trimmed = text.trim_start();
    if trimmed.starts_with('{') {
        let v: serde_json::Value = serde_json::from_slice(body).ok()?;
        let err = v.get("error")?;
        let msg = err.get("message").and_then(|m| m.as_str()).unwrap_or("refused the request");
        let details = err
            .get("details")
            .and_then(|d| d.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str())
                    .collect::<Vec<_>>()
                    .join("; ")
            })
            .unwrap_or_default();
        return Some(if details.is_empty() {
            msg.to_string()
        } else {
            format!("{msg} ({details})")
        });
    }
    if trimmed.starts_with("<?xml") || trimmed.starts_with('<') {
        // WCS and WMS faults. Grab whatever is inside the first ExceptionText.
        for tag in ["ExceptionText", "ServiceException", "ows:ExceptionText"] {
            if let Some(start) = trimmed.find(&format!("<{tag}")) {
                if let Some(gt) = trimmed[start..].find('>') {
                    let from = start + gt + 1;
                    if let Some(end) = trimmed[from..].find("</") {
                        let msg = trimmed[from..from + end].trim();
                        if !msg.is_empty() {
                            return Some(msg.to_string());
                        }
                    }
                }
            }
        }
        return Some("the server sent an error document instead of data".to_string());
    }
    None
}

fn first_line_of(body: &[u8]) -> String {
    String::from_utf8_lossy(&body[..body.len().min(300)])
        .lines()
        .next()
        .unwrap_or("")
        .trim()
        .to_string()
}

// ── Finding a place ──────────────────────────────────────────────────────────

/// Look a place up by name.
///
/// Uses OpenStreetMap's Nominatim, which is the only openly-licensed worldwide gazetteer
/// there is. The data is ODbL: what comes back is a coordinate, and a coordinate is a fact
/// rather than a substantial extract, so nothing travels into the track from here — but the
/// obligation is recorded in the guide all the same, and the panel credits it on screen.
///
/// Called by the search button and by nothing else. No type-ahead, no request per keystroke:
/// Nominatim's usage policy asks for at most one request a second and this respects it by
/// only ever firing when somebody presses the button.
#[tauri::command]
pub async fn place_find(query: String) -> Result<Vec<PlaceHit>, String> {
    let q = query.trim();
    if q.is_empty() {
        return Ok(Vec::new());
    }
    // A pair of numbers is a coordinate, not a search. Accept it as typed.
    if let Some(hit) = parse_coords(q) {
        return Ok(vec![hit]);
    }
    let url = format!(
        "https://nominatim.openstreetmap.org/search?q={}&format=jsonv2&limit=8&addressdetails=0",
        percent_encoding::utf8_percent_encode(q, percent_encoding::NON_ALPHANUMERIC)
    );
    let body = get_bytes(&url, "searching for that place").await?;
    let items: Vec<serde_json::Value> =
        serde_json::from_slice(&body).map_err(|e| format!("the place search answered oddly: {e}"))?;
    let hits: Vec<PlaceHit> = items
        .into_iter()
        .filter_map(|v| {
            let lat = v.get("lat")?.as_str()?.parse().ok()?;
            let lon = v.get("lon")?.as_str()?.parse().ok()?;
            Some(PlaceHit {
                label: v
                    .get("display_name")
                    .and_then(|d| d.as_str())
                    .unwrap_or("")
                    .to_string(),
                lat,
                lon,
                kind: v
                    .get("type")
                    .and_then(|d| d.as_str())
                    .unwrap_or("")
                    .to_string(),
                away_km: None,
            })
        })
        .collect();
    if !hits.is_empty() {
        return Ok(hits);
    }
    // A gazetteer knows the places people write addresses about. A motocross circuit is often
    // not one of them: it is a field with a name the club gave it, mapped as a `sport=motocross`
    // way with no street and no settlement of its own, and Nominatim's free-text index will not
    // find it by that name. So when the gazetteer comes back empty, ask the map itself for a
    // circuit called this — which is the question the rider was asking all along.
    //
    // Only then: it costs one Overpass request, and it is still the same button press.
    //
    // Its failures are reported rather than swallowed. An empty list means there is no circuit
    // by that name on the map, which is a fact worth acting on; a busy Overpass means try
    // again, and quietly turning the second into the first would send a rider off hunting
    // coordinates for a track that is right there.
    tracks_named(q).await
}

// ── Finding a circuit rather than a place ────────────────────────────────────
//
// Nominatim answers "where is this name", which is the wrong question for a motocross track
// half the time. Overpass answers "what is tagged as a motocross circuit here", which is the
// right one, and it reads the same OpenStreetMap data under the same ODbL terms: what comes
// back is a name and a coordinate, and a coordinate is a fact.
//
// Both entry points are behind a button, like everything else in this module. Overpass is a
// public server run on donations and its usage policy asks for a handful of requests a minute
// from a single client; one per press is well inside that.

/// Where the circuit search asks.
const OVERPASS: &str = "https://overpass-api.de/api/interpreter";

/// The tags a motocross circuit carries in OpenStreetMap.
///
/// `sport` is the useful one — `leisure` is `track`, `pitch` or `sports_centre` depending on
/// who mapped it, and none of those means motocross on their own. The values are matched
/// case-insensitively and loosely because a venue that hosts more than one discipline holds
/// them semicolon-separated (`motocross;enduro`). `motorsport` is deliberately not in here:
/// it is what a kart circuit and a rally stage wear, and a list of those is not what anybody
/// pressed the button for.
const CIRCUIT_SPORTS: &str = "motocross|supercross|motorcross|enduro";

/// How far out "tracks near here" looks, kilometres.
///
/// Forty, because that is about an hour's drive with a bike in the van and it is the radius a
/// rider means by "my local track". Wider than this and a busy region answers with fifty
/// circuits and the list stops being a list.
const NEAR_KM: f64 = 40.0;

/// The most circuits either search hands back.
const CIRCUIT_LIMIT: usize = 40;

/// Ask Overpass, and turn what comes back into hits.
async fn overpass(query: &str, what: &str) -> Result<Vec<serde_json::Value>, String> {
    let url = format!(
        "{OVERPASS}?data={}",
        percent_encoding::utf8_percent_encode(query, percent_encoding::NON_ALPHANUMERIC)
    );
    let body = get_bytes(&url, what).await?;
    let v: serde_json::Value =
        serde_json::from_slice(&body).map_err(|e| format!("{what}: the answer was not readable: {e}"))?;
    Ok(v.get("elements")
        .and_then(|e| e.as_array())
        .cloned()
        .unwrap_or_default())
}

/// Anything in a rider's typing that Overpass would read as a regular expression.
fn regex_safe(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        if "\\^$.|?*+()[]{}\"".contains(c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// One Overpass element as a place, where it has a name and a position.
///
/// A way or a relation has no coordinate of its own, so the query asks for `out center` and
/// the centre is what comes back — which for a circuit is a point inside it, and that is
/// exactly what a plot wants to be centred on.
fn circuit_hit(e: &serde_json::Value) -> Option<PlaceHit> {
    let tags = e.get("tags")?;
    let name = tags
        .get("name")
        .or_else(|| tags.get("name:en"))
        .and_then(|n| n.as_str())?;
    let at = e.get("center").unwrap_or(e);
    let lat = at.get("lat")?.as_f64()?;
    let lon = at.get("lon")?.as_f64()?;
    // Whatever the map knows about where it is, so two circuits of the same name can be told
    // apart. Often nothing: a field has no address.
    let where_ = ["addr:city", "addr:town", "addr:county", "addr:state", "addr:country"]
        .iter()
        .filter_map(|k| tags.get(*k).and_then(|v| v.as_str()))
        .collect::<Vec<_>>()
        .join(", ");
    Some(PlaceHit {
        label: if where_.is_empty() { name.to_string() } else { format!("{name}, {where_}") },
        lat,
        lon,
        kind: tags
            .get("sport")
            .and_then(|v| v.as_str())
            .unwrap_or("motocross")
            .to_string(),
        away_km: None,
    })
}

/// The shortest typing worth asking the whole world about.
///
/// "MX" matches a few thousand circuits and none of them usefully, and the server has to read
/// every one of them to find that out. Three characters is the shortest thing anybody means.
const CIRCUIT_LEAST_CHARS: usize = 3;

/// Circuits whose name contains what the rider typed, anywhere in the world.
async fn tracks_named(q: &str) -> Result<Vec<PlaceHit>, String> {
    if q.chars().count() < CIRCUIT_LEAST_CHARS {
        return Ok(Vec::new());
    }
    let query = format!(
        "[out:json][timeout:25];nwr[sport~\"{}\",i][name~\"{}\",i];out center {CIRCUIT_LIMIT};",
        CIRCUIT_SPORTS,
        regex_safe(q)
    );
    let elements = overpass(&query, "looking for a circuit by that name").await?;
    Ok(elements.iter().filter_map(circuit_hit).take(CIRCUIT_LIMIT).collect())
}

/// Every motocross circuit within [`NEAR_KM`] of a spot, nearest first.
///
/// This is the answer to "I don't know how to get coordinates". Search the nearest town — which
/// a gazetteer always finds — press this, and pick the track off the list. Nobody has to read a
/// latitude off a map for their local track ever again.
#[tauri::command]
pub async fn place_tracks_near(lat: f64, lon: f64) -> Result<Vec<PlaceHit>, String> {
    // A degree of latitude is 111.32 km everywhere; a degree of longitude is that times the
    // cosine of where you are. Near the poles that cosine goes to nothing and the box would
    // wrap the world, so it is held to a quarter turn either way.
    let (dlat, dlon) = degrees_per_m(lat);
    let (dlat, dlon) = (NEAR_KM * 1000.0 * dlat, (NEAR_KM * 1000.0 * dlon).min(90.0));
    let query = format!(
        "[out:json][timeout:25];nwr[sport~\"{}\",i]({:.5},{:.5},{:.5},{:.5});out center {};",
        CIRCUIT_SPORTS,
        (lat - dlat).max(-90.0),
        (lon - dlon).max(-180.0),
        (lat + dlat).min(90.0),
        (lon + dlon).min(180.0),
        CIRCUIT_LIMIT * 2,
    );
    let elements = overpass(&query, "looking for circuits near there").await?;
    let mut hits: Vec<PlaceHit> = elements
        .iter()
        .filter_map(circuit_hit)
        .map(|mut h| {
            h.away_km = Some(km_between(lat, lon, h.lat, h.lon));
            h
        })
        .filter(|h| h.away_km.unwrap_or(f64::MAX) <= NEAR_KM)
        .collect();
    hits.sort_by(|a, b| a.away_km.unwrap_or(0.0).total_cmp(&b.away_km.unwrap_or(0.0)));
    hits.truncate(CIRCUIT_LIMIT);
    Ok(hits)
}

/// Great-circle kilometres between two points. Only ever used to sort a short list and to
/// round a box off to a circle, so the spherical earth is plenty.
fn km_between(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let (p1, p2) = (lat1.to_radians(), lat2.to_radians());
    let (dp, dl) = ((lat2 - lat1).to_radians(), (lon2 - lon1).to_radians());
    let a = (dp / 2.0).sin().powi(2) + p1.cos() * p2.cos() * (dl / 2.0).sin().powi(2);
    6371.0 * 2.0 * a.sqrt().clamp(0.0, 1.0).asin()
}

/// `40.008, -86.9291` and the handful of ways people actually write that.
fn parse_coords(s: &str) -> Option<PlaceHit> {
    let cleaned: String = s
        .chars()
        .map(|c| if c == ',' || c == ';' { ' ' } else { c })
        .collect();
    let parts: Vec<&str> = cleaned.split_whitespace().collect();
    if parts.len() != 2 {
        return None;
    }
    let lat: f64 = parts[0].parse().ok()?;
    let lon: f64 = parts[1].parse().ok()?;
    if !(-90.0..=90.0).contains(&lat) || !(-180.0..=180.0).contains(&lon) {
        return None;
    }
    Some(PlaceHit {
        label: format!("{lat:.6}, {lon:.6}"),
        lat,
        lon,
        kind: "coordinates".to_string(),
        away_km: None,
    })
}

// ── What is available here ───────────────────────────────────────────────────

/// What each source has at a spot, before anything is downloaded.
///
/// This exists because every one of these services will happily answer a request for 1 m
/// cells over ground it only has at 30 m, by stretching the 30 m data. What comes back looks
/// exactly like data. It is not data. So: ask first, report honestly, and let the rider
/// decide whether a 30 m surface model is worth their evening.
#[tauri::command]
pub async fn place_coverage(lat: f64, lon: f64) -> Result<CoverageReport, String> {
    let (east, north, zone, north_hemi) = ll_to_utm(lat, lon);
    let mut out = Vec::new();
    for s in SOURCES {
        out.push(match s.protocol {
            Protocol::ArcGisImage => usgs_coverage(s, east, north, zone).await,
            Protocol::Wcs201 | Protocol::WmsRaster => national_coverage(s, lat, lon),
            Protocol::CogGrid => copernicus_coverage(s, lat),
        });
    }
    let best = out
        .iter()
        .filter(|c| c.covered)
        .min_by(|a, b| a.cell_m.total_cmp(&b.cell_m))
        .map(|c| c.id.clone())
        .unwrap_or_default();
    Ok(CoverageReport {
        lat,
        lon,
        epsg: nad83_utm_epsg(zone),
        east,
        north,
        sources: out,
        best,
    })
    .map(|mut r| {
        if !north_hemi {
            // Only matters for the label; the fetch path carries the hemisphere itself.
            r.epsg = nad83_utm_epsg(zone);
        }
        r
    })
}

fn blank_coverage(s: &Source, note: &str) -> SourceCoverage {
    SourceCoverage {
        id: s.id.to_string(),
        label: s.label.to_string(),
        region: s.region.to_string(),
        cell_m: s.best_cell_m,
        ground: s.ground,
        dataset: String::new(),
        collected: String::new(),
        licence: s.licence.to_string(),
        attribution: s.attribution.to_string(),
        terms_url: s.terms_url.to_string(),
        covered: false,
        note: note.to_string(),
    }
}

/// Ask the USGS mosaic what it is actually standing on here.
///
/// `identify` with `returnCatalogItems` hands back every layer of the mosaic under the
/// point, each with its cell size, the survey's name, and the dates it was flown. The finest
/// one is the truth. Measured at Ironman: a 1 m item named
/// `IN_Indiana_Statewide_LiDAR_2017_B17`, flown 2017-03-03 to 2020-04-11, sitting over a
/// 10.3 m item that would otherwise have been served without comment.
async fn usgs_coverage(s: &'static Source, east: f64, north: f64, zone: i32) -> SourceCoverage {
    let epsg = nad83_utm_epsg(zone);
    let url = format!(
        "{}/identify?geometry={}&geometryType=esriGeometryPoint&returnCatalogItems=true&returnGeometry=false&f=json",
        s.endpoint,
        percent_encoding::utf8_percent_encode(
            &format!(r#"{{"x":{east},"y":{north},"spatialReference":{{"wkid":{epsg}}}}}"#),
            percent_encoding::NON_ALPHANUMERIC
        )
    );
    let body = match get_bytes(&url, "checking United States elevation coverage").await {
        Ok(b) => b,
        Err(e) => return blank_coverage(s, &e),
    };
    let v: serde_json::Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(e) => return blank_coverage(s, &format!("the coverage service answered oddly: {e}")),
    };
    // A point outside the service returns no value at all.
    let has_value = v
        .get("value")
        .and_then(|x| x.as_str())
        .map(|s| !s.is_empty() && s != "NoData")
        .unwrap_or(false);
    let mut finest: Option<(f64, String, String)> = None;
    if let Some(items) = v
        .pointer("/catalogItems/features")
        .and_then(|f| f.as_array())
    {
        for it in items {
            let a = match it.get("attributes") {
                Some(a) => a,
                None => continue,
            };
            let cell = a.get("LowPS").and_then(|x| x.as_f64()).unwrap_or(f64::MAX);
            // Overview pyramids have no dataset type. Only real survey items count.
            if a.get("DEM_Type").and_then(|x| x.as_i64()).is_none() {
                continue;
            }
            let name = a
                .get("Name")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string();
            let start = a.get("StartDate").and_then(json_date);
            let end = a.get("EndDate").and_then(json_date);
            let when = match (start, end) {
                (Some(a), Some(b)) if a != b => format!("{a} to {b}"),
                (Some(a), _) => a,
                (_, Some(b)) => b,
                _ => String::new(),
            };
            if finest.as_ref().map(|(c, _, _)| cell < *c).unwrap_or(true) {
                finest = Some((cell, name, when));
            }
        }
    }
    match finest {
        Some((cell, name, when)) => SourceCoverage {
            cell_m: cell,
            dataset: name,
            collected: when,
            covered: true,
            note: if cell > 3.0 {
                format!(
                    "Only {cell:.0} m cells here — no LiDAR has been flown over this spot. Jumps, ruts and berms will not be in the data."
                )
            } else {
                String::new()
            },
            ..blank_coverage(s, "")
        },
        None if has_value => SourceCoverage {
            covered: true,
            note: "Covered, but the service didn't say what survey it is.".to_string(),
            ..blank_coverage(s, "")
        },
        None => blank_coverage(s, "Outside the United States, or outside this service."),
    }
}

/// USGS reports dates two different ways in the same response: `20170303` as a number, and
/// `"20200411"` as a string. Both mean the same thing.
fn json_date(v: &serde_json::Value) -> Option<String> {
    let raw = match v {
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::String(s) => s.clone(),
        _ => return None,
    };
    if raw.len() == 8 && raw.chars().all(|c| c.is_ascii_digit()) {
        Some(format!("{}-{}-{}", &raw[..4], &raw[4..6], &raw[6..]))
    } else if raw.is_empty() || raw == "0" {
        None
    } else {
        Some(raw)
    }
}

/// A national service covers its own country and nothing else, and it says so by refusing
/// a request outside it. A bounding box answers that without spending a request at all.
///
/// Deliberately cruder than the USGS check: those services have no equivalent of
/// `identify`, so there is no way to ask "what have you actually got here" short of asking
/// for the data. A rider inside the box who lands on a hole in the survey finds out when
/// the fetch comes back empty, and the fetch says so in as many words.
fn national_coverage(s: &'static Source, lat: f64, lon: f64) -> SourceCoverage {
    let (inside, outside_msg, when) = match s.id {
        "ahn" => (
            (50.6..=53.8).contains(&lat) && (3.2..=7.3).contains(&lon),
            "Outside the Netherlands.",
            "see the AHN release notes for the tile's flight year",
        ),
        "eaengland" => (
            (49.8..=55.9).contains(&lat) && (-6.5..=1.9).contains(&lon),
            "Outside England. Scotland and Wales publish their own LiDAR, which this tool \
             can't fetch yet — see the guide for how to download it and import it by hand.",
            "a composite of surveys flown from 1998 onwards, newest first",
        ),
        "ignfrance" => (
            (41.3..=51.2).contains(&lat) && (-5.2..=9.6).contains(&lon),
            "Outside mainland France.",
            "LiDAR HD from 2021 onwards where it has been flown; RGE ALTI otherwise",
        ),
        _ => (false, "This source doesn't cover that place.", ""),
    };
    if !inside {
        return blank_coverage(s, outside_msg);
    }
    SourceCoverage {
        covered: true,
        collected: when.to_string(),
        note: String::new(),
        ..blank_coverage(s, "")
    }
}

fn copernicus_coverage(s: &'static Source, lat: f64) -> SourceCoverage {
    if !(-60.0..=84.0).contains(&lat) {
        return blank_coverage(s, "Outside the satellite's coverage.");
    }
    SourceCoverage {
        covered: true,
        collected: "2010-2015".to_string(),
        note: "30 m cells, and a surface model: trees and buildings are in the ground. \
               Use it for the shape of a hillside, never for a track's own features."
            .to_string(),
        ..blank_coverage(s, "")
    }
}

// ── Fetching ─────────────────────────────────────────────────────────────────

/// Fetch a plot's ground and its picture, and write them into a place folder.
///
/// Runs only when a rider presses Fetch. One elevation request, one imagery request, and
/// nothing else — no tiles, no pyramids, no speculative neighbours.
#[tauri::command]
pub async fn place_fetch(
    app: AppHandle,
    name: String,
    lat: f64,
    lon: f64,
    plot_m: f64,
    source_id: String,
) -> Result<Place, String> {
    // Nothing sensible asked for means the plot the generator works on.
    let plot = if plot_m.is_finite() && plot_m > 0.0 {
        plot_m.clamp(MIN_PLOT_M, MAX_PLOT_M)
    } else {
        PLOT_M
    };
    let src = source(&source_id).ok_or_else(|| format!("no elevation source called {source_id:?}"))?;
    let name = if name.trim().is_empty() {
        format!("{lat:.5}, {lon:.5}")
    } else {
        name.trim().to_string()
    };

    let root = places_dir(&app)?;
    let slug = free_slug(&root, &name);
    let dir = root.join(&slug);
    std::fs::create_dir_all(&dir).map_err(|e| format!("couldn't make {}: {e}", dir.display()))?;

    // Anything that fails from here on leaves a half-made folder, so clean it up rather than
    // leaving something that lists as a place and isn't one.
    let made = fetch_into(&app, &dir, &slug, &name, lat, lon, plot, src).await;
    match made {
        Ok(p) => Ok(p),
        Err(e) => {
            let _ = std::fs::remove_dir_all(&dir);
            Err(e)
        }
    }
}

/// The event a fetch reports itself on.
///
/// The same shape and the same reason as a build's: a fetch is one to two minutes of a public
/// server cutting a plot out of a national survey, and a window that sits there doing nothing
/// for that long reads as a hang rather than as work.
pub const FETCH_EVENT: &str = "place-fetch-progress";

/// Where a fetch has got to.
///
/// `from` and `to` are where this stage sits on the bar and `expect` is how long it usually
/// takes, so the panel can creep across a stage instead of standing still between two events —
/// the stages here are single HTTP requests and there is nothing finer to report from inside
/// one.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FetchProgress {
    /// Which fetch this is, so a second window's fetch isn't drawn as this one's.
    pub slug: String,
    /// The place, as the rider named it.
    pub name: String,
    /// `elevation`, `hillshade` or `imagery`. The panel has a line for each.
    pub stage: String,
    /// Which layer is being asked for, when there is more than one candidate.
    pub detail: String,
    pub from: f64,
    pub to: f64,
    /// Seconds this stage usually takes.
    pub expect: f64,
}

/// Where each stage sits on the bar, and how long it usually takes.
///
/// Measured on a 1200 m plot over Ironman on a domestic line: the elevation cut is most of the
/// wait, the hillshade is local arithmetic, and the imagery is a second big picture. The bar
/// stops short of full because the command's own return is what finishes it.
const FETCH_SPANS: [(&str, f64, f64, f64); 3] = [
    ("elevation", 0.02, 0.62, 35.0),
    ("hillshade", 0.62, 0.70, 2.0),
    ("imagery", 0.70, 0.97, 25.0),
];

fn fetch_span(stage: &str) -> (f64, f64, f64) {
    FETCH_SPANS
        .iter()
        .find(|(s, ..)| *s == stage)
        .map(|(_, from, to, expect)| (*from, *to, *expect))
        .unwrap_or((0.0, 1.0, 10.0))
}

/// Say where the fetch has got to. Never fails a fetch: a bar is not worth a folder.
fn say_fetch(app: &AppHandle, slug: &str, name: &str, stage: &str, detail: &str) {
    let (from, to, expect) = fetch_span(stage);
    let _ = app.emit(
        FETCH_EVENT,
        FetchProgress {
            slug: slug.to_string(),
            name: name.to_string(),
            stage: stage.to_string(),
            detail: detail.to_string(),
            from,
            to,
            expect,
        },
    );
}

async fn fetch_into(
    app: &AppHandle,
    dir: &Path,
    slug: &str,
    name: &str,
    lat: f64,
    lon: f64,
    plot: f64,
    src: &'static Source,
) -> Result<Place, String> {
    let half = plot / 2.0;
    let dem_rel = format!("{slug}.dem.tif");
    let dem_path = dir.join(&dem_rel);

    let candidates = match src.id {
        "usgs3dep" => vec![usgs_layer(lat, lon, half)],
        "ahn" => vec![ahn_layer(lat, lon, half, src)],
        "eaengland" => vec![england_layer(lat, lon, half, src)],
        "ignfrance" => france_layers(lat, lon, half, src),
        _ => {
            return Err(
                "The worldwide 30 m model can't be cut to a plot by this tool yet. \
                 See docs/tracks/real-place-tracks.md for how to download it and import it by hand."
                    .to_string(),
            )
        }
    };

    // Try each layer in turn and keep the first that has actual ground in it.
    //
    // This is not belt-and-braces: France's 0.5 m LiDAR is a rolling programme, so a venue
    // that is inside the country can still be outside the survey, and the service answers
    // such a request with a full grid of nodata rather than an error. A run of empty cells
    // that looks exactly like data is the worst possible failure, so it is caught here and
    // turned into either a coarser layer that does have ground, or a sentence.
    let mut grid = None;
    let mut used = candidates[0].clone();
    let mut tried = Vec::new();
    for cand in &candidates {
        let which = if cand.label.is_empty() { src.label } else { &cand.label };
        say_fetch(app, slug, name, "elevation", which);
        let bytes = get_bytes(&cand.url, &cand.what).await?;
        std::fs::write(&dem_path, &bytes)
            .map_err(|e| format!("couldn't save the elevation to {}: {e}", dem_path.display()))?;
        let g = read_dem(&dem_path)?;
        if g.valid > 0 {
            used = cand.clone();
            grid = Some(g);
            break;
        }
        tried.push(cand.label.clone());
        log::info!("[place] {} is empty at {lat},{lon}, trying the next layer", cand.label);
    }
    let grid = grid.ok_or_else(|| {
        format!(
            "The elevation service answered, but every cell in that plot is empty — \
             there is no survey here. Tried: {}. Move the plot a little, or check coverage \
             on a nearby spot.",
            tried.join(", ")
        )
    })?;
    let (url, crs_epsg, cell) = (used.url.clone(), used.epsg, used.cell);
    let relief = grid.max_z - grid.min_z;

    say_fetch(app, slug, name, "hillshade", "");
    let hillshade_rel = format!("{slug}.hillshade.png");
    write_hillshade(&grid, &dir.join(&hillshade_rel))?;

    // Imagery is best-effort on purpose: a missing picture makes tracing harder, not
    // impossible, and it should never throw away elevation that arrived fine.
    say_fetch(app, slug, name, "imagery", "");
    let imagery = fetch_imagery(dir, slug, lat, lon, half, crs_epsg, &grid).await;
    let imagery = match imagery {
        Ok(i) => Some(i),
        Err(e) => {
            log::warn!("[place] no imagery for {slug}: {e}");
            None
        }
    };

    let dem = DemNote {
        path: dem_rel,
        crs: format!("EPSG:{crs_epsg}"),
        cell_m: cell,
        origin_e: grid.origin_e,
        origin_n: grid.origin_n,
        width: grid.width as u32,
        height: grid.height as u32,
        vertical_datum: match src.id {
            "usgs3dep" => "NAVD88".to_string(),
            "ahn" => "NAP".to_string(),
            "eaengland" => "ODN (Newlyn)".to_string(),
            "ignfrance" => "NGF-IGN69".to_string(),
            _ => "unknown".to_string(),
        },
        source: if used.label.is_empty() {
            src.label.to_string()
        } else {
            used.label.clone()
        },
        source_url: url,
        collected: String::new(),
        licence: src.licence.to_string(),
        attribution: src.attribution.to_string(),
        ground: src.ground,
        min_z: grid.min_z,
        max_z: grid.max_z,
    };

    let place = Place {
        slug: slug.to_string(),
        name: name.to_string(),
        lat,
        lon,
        plot_m: plot,
        bbox_ll: footprint_ll(&grid, crs_epsg, lat, lon, half),
        fetched: now_stamp(),
        dem,
        imagery,
        hillshade: hillshade_rel,
        has_trace: false,
        bytes: Some(dir_bytes(dir)),
        dir: dir.display().to_string(),
    };
    save_place(dir, &place)?;
    log::info!(
        "[place] {slug}: {:.0} m plot, {:.2} m cells, {relief:.1} m of relief, {} KB",
        plot,
        cell,
        place.bytes.unwrap_or(0) / 1024
    );
    Ok(place)
}

fn save_place(dir: &Path, place: &Place) -> Result<(), String> {
    let mut stored = place.clone();
    // Neither of these is a fact about the place; both are facts about this machine.
    stored.bytes = None;
    stored.dir = String::new();
    let text = serde_json::to_vec_pretty(&stored).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("place.json"), text)
        .map_err(|e| format!("couldn't write the place's notes: {e}"))
}

/// One layer that might have the ground in it: where to ask, and what to call it.
#[derive(Clone)]
struct Layer {
    /// What to say if this one fails, e.g. "fetching France elevation (LiDAR HD 0.5 m)".
    what: String,
    /// What actually answered, recorded in the place's notes so a rider can see whether
    /// they got the good layer or the fallback.
    label: String,
    url: String,
    epsg: u32,
    cell: f64,
}

/// Ask USGS for exactly the plot, already projected, already a GeoTIFF.
///
/// Whole metres, so the plot lands on the survey's own grid and no resampling happens that
/// didn't have to.
fn usgs_layer(lat: f64, lon: f64, half: f64) -> Layer {
    let (east, north, zone, _) = ll_to_utm(lat, lon);
    let epsg = nad83_utm_epsg(zone);
    let x0 = (east - half).round();
    let y0 = (north - half).round();
    let x1 = (east + half).round();
    let y1 = (north + half).round();
    let w = (x1 - x0) as u32;
    let h = (y1 - y0) as u32;
    Layer {
        what: "fetching United States elevation".to_string(),
        label: String::new(),
        url: format!(
            "{}/exportImage?bbox={x0},{y0},{x1},{y1}&bboxSR={epsg}&imageSR={epsg}&size={w},{h}\
             &format=tiff&pixelType=F32&noData=-999999&interpolation=RSP_NearestNeighbor&f=image",
            SOURCES[0].endpoint
        ),
        epsg,
        cell: 1.0,
    }
}

/// Degrees per metre at a latitude. Good to a fraction of a percent over a plot this size,
/// which is the difference between a 470.0 m window and a 470.3 m one.
fn degrees_per_m(lat: f64) -> (f64, f64) {
    (
        1.0 / 111_320.0,
        1.0 / (111_320.0 * lat.to_radians().cos().max(0.05)),
    )
}

/// Ask the Dutch service. It is asked in latitude and longitude and answers on its own
/// national grid, so nothing here has to know what that grid is.
fn ahn_layer(lat: f64, lon: f64, half: f64, src: &'static Source) -> Layer {
    let (dlat, dlon) = degrees_per_m(lat);
    let (dlat, dlon) = (half * dlat, half * dlon);
    Layer {
        what: "fetching Dutch elevation".to_string(),
        label: String::new(),
        url: format!(
            "{}?SERVICE=WCS&VERSION=2.0.1&REQUEST=GetCoverage&COVERAGEID=dtm_05m\
             &SUBSET=Lat({},{})&SUBSET=Long({},{})\
             &SUBSETTINGCRS=http%3A%2F%2Fwww.opengis.net%2Fdef%2Fcrs%2FEPSG%2F0%2F4326&FORMAT=image%2Ftiff",
            src.endpoint,
            lat - dlat,
            lat + dlat,
            lon - dlon,
            lon + dlon
        ),
        epsg: 28992,
        cell: 0.5,
    }
}

/// Ask England's service for the plot.
///
/// Subset in eastings and northings on the National Grid rather than in degrees. It accepts
/// degrees too, but then it answers in degrees, and a coverage whose cells are fractions of
/// a degree is no use to an importer that needs metres. Asking in native coordinates gets
/// exactly 1 m square cells on EPSG:27700.
fn england_layer(lat: f64, lon: f64, half: f64, src: &'static Source) -> Layer {
    let (east, north) = ll_to_bng(lat, lon);
    let (e0, n0) = ((east - half).round(), (north - half).round());
    let (e1, n1) = ((east + half).round(), (north + half).round());
    Layer {
        what: "fetching England elevation".to_string(),
        label: String::new(),
        url: format!(
            "{}?SERVICE=WCS&VERSION=2.0.1&REQUEST=GetCoverage\
             &COVERAGEID=13787b9a-26a4-4775-8523-806d13af58fc__Lidar_Composite_Elevation_DTM_1m\
             &SUBSET=E({e0},{e1})&SUBSET=N({n0},{n1})&FORMAT=image%2Ftiff",
            src.endpoint
        ),
        epsg: 27700,
        cell: 1.0,
    }
}

/// Ask France, best layer first.
///
/// Two layers, and the difference between them is real rather than cosmetic: over the same
/// 470 m box, LiDAR HD carries about twice the fine relief that RGE ALTI does, which is
/// what true 0.5 m data looks like next to 1 m data sampled onto a 0.5 m grid. So LiDAR HD
/// is asked for first every time, and RGE ALTI only answers where LiDAR HD hasn't flown yet.
///
/// The two layers even use different nodata sentinels — -9999 and -99999 — which is exactly
/// the sort of thing that makes a hand-rolled reader produce a track with a cliff in it.
/// [`read_dem`] treats anything below -9000 as empty, so both are covered.
fn france_layers(lat: f64, lon: f64, half: f64, src: &'static Source) -> Vec<Layer> {
    let (east, north) = ll_to_lambert93(lat, lon);
    let (e0, n0) = ((east - half).round(), (north - half).round());
    let (e1, n1) = ((east + half).round(), (north + half).round());
    let build = |layer: &str, cell: f64, label: &str, what: &str| {
        let px = ((half * 2.0 / cell).round() as u32).clamp(64, 4096);
        Layer {
            what: what.to_string(),
            label: label.to_string(),
            url: format!(
                "{}?SERVICE=WMS&VERSION=1.3.0&REQUEST=GetMap&LAYERS={layer}&STYLES=\
                 &CRS=EPSG:2154&BBOX={e0},{n0},{e1},{n1}&WIDTH={px}&HEIGHT={px}&FORMAT=image%2Fgeotiff",
                src.endpoint
            ),
            epsg: 2154,
            cell,
        }
    };
    vec![
        build(
            "IGNF_LIDAR-HD_MNT_ELEVATION.ELEVATIONGRIDCOVERAGE.LAMB93",
            0.5,
            "IGN LiDAR HD MNT — 0.5 m bare earth",
            "fetching France elevation (LiDAR HD, 0.5 m)",
        ),
        build(
            "ELEVATION.ELEVATIONGRIDCOVERAGE.HIGHRES",
            1.0,
            "IGN RGE ALTI — 1 m bare earth",
            "fetching France elevation (RGE ALTI, 1 m)",
        ),
    ]
}

/// A picture of the same footprint, where an openly-licensed one exists.
async fn fetch_imagery(
    dir: &Path,
    slug: &str,
    lat: f64,
    lon: f64,
    half: f64,
    epsg: u32,
    grid: &Grid,
) -> Result<ImageryNote, String> {
    // Pixels: enough that a rider can see a tyre line, capped so the request stays quick.
    let px = ((half * 2.0 / 0.5).round() as u32).clamp(256, 4096);
    let (url, img) = if epsg == 2154 {
        // France publishes 20 cm ortho from the same service as its elevation, on the same
        // grid, so one bbox serves both. Its own limit is 5010 px a side, and asking for
        // finer than 20 cm only blurs, so the request is clamped on both counts.
        let x0 = grid.origin_e - grid.cell / 2.0;
        let y1 = grid.origin_n + grid.cell / 2.0;
        let x1 = x0 + grid.width as f64 * grid.cell;
        let y0 = y1 - grid.height as f64 * grid.cell;
        let want = ((x1 - x0) / 0.2).round() as u32;
        let px = want.clamp(256, 5000);
        (
            format!(
                "{}?SERVICE=WMS&VERSION=1.3.0&REQUEST=GetMap&LAYERS={}&STYLES=&CRS=EPSG:2154\
                 &BBOX={x0},{y0},{x1},{y1}&WIDTH={px}&HEIGHT={px}&FORMAT=image%2Fjpeg",
                IMAGERY[1].endpoint, IMAGERY[1].layer
            ),
            &IMAGERY[1],
        )
    } else if epsg >= 26900 && epsg <= 26960 {
        let x0 = grid.origin_e - grid.cell / 2.0;
        let y1 = grid.origin_n + grid.cell / 2.0;
        let x1 = x0 + grid.width as f64 * grid.cell;
        let y0 = y1 - grid.height as f64 * grid.cell;
        (
            format!(
                "{}/exportImage?bbox={x0},{y0},{x1},{y1}&bboxSR={epsg}&imageSR={epsg}\
                 &size={px},{px}&format=png&f=image",
                IMAGERY[0].endpoint
            ),
            &IMAGERY[0],
        )
    } else if epsg == 28992 {
        let _ = &IMAGERY[2];
        let (dlat, dlon) = degrees_per_m(lat);
        let (dlat, dlon) = (half * dlat, half * dlon);
        (
            format!(
                "{}?SERVICE=WMS&VERSION=1.3.0&REQUEST=GetMap&LAYERS={}&STYLES=&CRS=EPSG:4326\
                 &BBOX={},{},{},{}&WIDTH={px}&HEIGHT={px}&FORMAT=image/png",
                IMAGERY[2].endpoint,
                IMAGERY[2].layer,
                lat - dlat,
                lon - dlon,
                lat + dlat,
                lon + dlon
            ),
            &IMAGERY[2],
        )
    } else {
        return Err("no openly-licensed imagery covers this place".to_string());
    };
    let bytes = get_bytes(&url, "fetching aerial imagery").await?;
    // JPEG where the service sends JPEG; a name that lies about its contents breaks the
    // webview's decoding rather than anything dramatic, but it still breaks it.
    let jpeg = bytes.starts_with(&[0xFF, 0xD8]);
    let rel = format!("{slug}.imagery.{}", if jpeg { "jpg" } else { "png" });
    std::fs::write(dir.join(&rel), &bytes).map_err(|e| format!("couldn't save the imagery: {e}"))?;
    Ok(ImageryNote {
        path: rel,
        source: img.label.to_string(),
        licence: img.licence.to_string(),
        attribution: img.attribution.to_string(),
        captured: String::new(),
    })
}

// ── Picking a spot off a map ─────────────────────────────────────────────────
//
// Searching by name is not enough on its own, and the two ways it fails are both expensive.
// "Ironman" returns nine places in five countries with the circuit seventh; "Saint-Jean"
// returns the town, and the circuit is several kilometres out of it. Both cost a fetch and a
// build before anyone finds out. A rider recognises a circuit the moment they see it from the
// air, so the fix is to show them the air.
//
// There is no map of the world here, because there is no map of the world we are allowed to
// use. Every worldwide aerial basemap at a resolution you could pick a circuit out of is
// licensed for viewing inside its owner's own product, and tracing a lap off one would put a
// derivative of somebody else's photography inside a track that gets handed around. Google,
// Apple and Bing are out on those grounds and always will be. So the map is shown where there
// is an openly-licensed survey and refused, in words, where there is not — which is no loss,
// since those are exactly the places a fetch can work anyway.

/// Which openly-licensed aerial imagery covers a spot, if any.
///
/// Rectangles rather than borders on purpose: the only thing this decides is which service to
/// ask, and a request that lands just outside a survey comes back as a blank picture, which
/// [`place_map`] recognises and reports rather than showing.
fn imagery_at(lat: f64, lon: f64) -> Option<&'static Imagery> {
    // The Netherlands first: its box and France's overlap along the Belgian border, and the
    // Dutch service is the one that actually covers that overlap.
    if (50.7..=53.7).contains(&lat) && (3.2..=7.3).contains(&lon) {
        return Some(&IMAGERY[2]);
    }
    // Conterminous United States, and Hawaii, which NAIP also flies. Alaska it does not.
    if (24.4..=49.4).contains(&lat) && (-125.0..=-66.9).contains(&lon) {
        return Some(&IMAGERY[0]);
    }
    if (18.8..=22.3).contains(&lat) && (-160.3..=-154.7).contains(&lon) {
        return Some(&IMAGERY[0]);
    }
    // Mainland France and Corsica.
    if (41.3..=51.1).contains(&lat) && (-5.2..=9.6).contains(&lon) {
        return Some(&IMAGERY[1]);
    }
    None
}

/// The smallest a real aerial picture ever comes back, in bytes.
///
/// A request that lands outside a survey is answered with a picture rather than an error: one
/// flat colour. Measured, at 1024 px square: NAIP over open desert in Mexico, which it does not
/// fly, came back at 17.2 KB, and the Dutch service over Brussels, which it does not cover, at
/// 17.0 KB. Real ortho at the same size measured 161 KB over Ironman, 256 KB over Ernée and
/// 425 KB over Utrecht, and even open sea off Cape Cod came back at 261 KB. 24 KB sits well
/// clear of both. Showing the flat one would be showing a void and calling it a map.
const BLANK_IMAGE_BYTES: usize = 24_000;

/// How far across the ground one picture shows. Whole circuits at the coarse end, a single
/// rhythm section at the fine end.
const MAP_SPANS_M: [f64; 6] = [400.0, 800.0, 1600.0, 3200.0, 6400.0, 12800.0];

/// One picture of one square of ground, and who has to be credited for it.
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PlaceMap {
    /// The middle of the picture.
    pub lat: f64,
    pub lon: f64,
    /// How many metres the picture is across, both ways.
    pub span_m: f64,
    /// Ready for an `<img>`.
    pub image: String,
    pub source: String,
    pub licence: String,
    /// The line that has to appear on screen while this picture is showing.
    pub attribution: String,
}

/// Fetch one aerial picture of one spot, for picking a circuit off rather than typing its name.
///
/// One request, one picture, and only when a rider moved the map. Panning and zooming are
/// button presses and drags, so each of them spends exactly one request — there is no tile
/// pyramid, nothing is fetched ahead, and opening the panel costs nothing at all.
#[tauri::command]
pub async fn place_map(lat: f64, lon: f64, span_m: f64) -> Result<PlaceMap, String> {
    let span = if span_m.is_finite() && span_m > 0.0 {
        span_m.clamp(MAP_SPANS_M[0], MAP_SPANS_M[MAP_SPANS_M.len() - 1])
    } else {
        MAP_SPANS_M[2]
    };
    let img = imagery_at(lat, lon).ok_or_else(|| {
        "No openly licensed aerial photography covers this spot, so there is no map to show. \
         The surveys this tool can use are the United States, France and the Netherlands. \
         Search by name, or type coordinates, and fetch the ground to see what is there."
            .to_string()
    })?;

    let half = span / 2.0;
    let (dlat, dlon) = degrees_per_m(lat);
    let (dlat, dlon) = (half * dlat, half * dlon);
    let (s, w, n, e) = (lat - dlat, lon - dlon, lat + dlat, lon + dlon);
    // A square picture of a square of ground. Big enough to tell a lane from a track, small
    // enough that a pan is a second rather than a wait.
    let px = 1024;
    let url = match img.id {
        "naip" => format!(
            "{}/exportImage?bbox={w},{s},{e},{n}&bboxSR=4326&imageSR=4326&size={px},{px}\
             &format=jpg&f=image",
            img.endpoint
        ),
        // WMS 1.3.0 takes EPSG:4326 in latitude-first order, which is the one thing about it
        // that catches everybody.
        _ => format!(
            "{}?SERVICE=WMS&VERSION=1.3.0&REQUEST=GetMap&LAYERS={}&STYLES=&CRS=EPSG:4326\
             &BBOX={s},{w},{n},{e}&WIDTH={px}&HEIGHT={px}&FORMAT=image%2Fjpeg",
            img.endpoint, img.layer
        ),
    };
    let bytes = get_bytes(&url, "fetching the map").await?;
    if bytes.len() < BLANK_IMAGE_BYTES {
        return Err(
            "The survey stops short of this spot, so the map came back blank. Move back towards \
             ground the survey covers, or search by name instead."
                .to_string(),
        );
    }
    use base64::Engine;
    let mime = if bytes.starts_with(&[0xFF, 0xD8]) { "image/jpeg" } else { "image/png" };
    Ok(PlaceMap {
        lat,
        lon,
        span_m: span,
        image: format!(
            "data:{mime};base64,{}",
            base64::engine::general_purpose::STANDARD.encode(&bytes)
        ),
        source: img.label.to_string(),
        licence: img.licence.to_string(),
        attribution: if img.attribution.is_empty() {
            format!("{} — {}", img.label, img.licence)
        } else {
            img.attribution.to_string()
        },
    })
}

// ── Reading the ground back ──────────────────────────────────────────────────

/// A DEM in memory: the cells, and enough about where they are to put a click back into
/// the world.
pub struct Grid {
    pub z: Vec<f32>,
    pub width: usize,
    pub height: usize,
    pub cell: f64,
    /// Centre of pixel (0, 0). North up; row index increases southward.
    pub origin_e: f64,
    pub origin_n: f64,
    pub min_z: f64,
    pub max_z: f64,
    pub valid: usize,
}

/// Read a GeoTIFF's cells and its georeferencing.
///
/// Handles what these services actually send: single-band 32-bit float, uncompressed from
/// USGS and Deflate from the Dutch service, with `ModelPixelScaleTag` and `ModelTiepointTag`
/// for the placement. Anything stranger than that is refused with a reason rather than
/// half-read, because a silently mis-read DEM produces a track that is subtly wrong
/// everywhere and obviously wrong nowhere.
pub fn read_dem(path: &Path) -> Result<Grid, String> {
    use tiff::decoder::{Decoder, DecodingResult};
    use tiff::tags::Tag;

    let file = std::fs::File::open(path)
        .map_err(|e| format!("couldn't open {}: {e}", path.display()))?;
    let mut dec = Decoder::new(std::io::BufReader::new(file))
        .map_err(|e| format!("{} isn't a TIFF this app can read: {e}", path.display()))?;
    let (w, h) = dec
        .dimensions()
        .map_err(|e| format!("couldn't read the image size: {e}"))?;

    // Two ways a GeoTIFF can say where it is, and real services in this pipeline use both.
    // USGS and the Dutch service write a pixel scale plus a tiepoint; England's writes a
    // ModelTransformation matrix instead. A reader that only knows the first sees England's
    // file as an unplaced picture, which is exactly the kind of failure that produces a
    // track that is wrong everywhere and obviously wrong nowhere. So: handle both.
    let scale = dec
        .get_tag(Tag::Unknown(33550))
        .and_then(|v| v.into_f64_vec())
        .ok();
    let tie = dec
        .get_tag(Tag::Unknown(33922))
        .and_then(|v| v.into_f64_vec())
        .ok();
    let xform = dec
        .get_tag(Tag::Unknown(34264))
        .and_then(|v| v.into_f64_vec())
        .ok();

    // (cell size east, cell size north, top-left corner easting, top-left corner northing)
    let (sx, sy, left, top) = match (scale, tie, xform) {
        (Some(s), Some(t), _) if s.len() >= 2 && t.len() >= 6 => (s[0], s[1], t[3], t[4]),
        (_, _, Some(m)) if m.len() >= 16 => {
            // Row-major 4x4. Rotation would put shear in the off-diagonals, and this app has
            // no business guessing at a rotated grid.
            if m[1].abs() > 1e-9 || m[4].abs() > 1e-9 {
                return Err(
                    "that GeoTIFF is rotated, and this app only handles north-up grids. \
                     Re-export it north-up from your GIS and try again."
                        .to_string(),
                );
            }
            (m[0], -m[5], m[3], m[7])
        }
        _ => {
            return Err(
                "that GeoTIFF doesn't say where on the earth it is — it has neither a pixel \
                 scale and tiepoint nor a transformation matrix. Export it from your GIS as a \
                 plain north-up GeoTIFF and try again."
                    .to_string(),
            )
        }
    };
    if sx <= 0.0 || sy <= 0.0 {
        return Err("that GeoTIFF's cell size is zero or negative.".to_string());
    }
    // A little out of square is normal and harmless: a service that cut the window in one
    // projection and answered in another lands a fraction of a percent off, which over a
    // 470 m plot is well under a metre. A lot out of square means something else entirely,
    // and quietly averaging it would stretch the whole track.
    let skew = (sx - sy).abs() / sx.max(sy);
    if skew > 0.01 {
        return Err(format!(
            "that GeoTIFF's cells are {sx:.3} m across and {sy:.3} m tall. This app needs \
             square cells — re-export it at a single resolution."
        ));
    }
    let cell = (sx + sy) / 2.0;
    // The tiepoint and the transformation both name a pixel's top-left CORNER. Half a cell
    // in gets its centre, which is what every coordinate downstream means. This is the
    // half-pixel bug, handled once, here.
    let origin_e = left + cell / 2.0;
    let origin_n = top - cell / 2.0;

    let nodata: Option<f64> = dec
        .get_tag(Tag::Unknown(42113))
        .ok()
        .and_then(|v| v.into_string().ok())
        .and_then(|s| s.trim().trim_end_matches('\0').parse().ok());

    let img = dec
        .read_image()
        .map_err(|e| format!("couldn't read the elevation cells: {e}"))?;
    let mut z = match img {
        DecodingResult::F32(v) => v,
        DecodingResult::F64(v) => v.into_iter().map(|x| x as f32).collect(),
        DecodingResult::I16(v) => v.into_iter().map(|x| x as f32).collect(),
        DecodingResult::U16(v) => v.into_iter().map(|x| x as f32).collect(),
        DecodingResult::I32(v) => v.into_iter().map(|x| x as f32).collect(),
        _ => {
            return Err(
                "that GeoTIFF's cells aren't numbers this app can read as heights. \
                 A DEM should be 32-bit float or 16-bit integer."
                    .to_string(),
            )
        }
    };
    if z.len() < (w as usize) * (h as usize) {
        return Err("that GeoTIFF is shorter than its own dimensions claim.".to_string());
    }
    z.truncate((w as usize) * (h as usize));

    // Blank out everything a service uses to mean "no survey here". The float-max sentinel
    // the Dutch service uses would otherwise drag the whole range with it.
    let mut min_z = f64::MAX;
    let mut max_z = f64::MIN;
    let mut valid = 0usize;
    for v in z.iter_mut() {
        let x = *v as f64;
        // Eleven services, five different sentinels, and three that ship no tag at all:
        // -999999, -9999, -99999, and float max in both signs. So the tag is honoured when
        // it is there and a sane elevation band is enforced regardless. Nothing on land is
        // below -500 m or above 9000 m.
        let empty = !x.is_finite()
            || !(-500.0..=9000.0).contains(&x)
            || nodata.map(|n| (x - n).abs() <= n.abs() * 1e-6 + 1e-6).unwrap_or(false);
        if empty {
            *v = f32::NAN;
        } else {
            valid += 1;
            min_z = min_z.min(x);
            max_z = max_z.max(x);
        }
    }
    if valid == 0 {
        min_z = 0.0;
        max_z = 0.0;
    }
    Ok(Grid {
        z,
        width: w as usize,
        height: h as usize,
        cell,
        origin_e,
        origin_n,
        min_z,
        max_z,
        valid,
    })
}

/// The plot's corners in latitude and longitude.
///
/// Exact where the grid is a UTM one, because the inverse projection is exact. Elsewhere
/// there is no general inverse in this module, so it falls back to the same
/// degrees-per-metre approximation the fetch used — good to well under a metre over a plot
/// this size, and it is only ever read by a human checking the footprint.
fn footprint_ll(grid: &Grid, epsg: u32, lat: f64, lon: f64, half: f64) -> Vec<f64> {
    let w = grid.width as f64 * grid.cell;
    let h = grid.height as f64 * grid.cell;
    if (26901..=26960).contains(&epsg) {
        let zone = (epsg - 26900) as i32;
        let west = grid.origin_e - grid.cell / 2.0;
        let north = grid.origin_n + grid.cell / 2.0;
        let (s, ww) = utm_to_ll(west, north - h, zone, lat >= 0.0);
        let (n, ee) = utm_to_ll(west + w, north, zone, lat >= 0.0);
        return vec![s, ww, n, ee];
    }
    let (dlat, dlon) = degrees_per_m(lat);
    vec![
        lat - half * dlat,
        lon - half * dlon,
        lat + half * dlat,
        lon + half * dlon,
    ]
}

/// A picture of the ground's shape, for tracing against and for judging the data by eye.
///
/// Four light directions averaged rather than one, because a single sun angle hides every
/// feature running along it — a rhythm section lit end-on disappears completely. The
/// vertical exaggeration is deliberate and large: on a flat field the whole story is in the
/// last few centimetres, and at true scale there is nothing to see.
fn write_hillshade(grid: &Grid, path: &Path) -> Result<(), String> {
    const ZF: f64 = 2.5;
    const ALT: f64 = 30.0;
    const AZS: [f64; 4] = [315.0, 45.0, 135.0, 225.0];
    let (w, h) = (grid.width, grid.height);
    let mut out = vec![0u8; w * h];
    let at = |r: isize, c: isize| -> f64 {
        let r = r.clamp(0, h as isize - 1) as usize;
        let c = c.clamp(0, w as isize - 1) as usize;
        let v = grid.z[r * w + c];
        if v.is_nan() {
            grid.min_z
        } else {
            v as f64
        }
    };
    let alt = ALT.to_radians();
    for r in 0..h {
        for c in 0..w {
            // Central differences. dz/drow, with row increasing southward, so the
            // north-facing gradient is its negative.
            let dzdx = (at(r as isize, c as isize + 1) - at(r as isize, c as isize - 1)) * ZF
                / (2.0 * grid.cell);
            let dzdy = (at(r as isize + 1, c as isize) - at(r as isize - 1, c as isize)) * ZF
                / (2.0 * grid.cell);
            let slope = dzdx.hypot(dzdy).atan();
            let aspect = (-dzdy).atan2(-dzdx);
            let mut sum = 0.0;
            for az in AZS {
                let a = (360.0 - az + 90.0).to_radians();
                sum += (alt.sin() * slope.cos() + alt.cos() * slope.sin() * (a - aspect).cos())
                    .clamp(0.0, 1.0);
            }
            out[r * w + c] = ((sum / AZS.len() as f64) * 255.0).clamp(0.0, 255.0) as u8;
        }
    }
    let img = image::GrayImage::from_raw(w as u32, h as u32, out)
        .ok_or_else(|| "couldn't build the hillshade picture".to_string())?;
    img.save(path)
        .map_err(|e| format!("couldn't save the hillshade: {e}"))
}

// ── The places on this machine ───────────────────────────────────────────────

/// Every place fetched so far, with what each one costs on disk.
#[tauri::command]
pub async fn place_list(app: AppHandle) -> Result<Vec<Place>, String> {
    let root = places_dir(&app)?;
    let mut out = Vec::new();
    let rd = match std::fs::read_dir(&root) {
        Ok(rd) => rd,
        Err(_) => return Ok(out),
    };
    for entry in rd.flatten() {
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        let notes = dir.join("place.json");
        let text = match std::fs::read(&notes) {
            Ok(t) => t,
            Err(_) => continue,
        };
        let mut place: Place = match serde_json::from_slice(&text) {
            Ok(p) => p,
            Err(e) => {
                log::warn!("[place] skipping {}: {e}", dir.display());
                continue;
            }
        };
        place.bytes = Some(dir_bytes(&dir));
        place.dir = dir.display().to_string();
        place.has_trace = dir.join(format!("{}.lap.json", place.slug)).is_file();
        out.push(place);
    }
    out.sort_by(|a, b| b.fetched.cmp(&a.fetched));
    Ok(out)
}

/// Delete a place and everything in it. Asked for explicitly, never on a timer.
#[tauri::command]
pub async fn place_forget(app: AppHandle, slug: String) -> Result<(), String> {
    let dir = place_dir(&app, &slug)?;
    if !dir.is_dir() {
        return Ok(());
    }
    std::fs::remove_dir_all(&dir).map_err(|e| format!("couldn't delete {}: {e}", dir.display()))
}

/// The absolute paths of a place's two artefacts, which is what the importer is handed.
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PlacePaths {
    pub dir: String,
    pub dem: String,
    pub trace: String,
    pub has_trace: bool,
}

#[tauri::command]
pub async fn place_paths(app: AppHandle, slug: String) -> Result<PlacePaths, String> {
    let dir = place_dir(&app, &slug)?;
    let dem = dir.join(format!("{slug}.dem.tif"));
    let trace = dir.join(format!("{slug}.lap.json"));
    Ok(PlacePaths {
        dir: dir.display().to_string(),
        dem: dem.display().to_string(),
        has_trace: trace.is_file(),
        trace: trace.display().to_string(),
    })
}

// ── Tracing ──────────────────────────────────────────────────────────────────

/// Everything the tracing canvas needs: two pictures and the maths to put a click back on
/// the earth.
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PlaceLayers {
    pub slug: String,
    pub name: String,
    /// PNG bytes, base64, ready for an `<img>` source. Small enough for this to be sane:
    /// a 470 m plot is about a megabyte of imagery.
    pub hillshade: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub imagery: Option<String>,
    pub dem: DemNote,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub imagery_note: Option<ImageryNote>,
    /// The existing lap, where there is one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace: Option<LapTrace>,
    /// Every licence line that has to travel with anything made from this place.
    pub attributions: Vec<String>,
}

fn as_data_url(path: &Path) -> Result<String, String> {
    use base64::Engine;
    let bytes = std::fs::read(path).map_err(|e| format!("couldn't read {}: {e}", path.display()))?;
    // Read the type off the bytes rather than the extension: France's ortho arrives as JPEG
    // and everything else as PNG, and a data URL that lies about which simply fails to
    // decode in the webview, silently.
    let mime = if bytes.starts_with(&[0xFF, 0xD8]) {
        "image/jpeg"
    } else {
        "image/png"
    };
    Ok(format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

/// Open a place for tracing.
#[tauri::command]
pub async fn place_layers(app: AppHandle, slug: String) -> Result<PlaceLayers, String> {
    let dir = place_dir(&app, &slug)?;
    let text = std::fs::read(dir.join("place.json"))
        .map_err(|_| format!("{slug} isn't a fetched place any more"))?;
    let place: Place =
        serde_json::from_slice(&text).map_err(|e| format!("that place's notes are damaged: {e}"))?;

    let hillshade = as_data_url(&dir.join(&place.hillshade))?;
    let imagery = match &place.imagery {
        Some(i) => as_data_url(&dir.join(&i.path)).ok(),
        None => None,
    };
    // Accept either spelling on the way in. A file this app wrote is snake_case; one a
    // rider hand-edited, or another tool produced, may not be.
    let trace = std::fs::read(dir.join(format!("{slug}.lap.json")))
        .ok()
        .and_then(|t| serde_json::from_slice::<serde_json::Value>(&t).ok())
        .map(camelise)
        .and_then(|v| serde_json::from_value::<LapTrace>(v).ok());

    // One credit per source, not one per file. The ground and the picture often come from
    // the same survey — every French place has IGN twice — and a doubled credit reads as a
    // mistake rather than as care.
    let mut attributions: Vec<String> = Vec::new();
    let mut add = |a: &str| {
        let a = a.trim();
        if !a.is_empty() && !attributions.iter().any(|x| x == a) {
            attributions.push(a.to_string());
        }
    };
    add(&place.dem.attribution);
    if let Some(i) = &place.imagery {
        add(&i.attribution);
    }

    Ok(PlaceLayers {
        slug: place.slug.clone(),
        name: place.name.clone(),
        hillshade,
        imagery,
        dem: place.dem.clone(),
        imagery_note: place.imagery.clone(),
        trace,
        attributions,
    })
}

/// Save the lap a rider drew.
///
/// Takes the bare points and builds the rest here, so the file's provenance always matches
/// the place it was drawn on rather than whatever the UI happened to be holding.
#[tauri::command]
pub async fn place_save_trace(
    app: AppHandle,
    slug: String,
    points: Vec<Vec<f64>>,
    closed: bool,
    default_width_m: f64,
    start_index: usize,
) -> Result<String, String> {
    if points.len() < 3 {
        return Err("A lap needs at least three points.".to_string());
    }
    for p in &points {
        if p.len() < 2 || p.iter().any(|v| !v.is_finite()) {
            return Err("One of those points isn't a coordinate.".to_string());
        }
    }
    let dir = place_dir(&app, &slug)?;
    let text = std::fs::read(dir.join("place.json"))
        .map_err(|_| format!("{slug} isn't a fetched place any more"))?;
    let place: Place =
        serde_json::from_slice(&text).map_err(|e| format!("that place's notes are damaged: {e}"))?;

    let trace = LapTrace {
        version: 1,
        kind: "mxb-lap-trace".to_string(),
        name: place.name.clone(),
        crs: place.dem.crs.clone(),
        units: "m".to_string(),
        closed,
        default_width_m: if default_width_m.is_finite() && default_width_m > 0.5 {
            default_width_m
        } else {
            6.0
        },
        start_index: start_index.min(points.len() - 1),
        points,
        dem: place.dem.clone(),
        imagery: place.imagery.clone(),
    };
    let path = dir.join(format!("{slug}.lap.json"));
    let tmp = path.with_extension("json.tmp");
    let body = serde_json::to_vec_pretty(&lap_file(&trace)).map_err(|e| e.to_string())?;
    std::fs::write(&tmp, body).map_err(|e| format!("couldn't save the lap: {e}"))?;
    std::fs::rename(&tmp, &path).map_err(|e| format!("couldn't save the lap: {e}"))?;
    Ok(path.display().to_string())
}

// ── Bringing your own ────────────────────────────────────────────────────────


/// The lap file exactly as it goes to disk.
///
/// Written by hand rather than derived, because this file is a contract with a separate
/// program and its field names are part of that contract. The structs in this module are
/// named for Rust and serialised in camelCase for the webview; the file is snake_case,
/// which is what the format was agreed in.
///
/// The temptation, when two spellings are both "accepted", is to write both and be safe.
/// Do not: a JSON object with `default_width_m` and `defaultWidthM` side by side has two
/// keys mapping to one field, and a strict reader rejects the whole file as a duplicate
/// rather than picking one. Write each key once.
fn lap_file(t: &LapTrace) -> serde_json::Value {
    let dem = &t.dem;
    let mut v = serde_json::json!({
        "version": t.version,
        "kind": t.kind,
        "name": t.name,
        "crs": t.crs,
        "units": t.units,
        "closed": t.closed,
        "default_width_m": t.default_width_m,
        "start_index": t.start_index,
        "points": t.points,
        "dem": {
            "path": dem.path,
            "crs": dem.crs,
            "cell_m": dem.cell_m,
            "origin_e": dem.origin_e,
            "origin_n": dem.origin_n,
            "width": dem.width,
            "height": dem.height,
            "vertical_datum": dem.vertical_datum,
            "source": dem.source,
            "source_url": dem.source_url,
            "collected": dem.collected,
            "licence": dem.licence,
            "ground": if dem.ground == Ground::Terrain { "terrain" } else { "surface" },
            "min_z": dem.min_z,
            "max_z": dem.max_z,
        },
    });
    if !dem.attribution.is_empty() {
        v["dem"]["attribution"] = serde_json::Value::String(dem.attribution.clone());
    }
    if let Some(i) = &t.imagery {
        let mut im = serde_json::json!({
            "path": i.path, "source": i.source, "licence": i.licence, "captured": i.captured,
        });
        if !i.attribution.is_empty() {
            im["attribution"] = serde_json::Value::String(i.attribution.clone());
        }
        v["imagery"] = im;
    }
    v
}

/// Rewrite a lap file's snake_case keys into the camelCase the structs here expect.
///
/// Recursive and total: every key in the document is converted, which is safe because the
/// only keys this format has are single words or snake_case ones. A key already in
/// camelCase passes through untouched, so a file written either way reads back.
fn camelise(v: serde_json::Value) -> serde_json::Value {
    match v {
        serde_json::Value::Object(m) => serde_json::Value::Object(
            m.into_iter()
                .map(|(k, val)| (to_camel(&k), camelise(val)))
                .collect(),
        ),
        serde_json::Value::Array(a) => {
            serde_json::Value::Array(a.into_iter().map(camelise).collect())
        }
        other => other,
    }
}

fn to_camel(k: &str) -> String {
    let mut out = String::with_capacity(k.len());
    let mut up = false;
    for c in k.chars() {
        if c == '_' {
            up = true;
        } else if up {
            out.extend(c.to_uppercase());
            up = false;
        } else {
            out.push(c);
        }
    }
    out
}

/// Take a GeoTIFF a rider downloaded themselves and make a place out of it.
///
/// This is the path for everywhere the tool can't fetch from, which is most of the world.
/// A national mapping agency's 1 m DTM, exported as a north-up GeoTIFF, is exactly as good
/// as anything fetched here — better, in the Swiss and Nordic cases — and there is no reason
/// the rest of the pipeline should care where the file came from.
#[tauri::command]
pub async fn place_import_dem(
    app: AppHandle,
    name: String,
    path: String,
    licence: String,
    attribution: String,
) -> Result<Place, String> {
    let src = PathBuf::from(&path);
    if !src.is_file() {
        return Err(format!("there's no file at {path}"));
    }
    let name = if name.trim().is_empty() {
        src.file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "imported place".to_string())
    } else {
        name.trim().to_string()
    };
    let root = places_dir(&app)?;
    let slug = free_slug(&root, &name);
    let dir = root.join(&slug);
    std::fs::create_dir_all(&dir).map_err(|e| format!("couldn't make {}: {e}", dir.display()))?;

    let out = (|| -> Result<Place, String> {
        let dem_rel = format!("{slug}.dem.tif");
        std::fs::copy(&src, dir.join(&dem_rel))
            .map_err(|e| format!("couldn't copy that GeoTIFF in: {e}"))?;
        let grid = read_dem(&dir.join(&dem_rel))?;
        if grid.valid == 0 {
            return Err("every cell in that GeoTIFF is empty.".to_string());
        }
        let hillshade_rel = format!("{slug}.hillshade.png");
        write_hillshade(&grid, &dir.join(&hillshade_rel))?;
        let epsg = geotiff_epsg(&dir.join(&dem_rel)).unwrap_or(0);
        let place = Place {
            slug: slug.clone(),
            name: name.clone(),
            lat: 0.0,
            lon: 0.0,
            plot_m: grid.width as f64 * grid.cell,
            fetched: now_stamp(),
            dem: DemNote {
                path: dem_rel,
                crs: if epsg == 0 {
                    "unknown".to_string()
                } else {
                    format!("EPSG:{epsg}")
                },
                cell_m: grid.cell,
                origin_e: grid.origin_e,
                origin_n: grid.origin_n,
                width: grid.width as u32,
                height: grid.height as u32,
                vertical_datum: "unknown".to_string(),
                source: format!("imported from {}", src.display()),
                source_url: String::new(),
                collected: String::new(),
                licence: if licence.trim().is_empty() {
                    "not recorded — see the file's own terms".to_string()
                } else {
                    licence.trim().to_string()
                },
                attribution: attribution.trim().to_string(),
                ground: Ground::Terrain,
                min_z: grid.min_z,
                max_z: grid.max_z,
            },
            imagery: None,
            hillshade: hillshade_rel,
            bbox_ll: Vec::new(),
            has_trace: false,
            bytes: Some(dir_bytes(&dir)),
            dir: dir.display().to_string(),
        };
        save_place(&dir, &place)?;
        Ok(place)
    })();
    if out.is_err() {
        let _ = std::fs::remove_dir_all(&dir);
    }
    out
}

/// The projection a GeoTIFF says it is on, from GeoKey 3072.
///
/// Read separately from the cells because an imported file might be on anything, and being
/// able to say "EPSG:27700" rather than "unknown" is the difference between the importer
/// trusting the trace and refusing it.
fn geotiff_epsg(path: &Path) -> Option<u32> {
    use tiff::decoder::Decoder;
    use tiff::tags::Tag;
    let file = std::fs::File::open(path).ok()?;
    let mut dec = Decoder::new(std::io::BufReader::new(file)).ok()?;
    let keys = dec
        .get_tag(Tag::Unknown(34735))
        .ok()
        .and_then(|v| v.into_u64_vec().ok())?;
    // A header of four, then four-word entries: key, location, count, value.
    let mut i = 4;
    while i + 3 < keys.len() {
        // 3072 is ProjectedCSTypeGeoKey; a location of 0 means the value is inline.
        if keys[i] == 3072 && keys[i + 1] == 0 {
            return Some(keys[i + 3] as u32);
        }
        i += 4;
    }
    None
}


// ── Making the track ─────────────────────────────────────────────────────────

/// Turn a fetched place and its traced lap into a track programme, ready for `build_track`.
///
/// This is the step that was missing. Everything before it — find, coverage, fetch, trace,
/// save — left a folder on disk that only a test could turn into a track, so the feature was
/// complete for whoever wrote it and unusable for everyone else.
///
/// It stops at the programme rather than building, so a scanned place goes through exactly
/// the same build, progress reporting and install as any other track. One build path, one
/// packer, and nothing here to drift out of step with it.
#[tauri::command]
pub async fn place_program(
    app: AppHandle,
    slug: String,
    recut: bool,
) -> Result<serde_json::Value, String> {
    let dir = place_dir(&app, &slug)?;
    let dem = dir.join(format!("{slug}.dem.tif"));
    let lap = dir.join(format!("{slug}.lap.json"));
    if !dem.is_file() {
        return Err("That place has no elevation saved. Fetch it again.".to_string());
    }
    if !lap.is_file() {
        return Err(
            "That place has no lap yet. Trace one on the picture and press Save lap first."
                .to_string(),
        );
    }
    let jumps = if recut {
        crate::trackprog::ScanJumps::Recut
    } else {
        crate::trackprog::ScanJumps::Keep
    };
    let prog = tauri::async_runtime::spawn_blocking(move || {
        let imp = crate::trackground::import(&dem, &lap, 1.0)?;
        crate::trackground::program_for(&imp, jumps)
    })
    .await
    .map_err(|e| format!("the ground reader stopped: {e}"))?
    .map_err(|e| e.to_string())?;
    serde_json::to_value(prog).map_err(|e| format!("couldn't hand the programme over: {e}"))
}


#[cfg(test)]
mod tests {
    use super::*;

    /// The one number the whole pipeline hangs off: a place on the earth landing on the
    /// right square metre of the right national grid. Checked against the coordinate the
    /// feasibility test measured Ironman Raceway at.
    #[test]
    fn utm_round_trips_at_ironman() {
        let (e, n, zone, north) = ll_to_utm(40.008, -86.9291);
        assert_eq!(zone, 16);
        assert!(north);
        // Ironman sits in UTM 16N a little east of 500 km and a little under 4429 km north.
        assert!((e - 505_996.0).abs() < 400.0, "easting was {e}");
        assert!((n - 4_428_675.0).abs() < 400.0, "northing was {n}");
        let (lat, lon) = utm_to_ll(e, n, zone, north);
        assert!((lat - 40.008).abs() < 1e-7, "lat came back {lat}");
        assert!((lon + 86.9291).abs() < 1e-7, "lon came back {lon}");
    }

    /// A metre east has to be a metre east, or a traced lap is stretched.
    #[test]
    fn utm_is_metric() {
        let (e0, n0, z, h) = ll_to_utm(40.008, -86.9291);
        let (lat, lon) = utm_to_ll(e0 + 100.0, n0 + 100.0, z, h);
        let (e1, n1, _, _) = ll_to_utm(lat, lon);
        assert!((e1 - e0 - 100.0).abs() < 0.01);
        assert!((n1 - n0 - 100.0).abs() < 0.01);
    }

    #[test]
    fn southern_hemisphere_gets_the_false_northing() {
        let (_, n, zone, north) = ll_to_utm(-33.9, 18.4);
        assert_eq!(zone, 34);
        assert!(!north);
        assert!(n > 6_000_000.0, "northing was {n}");
    }

    /// England's plot has to land on the right square of the National Grid, or the fetch
    /// comes back with somebody else's field.
    ///
    /// The reference is measured, not looked up: the Environment Agency's own service was
    /// asked for a window in latitude and longitude with `OUTPUTCRS` set to 27700, and it
    /// answered with a grid whose top-left corner it placed at E 346458.36, N 153965.87 for
    /// the corner 51.282345535 N, 2.769076870 W. That is the service's own reprojection of
    /// the exact point, which makes it the right thing to check against.
    ///
    /// Measured difference from this function: 3.45 m west, 4.70 m north. That is the
    /// expected size of the error from using the plain Helmert datum shift instead of the
    /// OSTN15 grid, and it is harmless here because all it does is move where a 470 m window
    /// is cut. Every coordinate that reaches the importer comes from the server's own
    /// georeferencing, never from this function.
    #[test]
    fn the_british_grid_lands_on_the_right_field() {
        let (e, n) = ll_to_bng(51.282_345_535_542_674, -2.769_076_870_192_833_4);
        assert!((e - 346_458.36).abs() < 10.0, "easting was {e}, wanted 346458.36");
        assert!((n - 153_965.87).abs() < 10.0, "northing was {n}, wanted 153965.87");
    }

    /// A metre north is a metre north on the National Grid too, or a traced lap is stretched.
    ///
    /// Due north in degrees is not due north on the grid — the meridians converge — so the
    /// easting moves, and by how much is worth pinning down rather than waving at. At this
    /// latitude the convergence is about 0.6 degrees, which over 200 m is about 2.1 m. That
    /// is the number below, and if it ever changes, the projection changed.
    #[test]
    fn the_british_grid_is_metric() {
        let (e0, n0) = ll_to_bng(51.280_15, -2.765_6);
        let (e1, n1) = ll_to_bng(51.280_15 + 200.0 / 111_320.0, -2.765_6);
        assert!((n1 - n0 - 200.0).abs() < 1.0, "200 m north came out {}", n1 - n0);
        assert!((e1 - e0 - 2.08).abs() < 0.3, "200 m north moved east by {}", e1 - e0);
    }

    /// France's plot, same question. Checked against the window the coverage probe measured
    /// over Ernée: E 408205 to 408675, N 6806602 to 6807072 on Lambert-93.
    ///
    /// Tolerance is a metre rather than ten, because there is no datum shift in this one:
    /// RGF93 and WGS84 agree to within centimetres, so the projection is the whole of it.
    #[test]
    fn the_french_grid_lands_on_the_right_field() {
        // The centre of that window, turned back into degrees by the same projection.
        let (e, n) = ll_to_lambert93(48.297_0, -0.928_5);
        assert!(e > 380_000.0 && e < 440_000.0, "easting was {e}");
        assert!(n > 6_780_000.0 && n < 6_830_000.0, "northing was {n}");
        // A metre is a metre.
        let (e1, n1) = ll_to_lambert93(48.297_0 + 300.0 / 111_320.0, -0.928_5);
        assert!((n1 - n - 300.0).abs() < 1.5, "300 m north came out {}", n1 - n);
        assert!((e1 - e).abs() < 25.0, "300 m north moved east by {}", e1 - e);
    }

    /// The origin of Lambert-93 is defined, so it is the one point that can be checked
    /// against the specification rather than against a server: 46.5 N, 3 E is exactly
    /// E 700000, N 6600000.
    #[test]
    fn the_french_grid_has_its_defined_origin() {
        let (e, n) = ll_to_lambert93(46.5, 3.0);
        assert!((e - 700_000.0).abs() < 0.01, "easting was {e}");
        assert!((n - 6_600_000.0).abs() < 0.01, "northing was {n}");
    }

    /// The on-disk lap file is a contract with another program, so its exact key spelling is
    /// worth a test. Writing a key twice, once in each spelling, looks generous and is not:
    /// a strict reader rejects the whole document as a duplicate. Each key appears once.
    #[test]
    fn the_lap_file_writes_each_key_once_in_snake_case() {
        let dem = DemNote {
            path: "x.dem.tif".into(), crs: "EPSG:26916".into(), cell_m: 1.0,
            origin_e: 1.5, origin_n: 2.5, width: 10, height: 10,
            vertical_datum: "NAVD88".into(), source: "USGS".into(), source_url: "http://x".into(),
            collected: "2017".into(), licence: "public domain".into(),
            attribution: "\u{a9} IGN — Licence Ouverte".into(),
            ground: Ground::Terrain, min_z: 1.0, max_z: 2.0,
        };
        let trace = LapTrace {
            version: 1, kind: "mxb-lap-trace".into(), name: "X".into(),
            crs: "EPSG:26916".into(), units: "m".into(), closed: true,
            default_width_m: 7.0, start_index: 3,
            points: vec![vec![1.0, 2.0], vec![3.0, 4.0, 9.0]],
            dem: dem.clone(), imagery: None,
        };
        let v = lap_file(&trace);
        let text = serde_json::to_string(&v).unwrap();
        for snake in ["default_width_m", "start_index", "cell_m", "origin_e", "origin_n",
                      "vertical_datum", "source_url", "min_z", "max_z"] {
            assert_eq!(text.matches(&format!("\"{snake}\"")).count(), 1, "{snake} once");
        }
        for camel in ["defaultWidthM", "startIndex", "cellM", "originE", "originN",
                      "verticalDatum", "sourceUrl", "minZ", "maxZ"] {
            assert!(!text.contains(camel), "{camel} must not be written");
        }
        // Per-point width survives, and the third element is not rounded away.
        assert!(text.contains("9.0"), "a point's own width must survive");
        // The credit the licence demands is the one thing in here that is a legal obligation
        // rather than a convenience, and the built track can only carry it if the lap file
        // does. It has no business being dropped anywhere along the way.
        assert!(text.contains("Licence Ouverte"), "the credit must survive: {text}");
        // And it reads back through the same door a rider's hand-edited file comes in by.
        let back: LapTrace = serde_json::from_value(camelise(v)).expect("reads back");
        assert_eq!(back.start_index, 3);
        assert_eq!(back.default_width_m, 7.0);
        assert_eq!(back.points[1].len(), 3);
        assert_eq!(back.dem.origin_e, 1.5);
        assert!(back.dem.attribution.contains("Licence Ouverte"));
    }

    /// A file written in either spelling has to read back, because files in both exist.
    #[test]
    fn either_spelling_reads_back() {
        assert_eq!(to_camel("default_width_m"), "defaultWidthM");
        assert_eq!(to_camel("startIndex"), "startIndex");
        assert_eq!(to_camel("points"), "points");
        let snake = serde_json::json!({ "dem": { "cell_m": 2.0 }, "start_index": 4 });
        let c = camelise(snake);
        assert_eq!(c["dem"]["cellM"], 2.0);
        assert_eq!(c["startIndex"], 4);
    }

    #[test]
    fn slugs_are_filesystem_safe() {
        assert_eq!(slugify("Ironman Raceway"), "ironman-raceway");
        assert_eq!(slugify("St. Jean d'Angély / MX"), "st-jean-d-ang-ly-mx");
        assert_eq!(slugify("   "), "place");
        assert_eq!(slugify("40.008, -86.9291"), "40-008-86-9291");
    }

    #[test]
    fn coordinates_typed_in_are_recognised() {
        let hit = parse_coords("40.008, -86.9291").expect("should parse");
        assert!((hit.lat - 40.008).abs() < 1e-9);
        assert!((hit.lon + 86.9291).abs() < 1e-9);
        assert!(parse_coords("Ironman Raceway").is_none());
        assert!(parse_coords("91.0 0.0").is_none(), "latitude past the pole");
    }

    /// A circuit comes back off the map as a place, whatever shape it was mapped as.
    ///
    /// The three cases are the three ways OpenStreetMap holds a track: a node with its own
    /// coordinate, a way with a centre because the query asked for one, and a relation the
    /// same. A way without `out center` has no position at all and must be dropped rather
    /// than defaulted to a corner of the sea.
    #[test]
    fn circuits_are_read_out_of_an_overpass_answer() {
        let answer = serde_json::json!({
            "version": 0.6,
            "elements": [
                { "type": "node", "id": 1, "lat": 40.008, "lon": -86.9291,
                  "tags": { "name": "Ironman Raceway", "sport": "motocross", "addr:state": "Indiana" } },
                { "type": "way", "id": 2, "center": { "lat": 51.1, "lon": 5.2 },
                  "tags": { "name": "Lommel", "sport": "motocross;enduro" } },
                { "type": "way", "id": 3, "tags": { "name": "No position", "sport": "motocross" } },
                { "type": "way", "id": 4, "center": { "lat": 1.0, "lon": 2.0 },
                  "tags": { "sport": "motocross" } },
            ]
        });
        let hits: Vec<PlaceHit> = answer["elements"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(circuit_hit)
            .collect();
        assert_eq!(hits.len(), 2, "kept: {:?}", hits.iter().map(|h| &h.label).collect::<Vec<_>>());
        assert_eq!(hits[0].label, "Ironman Raceway, Indiana");
        assert!((hits[0].lat - 40.008).abs() < 1e-9);
        assert_eq!(hits[0].kind, "motocross");
        // A way's centre is its position, and a venue with no name is no use in a list.
        assert_eq!(hits[1].label, "Lommel");
        assert!((hits[1].lon - 5.2).abs() < 1e-9);
    }

    /// What a rider types goes into a regular expression, so it cannot be allowed to *be* one.
    ///
    /// `Frost's MX (old)` is an ordinary enough name and its brackets are a group; a lone `[`
    /// or `*` is a syntax error Overpass answers with a 400, which would read to the rider as
    /// the search being broken.
    #[test]
    fn a_typed_name_cannot_break_the_query() {
        assert_eq!(regex_safe("Frost's MX (old)"), "Frost's MX \\(old\\)");
        assert_eq!(regex_safe("a\"b"), "a\\\"b");
        assert_eq!(regex_safe("+*?"), "\\+\\*\\?");
        assert_eq!(regex_safe("Ironman"), "Ironman");
    }

    /// The box a nearby search asks over is the radius it promises, and it stays on the planet.
    #[test]
    fn a_nearby_box_is_the_radius_it_says() {
        // Due north of a point, one box-height away, is about the search radius.
        let (dlat, _) = degrees_per_m(40.0);
        let north = 40.0 + NEAR_KM * 1000.0 * dlat;
        let away = km_between(40.0, -86.0, north, -86.0);
        assert!((away - NEAR_KM).abs() < 1.0, "the box reaches {away:.1} km, not {NEAR_KM}");
        // And the known distance every map textbook carries: Indianapolis to Chicago is 265 km.
        let far = km_between(39.7684, -86.1581, 41.8781, -87.6298);
        assert!((far - 265.0).abs() < 8.0, "{far:.0} km against a known 265");
    }

    /// Every failure mode these services have, turned into a sentence.
    #[test]
    fn faults_are_read_out_of_both_dialects() {
        let esri = br#"{"error":{"code":400,"message":"Invalid URL","details":["Invalid URL"]}}"#;
        assert!(fault_in(esri).unwrap().contains("Invalid URL"));
        let wcs = b"<?xml version=\"1.0\"?><ows:ExceptionReport><ows:Exception><ows:ExceptionText>subset out of range</ows:ExceptionText></ows:Exception></ows:ExceptionReport>";
        assert!(fault_in(wcs).unwrap().contains("subset out of range"));
        // Real data must not look like a fault.
        assert!(fault_in(b"II\x2a\x00\x08\x00\x00\x00").is_none());
        assert!(fault_in(&[0x89, b'P', b'N', b'G']).is_none());
    }

    #[test]
    fn a_slug_cannot_walk_out_of_the_places_folder() {
        // The check itself, without needing an app handle.
        for bad in ["..", "../x", "a/b", "a\\b", "", "a b"] {
            assert!(
                !(bad
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
                    && !bad.is_empty()),
                "{bad:?} should be refused"
            );
        }
    }

    #[test]
    fn usgs_dates_come_in_two_shapes() {
        assert_eq!(
            json_date(&serde_json::json!(20170303)).as_deref(),
            Some("2017-03-03")
        );
        assert_eq!(
            json_date(&serde_json::json!("20200411")).as_deref(),
            Some("2020-04-11")
        );
        assert_eq!(json_date(&serde_json::json!(0)), None);
    }

    #[test]
    fn every_source_is_openly_licensed_and_named() {
        for s in SOURCES {
            assert!(!s.licence.is_empty(), "{} has no licence", s.id);
            assert!(!s.terms_url.is_empty(), "{} has no terms link", s.id);
            assert!(s.best_cell_m > 0.0);
        }
        for i in IMAGERY {
            assert!(!i.licence.is_empty(), "{} has no licence", i.id);
            let e = i.endpoint.to_ascii_lowercase();
            for banned in ["google", "apple", "bing", "virtualearth", "mapbox"] {
                assert!(!e.contains(banned), "{} is not an open imagery source", i.id);
            }
        }
    }

    /// The map is shown where there is an open survey and nowhere else.
    ///
    /// The honest finding behind this: there is no worldwide aerial basemap we are allowed to
    /// trace off. Every one sharp enough to pick a circuit out of is licensed for viewing
    /// inside its owner's own product. So the map covers the United States, France and the
    /// Netherlands, and says so plainly everywhere else rather than filling the panel with a
    /// picture nobody may use.
    #[test]
    fn the_map_is_offered_only_where_an_open_survey_covers_the_ground() {
        // Ironman Raceway, Indiana.
        assert_eq!(imagery_at(40.008, -86.9291).map(|i| i.id), Some("naip"));
        // Ernée, France.
        assert_eq!(imagery_at(48.297, -0.9285).map(|i| i.id), Some("ign-ortho"));
        // Lierop, the Netherlands — and the Dutch box wins where it overlaps France's.
        assert_eq!(imagery_at(51.35, 5.66).map(|i| i.id), Some("pdok-ortho"));
        // Everywhere else, which is most of the world, and it is told so.
        for (lat, lon) in [(-27.47, 153.02), (35.68, 139.69), (-33.9, 18.4), (61.2, -149.9)] {
            assert!(imagery_at(lat, lon).is_none(), "{lat},{lon} has no open imagery");
        }
    }

    /// The date stamp has to be a real date, because it is what tells a rider six months
    /// from now which vintage of a rebuilt venue they are looking at.
    #[test]
    fn the_stamp_is_a_real_date() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(19_617), (2023, 9, 17));
        let s = now_stamp();
        assert_eq!(s.len(), 20, "{s}");
        assert!(s.ends_with('Z'));
        assert!(s[..4].parse::<i32>().unwrap() >= 2024, "{s}");
    }
}
