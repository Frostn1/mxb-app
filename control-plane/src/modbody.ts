/**
 * A mirrored post's own words and pictures, out of its rendered page.
 *
 * The page is someone else's HTML, so none of it is ever stored or served as HTML. What comes
 * out is data: text blocks with three marks (bold, italic, a link), YouTube embeds as a video
 * id, and the post's content images as source URLs for the sync to copy into R2
 * (`img/<sha256>.<ext>` on cdn.mxbsecure.com). The site renders the blocks as elements.
 *
 * A Worker has no DOM, so this is a small tokenizer, not a parser: tags open and close blocks
 * and marks, everything it does not know is dropped, and its text kept.
 */

import { decodeEntities } from "./trackcatalog";

/** One run of text. `a` is an http(s) link. */
export interface Inline {
  x: string;
  b?: 1;
  i?: 1;
  a?: string;
}

export type Block =
  | { t: "p" | "h" | "li"; c: Inline[] }
  | { t: "video"; yt: string };

export interface PostImage {
  src: string;
  width: number | null;
  height: number | null;
}

export interface PostBody {
  blocks: Block[];
  images: PostImage[];
}

/** Content images copied per post, and the largest one copied. */
export const MAX_IMAGES = 12;
export const MAX_IMAGE_BYTES = 3 * 1024 * 1024;
/** The widest copy asked for, out of a `srcset`. */
const MAX_IMAGE_W = 1280;
const MAX_BLOCKS = 120;
const MAX_TEXT = 8000;
const MAX_LINK = 500;

/** Where a post's pictures may be copied from: the site itself and the hosts its authors use. */
const IMAGE_HOSTS = [
  "mxb-mods.com",
  "cdn.mxb-mods.com",
  "github.com",
  "i.imgur.com",
  "cdn.discordapp.com",
  "media.discordapp.net",
];
const IMAGE_HOST_SUFFIXES = [".githubusercontent.com"];

export function imageHostAllowed(host: string): boolean {
  const h = host.toLowerCase().replace(/^www\./, "");
  return IMAGE_HOSTS.includes(h) || IMAGE_HOST_SUFFIXES.some((s) => h.endsWith(s));
}

/** The site's own hosts: a refusal from these is the site saying no, and the sync backs off. */
export function isModsHost(host: string): boolean {
  const h = host.toLowerCase().replace(/^www\./, "");
  return h === "mxb-mods.com" || h === "cdn.mxb-mods.com";
}

/**
 * The post's content, from the theme's `entry-content` to where the theme's own blocks start
 * (track info, downloads, instructions). Never the header image, the sidebar or related posts.
 */
export function contentRegion(html: string): string {
  const open = /<div\b[^>]*class="[^"]*\bentry-content\b[^"]*"[^>]*>/i.exec(html);
  if (!open) return "";
  const start = open.index + open[0].length;
  const rest = html.slice(start);
  const ends = [
    /<div\b[^>]*class="stuff"/i,
    /<div\b[^>]*class="download"/i,
    /<p\b[^>]*id=['"]download['"]/i,
    /<div\b[^>]*id="instructions"/i,
    /<!--\s*\.post-content\s*-->/i,
  ]
    .map((re) => re.exec(rest)?.index ?? -1)
    .filter((i) => i >= 0);
  const end = ends.length ? Math.min(...ends) : Math.min(rest.length, 200_000);
  return rest.slice(0, end);
}

function attrs(raw: string): Map<string, string> {
  const out = new Map<string, string>();
  const re = /([a-zA-Z_:][-a-zA-Z0-9_:.]*)\s*(?:=\s*(?:"([^"]*)"|'([^']*)'|([^\s"'>]+)))?/g;
  for (let m; (m = re.exec(raw)); ) {
    const name = m[1].toLowerCase();
    if (!out.has(name)) out.set(name, decodeEntities(m[2] ?? m[3] ?? m[4] ?? ""));
  }
  return out;
}

/** An address on the open web, upgraded to https on the site itself. Null for anything else. */
export function webUrl(raw: string, base = "https://mxb-mods.com/"): URL | null {
  const s = raw.trim();
  if (!s || /^(data|javascript|vbscript|blob|file):/i.test(s)) return null;
  let u: URL;
  try {
    u = new URL(s, base);
  } catch {
    return null;
  }
  if (u.protocol === "http:" && isModsHost(u.hostname)) u.protocol = "https:";
  if (u.protocol !== "https:" && u.protocol !== "http:") return null;
  if (u.username || u.password) return null;
  return u;
}

function num(v: string | undefined): number | null {
  const n = Number(v);
  return Number.isFinite(n) && n > 0 && n < 100_000 ? Math.round(n) : null;
}

/**
 * The copy of an `<img>` worth keeping: out of its `srcset`, the widest at most 1280 wide (the
 * smallest above that when there is none), else its `src`. Lazy loaders' `data-` names too.
 */
export function pickImage(a: Map<string, string>): PostImage | null {
  const w = num(a.get("width"));
  const h = num(a.get("height"));
  // Smileys, spacers and icons are not the post's pictures.
  if ((w !== null && w < 48) || (h !== null && h < 48)) return null;
  const set = a.get("srcset") || a.get("data-srcset") || a.get("data-lazy-srcset") || "";
  const cands = set
    .split(/,\s+/)
    .map((c) => /^(\S+)\s+(\d+)w$/.exec(c.trim()))
    .filter((m): m is RegExpExecArray => !!m)
    .map((m) => ({ url: m[1], w: Number(m[2]) }));
  let chosen: { url: string; w: number | null } | null = null;
  if (cands.length) {
    const fit = cands.filter((c) => c.w <= MAX_IMAGE_W).sort((x, y) => y.w - x.w)[0];
    const over = cands.filter((c) => c.w > MAX_IMAGE_W).sort((x, y) => x.w - y.w)[0];
    const c = fit && fit.w >= 300 ? fit : over ?? fit;
    if (c) chosen = c;
  }
  const plain = [a.get("data-src"), a.get("data-lazy-src"), a.get("src")].find((s) => s && !s.startsWith("data:"));
  if (!chosen && plain) chosen = { url: plain, w: null };
  if (!chosen) return null;
  // An absolute address, or one from the site's root; nothing relative to who knows what.
  if (!/^(https?:)?\/\//i.test(chosen.url.trim()) && !chosen.url.trim().startsWith("/")) return null;
  const u = webUrl(chosen.url);
  if (!u || u.protocol !== "https:" || !imageHostAllowed(u.hostname)) return null;
  if (/\/(wp-includes|plugins)\/|emoji/i.test(u.pathname)) return null;
  // WordPress names its sizes `-1280x720`: the truest dimensions there are.
  const named = /-(\d{2,5})x(\d{2,5})\.[a-z0-9]+$/i.exec(u.pathname);
  const width = named ? Number(named[1]) : (chosen.w ?? w);
  const height = named ? Number(named[2]) : chosen.w && w && h ? Math.round((chosen.w * h) / w) : h;
  return { src: u.toString(), width, height };
}

/** The same picture at another size (`name-1280x720.webp` and `name.webp`) is one picture. */
function imageIdentity(src: string): string {
  return src.replace(/^https?:\/\/(cdn\.)?/, "").replace(/-\d+x\d+(\.[a-z0-9]+)$/i, "$1").toLowerCase();
}

export function youtubeId(src: string): string | null {
  const m = /^https?:\/\/(?:www\.)?(?:youtube(?:-nocookie)?\.com\/embed\/|youtu\.be\/)([A-Za-z0-9_-]{11})/.exec(src.trim());
  return m ? m[1] : null;
}

const SKIP = new Set([
  "script", "style", "noscript", "svg", "form", "button", "select", "textarea", "template", "object", "head", "nav",
]);
const BLOCK = new Set([
  "p", "div", "section", "article", "blockquote", "figure", "figcaption", "ul", "ol", "table", "tr", "td", "th",
  "pre", "center", "dl", "dt", "dd", "hr",
]);
const HEADING = /^h[1-6]$/;

/** Words and pictures out of the post's content region (`contentRegion`). */
export function parseBody(region: string): PostBody {
  const blocks: Block[] = [];
  const images: PostImage[] = [];
  const seen = new Set<string>();
  let cur: { t: "p" | "h" | "li"; c: Inline[] } = { t: "p", c: [] };
  let bold = 0;
  let italic = 0;
  const links: (string | null)[] = [];
  let textBudget = MAX_TEXT;
  /** A skipped element and how deep inside more of the same tag we are. */
  let skip: { tag: string; depth: number } | null = null;
  let firstHeading = true;

  const flush = (next: "p" | "h" | "li" = "p") => {
    // Runs with the same marks are one run; space at the ends is trimmed.
    const merged: Inline[] = [];
    for (const r of cur.c) {
      const last = merged[merged.length - 1];
      if (last && last.b === r.b && last.i === r.i && last.a === r.a) last.x += r.x;
      else merged.push({ ...r });
    }
    while (merged.length && !merged[0].x.trim()) merged.shift();
    while (merged.length && !merged[merged.length - 1].x.trim()) merged.pop();
    if (merged.length) {
      merged[0].x = merged[0].x.replace(/^[ \t]+/, "");
      merged[merged.length - 1].x = merged[merged.length - 1].x.replace(/[ \t]+$/, "");
      const text = merged.map((r) => r.x).join("").trim();
      // The theme heads every post with "Description"; the mod page has its own heading.
      const themeHeading = cur.t === "h" && firstHeading && /^description$/i.test(text);
      if (cur.t === "h") firstHeading = false;
      if (!themeHeading && blocks.length < MAX_BLOCKS) blocks.push({ t: cur.t, c: merged });
    }
    cur = { t: next, c: [] };
  };

  const TOKEN = /<!--[\s\S]*?-->|<(\/?)([a-zA-Z][a-zA-Z0-9]*)\b((?:[^>"']|"[^"]*"|'[^']*')*)\/?>|([^<]+)|</g;
  for (let m; (m = TOKEN.exec(region)); ) {
    const [whole, close, rawTag, rawAttrs, text] = m;
    if (whole.startsWith("<!--")) continue;
    const tag = rawTag?.toLowerCase();

    if (skip) {
      if (tag === skip.tag && !whole.endsWith("/>")) {
        skip.depth += close ? -1 : 1;
        if (skip.depth === 0) skip = null;
      }
      continue;
    }

    if (text !== undefined || whole === "<") {
      const t = decodeEntities(text ?? "<").replace(/\s+/g, " ");
      if (!t || textBudget <= 0) continue;
      const x = t.slice(0, textBudget);
      textBudget -= x.length;
      const run: Inline = { x };
      if (bold > 0) run.b = 1;
      if (italic > 0) run.i = 1;
      const href = links[links.length - 1];
      if (href) run.a = href;
      cur.c.push(run);
      continue;
    }
    if (!tag) continue;
    const a = close ? new Map<string, string>() : attrs(rawAttrs ?? "");

    if (!close && SKIP.has(tag)) {
      if (!whole.endsWith("/>")) skip = { tag, depth: 1 };
      continue;
    }
    // The theme's "Downloads" jump link.
    if (!close && tag === "div" && /\bquick-links\b/.test(a.get("class") ?? "")) {
      skip = { tag, depth: 1 };
      continue;
    }

    if (tag === "img" && !close) {
      const img = pickImage(a);
      if (img && images.length < MAX_IMAGES) {
        const id = imageIdentity(img.src);
        if (!seen.has(id)) {
          seen.add(id);
          images.push(img);
        }
      }
      continue;
    }
    if (tag === "iframe" && !close) {
      const yt = youtubeId(a.get("src") ?? a.get("data-src") ?? "");
      if (yt && blocks.length < MAX_BLOCKS) {
        flush(cur.t);
        if (!blocks.some((b) => b.t === "video" && b.yt === yt)) blocks.push({ t: "video", yt });
      }
      skip = { tag, depth: 1 };
      continue;
    }
    if (tag === "br") {
      cur.c.push({ x: "\n", ...(bold > 0 ? { b: 1 as const } : {}), ...(italic > 0 ? { i: 1 as const } : {}) });
      continue;
    }
    if (tag === "b" || tag === "strong") {
      bold = Math.max(0, bold + (close ? -1 : 1));
      continue;
    }
    if (tag === "i" || tag === "em") {
      italic = Math.max(0, italic + (close ? -1 : 1));
      continue;
    }
    if (tag === "a") {
      if (close) links.pop();
      else {
        const u = webUrl(a.get("href") ?? "");
        const href = u && !/^#/.test(a.get("href") ?? "") ? u.toString() : null;
        links.push(href && href.length <= MAX_LINK ? href : null);
      }
      continue;
    }
    if (HEADING.test(tag)) {
      flush(close ? "p" : "h");
      continue;
    }
    if (tag === "li") {
      flush(close ? "p" : "li");
      continue;
    }
    if (BLOCK.has(tag)) {
      flush("p");
      continue;
    }
    // Anything else (span, u, small, font …): its text stays, the tag goes.
  }
  flush();
  return { blocks, images };
}

/** A post page's body: `contentRegion`, then `parseBody`. */
export function parsePostBody(html: string): PostBody {
  return parseBody(contentRegion(html));
}

/**
 * Stored blocks, checked again on the way out: only the shapes above, only http(s) links,
 * only YouTube ids. Whatever is in the column, the API hands on nothing else.
 */
export function safeBlocks(raw: unknown): Block[] {
  let v: unknown = raw;
  if (typeof raw === "string") {
    try {
      v = JSON.parse(raw);
    } catch {
      return [];
    }
  }
  if (!Array.isArray(v)) return [];
  const out: Block[] = [];
  for (const b of v.slice(0, MAX_BLOCKS)) {
    if (!b || typeof b !== "object") continue;
    const t = (b as { t?: unknown }).t;
    if (t === "video") {
      const yt = (b as { yt?: unknown }).yt;
      if (typeof yt === "string" && /^[A-Za-z0-9_-]{11}$/.test(yt)) out.push({ t, yt });
      continue;
    }
    if (t !== "p" && t !== "h" && t !== "li") continue;
    const c = (b as { c?: unknown }).c;
    if (!Array.isArray(c)) continue;
    const runs: Inline[] = [];
    for (const r of c.slice(0, 200)) {
      if (!r || typeof r !== "object" || typeof (r as Inline).x !== "string") continue;
      const run: Inline = { x: (r as Inline).x.slice(0, MAX_TEXT) };
      if ((r as Inline).b === 1) run.b = 1;
      if ((r as Inline).i === 1) run.i = 1;
      const href = (r as Inline).a;
      if (typeof href === "string" && href.length <= MAX_LINK && /^https?:\/\//i.test(href)) {
        const u = webUrl(href);
        if (u) run.a = u.toString();
      }
      runs.push(run);
    }
    if (runs.length) out.push({ t, c: runs });
  }
  return out;
}
