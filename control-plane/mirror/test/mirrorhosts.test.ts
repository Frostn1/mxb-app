import { describe, expect, it } from "vitest";
import {
  HostError,
  RunnerNeeded,
  aesEcbDecrypt,
  b64urlDecode,
  dropboxDirect,
  filenameFrom,
  gdriveDirect,
  gdrivePageError,
  hostKind,
  isGdriveFolder,
  isMediafireDirect,
  mediafireFolderKey,
  mediafireQuickKey,
  mediafireRefusal,
  megaDecryptStream,
  megaFileKey,
  openBody,
  parseGdriveConfirm,
  parseGdriveFolder,
  parseMediafireLink,
  parseMegaLink,
  resolveShare,
} from "../../src/mirrorhosts";
import { fakeFetch, fixture } from "../../test/modfakes";

const json = (body: unknown) => new Response(JSON.stringify(body), { headers: { "content-type": "application/json" } });
const html = (body: string) => new Response(body, { headers: { "content-type": "text/html; charset=utf-8" } });

describe("host detection", () => {
  it("names every host the catalogue links to", () => {
    expect(hostKind("https://www.mediafire.com/file/abc/x.zip/file")).toBe("mediafire");
    expect(hostKind("https://drive.google.com/file/d/X/view")).toBe("gdrive");
    expect(hostKind("https://drive.usercontent.google.com/download?id=X")).toBe("gdrive");
    expect(hostKind("https://mega.nz/file/abc#key")).toBe("mega");
    expect(hostKind("https://mega.co.nz/#!abc!key")).toBe("mega");
    expect(hostKind("https://dl.dropbox.com/scl/fi/x/y.zip?dl=0")).toBe("dropbox");
    expect(hostKind("https://1drv.ms/u/s!abc")).toBe("onedrive");
    expect(hostKind("https://pixeldrain.com/u/abc")).toBe("pixeldrain");
    expect(hostKind("https://drive.proton.me/urls/X#Y")).toBe("proton");
    expect(hostKind("https://mxb-mods.com/wp-content/uploads/2023/08/x.pnt")).toBe("direct");
    expect(hostKind("https://cdn.discordapp.com/attachments/1/2/x.zip")).toBe("direct");
  });

  it("refuses links that are pages, not files, without fetching them", async () => {
    const f = fakeFetch([]);
    for (const u of ["https://paypal.me/someone", "https://discord.gg/abc", "https://youtu.be/x", "https://shop.myshopify.com/p"]) {
      await expect(resolveShare(u, f)).rejects.toMatchObject({ permanent: true });
    }
    expect(f.calls).toEqual([]);
  });

  it("refuses Proton Drive, which is end-to-end encrypted", async () => {
    await expect(resolveShare("https://drive.proton.me/urls/ABC#key", fakeFetch([]))).rejects.toBeInstanceOf(HostError);
  });
});

describe("MediaFire (the app's install.rs tests, ported)", () => {
  it("takes the link out of the scrambled attribute", () => {
    const url = "https://download2.mediafire.com/abc/track.zip";
    expect(parseMediafireLink(`<a data-scrambled-url="${btoa(url)}" href="#">x</a>`)).toBe(url);
  });
  it("survives attribute order", () => {
    const h = `<a href="https://download1234.mediafire.com/xyz/bike.zip" aria-label="Download file" id="downloadButton">go</a>`;
    expect(parseMediafireLink(h)).toBe("https://download1234.mediafire.com/xyz/bike.zip");
  });
  it("reads a protocol-relative button", () => {
    expect(parseMediafireLink(`<a id="downloadButton" href="//download99.mediafire.com/q/track.rar">go</a>`)).toBe(
      "https://download99.mediafire.com/q/track.rar",
    );
  });
  it("does not take a placeholder href for a link", () => {
    expect(parseMediafireLink(`<a id="downloadButton" href="javascript:void(0)">go</a>`)).toBeNull();
  });
  it("finds a scrambled URL assigned in a script", () => {
    const url = "https://download7.mediafire.com/a/b.zip";
    expect(parseMediafireLink(`<script>var scrambled_url = "${btoa(url)}";</script>`)).toBe(url);
  });
  it("finds the CDN link in page source, unnumbered host and escaped slashes too", () => {
    expect(parseMediafireLink(`<script>u="https:\\/\\/download.mediafire.com\\/ab\\/cd\\/bike.zip"</script>`)).toBe(
      "https://download.mediafire.com/ab/cd/bike.zip",
    );
  });
  it("parses the recorded file page", () => {
    const link = parseMediafireLink(fixture("mediafire-file.html"));
    expect(link).toMatch(/^https:\/\/download850\.mediafire\.com\/.+\/6hilor1pbmga286\/metals\.tga$/);
  });
  it("pulls the quick key out of every share shape", () => {
    expect(mediafireQuickKey("https://www.mediafire.com/file/bqmw1tdd7yq3qzr/I40_MX.pkz/file")).toBe("bqmw1tdd7yq3qzr");
    expect(mediafireQuickKey("https://www.mediafire.com/download/bqmw1tdd7yq3qzr")).toBe("bqmw1tdd7yq3qzr");
    expect(mediafireQuickKey("https://www.mediafire.com/?bqmw1tdd7yq")).toBe("bqmw1tdd7yq");
  });
  it("recognises folder links", () => {
    expect(mediafireFolderKey("https://www.mediafire.com/folder/9dhrz4bkzcnzo/I40")).toBe("9dhrz4bkzcnzo");
    expect(mediafireFolderKey("https://www.mediafire.com/?sharekey=abc123")).toBe("abc123");
    expect(mediafireFolderKey("https://www.mediafire.com/file/abc/x.zip/file")).toBeNull();
  });
  it("needs no resolving for a CDN link", () => {
    expect(isMediafireDirect("https://download2202.mediafire.com/abc/track.pkz")).toBe(true);
    expect(isMediafireDirect("https://www.mediafire.com/file/abc/track.pkz/file")).toBe(false);
  });
  it("turns refusals into permanent or retryable errors", () => {
    expect(mediafireRefusal("Invalid or Deleted File.")?.permanent).toBe(true);
    expect(mediafireRefusal("Please enter password to access")?.permanent).toBe(true);
    expect(mediafireRefusal("this file has exceeded its daily download limit")?.permanent).toBe(false);
    expect(mediafireRefusal("all good")).toBeNull();
  });

  it("expands a recorded folder into one file per entry, with sizes", async () => {
    const f = fakeFetch([
      [/content_type=files/, () => new Response(fixture("mediafire-folder-files.json"))],
      [/content_type=folders/, () => new Response(fixture("mediafire-folder-folders.json"))],
    ]);
    const r = await resolveShare("https://www.mediafire.com/folder/z47z7eaf2rmm3/publicyz450", f);
    expect(r.kind).toBe("folder");
    if (r.kind !== "folder") return;
    expect(r.name).toBe("publicyz450");
    expect(r.files.map((x) => x.rel)).toEqual(["metals.tga", "metals_n.tga", "plastics.tga", "plastics_n.tga"]);
    expect(r.files[0]).toEqual({
      rel: "metals.tga",
      url: "https://www.mediafire.com/file/6hilor1pbmga286/metals.tga/file",
      size: 50331692,
    });
  });

  it("walks sub-folders and prefixes their paths", async () => {
    const f = fakeFetch([
      [/folder_key=root&content_type=files/, () => json({ response: { result: "Success", folder_content: { files: [] } } })],
      [
        /folder_key=root&content_type=folders/,
        () => json({ response: { result: "Success", folder_content: { folders: [{ folderkey: "sub", name: "KTM" }] } } }),
      ],
      [
        /folder_key=sub&content_type=files/,
        () =>
          json({
            response: {
              result: "Success",
              folder_content: { files: [{ filename: "a.pnt", size: "10", links: { normal_download: "https://www.mediafire.com/file/k/a.pnt/file" } }] },
            },
          }),
      ],
      [/folder_key=sub&content_type=folders/, () => json({ response: { result: "Success", folder_content: { folders: [] } } })],
    ]);
    const r = await resolveShare("https://www.mediafire.com/folder/root/Pack", f);
    expect(r).toEqual({ kind: "folder", name: "Pack", files: [{ rel: "KTM/a.pnt", url: "https://www.mediafire.com/file/k/a.pnt/file", size: 10 }] });
  });

  it("resolves a file page to its CDN link, with the API as fallback", async () => {
    const page = fakeFetch([[/www\.mediafire\.com\/file/, () => html(fixture("mediafire-file.html"))]]);
    const r = await resolveShare("https://www.mediafire.com/file/6hilor1pbmga286/metals.tga/file", page);
    expect(r.kind === "file" && isMediafireDirect(r.url)).toBe(true);

    const api = fakeFetch([
      [/www\.mediafire\.com\/file/, () => html("<html>nothing here</html>")],
      [/get_links/, () => json({ response: { result: "Success", links: [{ direct_download: "https://download9.mediafire.com/x/y.zip" }] } })],
    ]);
    expect(await resolveShare("https://www.mediafire.com/file/bqmw1tdd7yq3qzr/y.zip/file", api)).toEqual({
      kind: "file",
      url: "https://download9.mediafire.com/x/y.zip",
    });
  });
});

describe("Google Drive (the app's install.rs tests, ported)", () => {
  it("builds the usercontent link from every id shape", () => {
    expect(gdriveDirect("https://drive.google.com/file/d/ABC123_xyz/view?usp=sharing")).toBe(
      "https://drive.usercontent.google.com/download?id=ABC123_xyz&export=download",
    );
    expect(gdriveDirect("https://drive.google.com/open?id=QQ-1")).toBe("https://drive.usercontent.google.com/download?id=QQ-1&export=download");
  });
  it("tells folders from files, including the /u/0/ and entity-mangled forms the catalogue has", () => {
    expect(isGdriveFolder("https://drive.google.com/drive/folders/1vYkgITTCU8hXhu1yBgfsLhyvXfnlG2Ln")).toBe(true);
    expect(isGdriveFolder("https://drive.google.com/drive/u/0/folders/1nsUd4fUZJwS1jH3LQLdRJQ9UhhtbVLK_")).toBe(true);
    expect(isGdriveFolder("https://drive.google.com/file/d/X/view")).toBe(false);
  });
  it("reads the virus-scan form", () => {
    const page = `<html><title>Google Drive - Virus scan warning</title><form id="download-form" action="https://drive.usercontent.google.com/download" method="get">
      <input type="hidden" name="id" value="ABC"><input type="hidden" name="export" value="download">
      <input type="hidden" name="confirm" value="t"><input type="hidden" name="uuid" value="u-1"></form></html>`;
    const u = new URL(parseGdriveConfirm(page)!);
    expect(u.origin + u.pathname).toBe("https://drive.usercontent.google.com/download");
    expect(Object.fromEntries(u.searchParams)).toEqual({ id: "ABC", export: "download", confirm: "t", uuid: "u-1" });
  });
  it("names quota, permission and missing pages", () => {
    expect(gdrivePageError("<title>Google Drive - Quota exceeded</title>")?.permanent).toBe(false);
    expect(gdrivePageError("<title>Google Drive - Access Denied</title>")?.permanent).toBe(true);
    expect(gdrivePageError("<title>Google Drive - Not Found</title>")?.permanent).toBe(true);
  });
  it("parses the recorded folder page", () => {
    const entries = parseGdriveFolder(fixture("gdrive-folder.html"), "1yo91EdY-Er74kkYxNlwjq7q3xyVNzSJ5");
    expect(entries).toEqual([
      { id: "1i5R9MyH1ob2VMn1unOBPFkpdF6ZAiWpl", name: "!BD Valentine Husky Pub.pnt", mime: "application/octet-stream" },
    ]);
  });
  it("walks a recorded folder of folders", async () => {
    const f = fakeFetch([
      [
        /drive\.google\.com\/drive\/folders\/([A-Za-z0-9_-]+)$/,
        (req) => html(fixture(`gdrive-nested-${new URL(req.url).pathname.split("/").pop()}.html`)),
      ],
    ]);
    const r = await resolveShare("https://drive.google.com/drive/u/0/folders/1nsUd4fUZJwS1jH3LQLdRJQ9UhhtbVLK_", f);
    expect(r.kind).toBe("folder");
    if (r.kind !== "folder") return;
    expect(r.name).toBe("phony sets");
    expect(r.files).toHaveLength(11);
    expect(r.files.every((x) => x.rel.includes("/") && x.rel.endsWith(".pnt"))).toBe(true);
    expect(r.files.find((x) => x.rel === "armor set/Phony armor.pnt")?.url).toBe(
      "https://drive.google.com/file/d/1VCxHzPhhGFW2eVhXfu0LE1F_CzARSey5/view",
    );
  });
  it("says a private folder is private", async () => {
    const f = fakeFetch([[/folders/, () => html("<html><title>Google Drive: Sign-in</title></html>")]]);
    await expect(resolveShare("https://drive.google.com/drive/folders/1abcdefghijklmnopqrstu", f)).rejects.toThrow(/isn't shared publicly/);
  });
  it("submits the virus-scan form and streams the file", async () => {
    const f = fakeFetch([
      [
        /download\?id=BIG&export=download$/,
        () =>
          html(`<title>Google Drive - Virus scan warning</title><form action="https://drive.usercontent.google.com/download">
            <input type="hidden" name="id" value="BIG"><input type="hidden" name="confirm" value="t"></form>`),
      ],
      [/confirm=t/, () => new Response(new Uint8Array([0x50, 0x4b, 3, 4]), { headers: { "content-type": "application/octet-stream" } })],
    ]);
    const res = await openBody(f, gdriveDirect("https://drive.google.com/file/d/BIG/view"));
    expect(new Uint8Array(await res.arrayBuffer())).toEqual(new Uint8Array([0x50, 0x4b, 3, 4]));
  });
  it("a quota page after the form is a retry, not a failure", async () => {
    const f = fakeFetch([[/drive\.usercontent/, () => html("<title>Google Drive - Quota exceeded</title>")]]);
    await expect(openBody(f, gdriveDirect("https://drive.google.com/file/d/Q/view"))).rejects.toMatchObject({ permanent: false });
  });
});

describe("Dropbox, OneDrive, Pixeldrain", () => {
  it("asks Dropbox for the bytes (or the folder as a zip)", () => {
    expect(dropboxDirect("https://www.dropbox.com/scl/fi/x/Pack.zip?rlkey=k&st=s&dl=0")).toBe(
      "https://www.dropbox.com/scl/fi/x/Pack.zip?rlkey=k&st=s&dl=1",
    );
    expect(dropboxDirect("https://www.dropbox.com/scl/fo/abc/def?rlkey=k&dl=0")).toContain("dl=1");
    expect(dropboxDirect("https://www.dropbox.com/s/abc/x.zip?raw=1")).toBe("https://www.dropbox.com/s/abc/x.zip?dl=1");
  });
  it("leaves personal OneDrive links that only open in a browser to the runner", async () => {
    const f = fakeFetch([[/api\.onedrive\.com/, () => json({ error: { code: "itemNotFound" } })]]);
    await expect(resolveShare("https://1drv.ms/u/s!AkCetHIg0Bwoi16ffIbpK_wyp8wW?e=1pgkPr", f)).rejects.toBeInstanceOf(RunnerNeeded);
  });
  it("lists a OneDrive folder through the shares API", async () => {
    const f = fakeFetch([
      [
        /api\.onedrive\.com\/v1\.0\/shares\/u!/,
        () =>
          json({
            name: "Pack",
            folder: {},
            children: [{ name: "a.pkz", size: 5, file: {}, "@content.downloadUrl": "https://x.example/a" }, { name: "sub", folder: {} }],
          }),
      ],
    ]);
    expect(await resolveShare("https://1drv.ms/f/s!abc", f)).toEqual({
      kind: "folder",
      name: "Pack",
      files: [{ rel: "a.pkz", url: "https://x.example/a", size: 5 }],
    });
  });
  it("uses Pixeldrain's API for files and lists", async () => {
    expect(await resolveShare("https://pixeldrain.com/u/abcd1234", fakeFetch([]))).toEqual({
      kind: "file",
      url: "https://pixeldrain.com/api/file/abcd1234?download",
    });
    const f = fakeFetch([[/api\/list\/L1/, () => json({ title: "Kit", files: [{ id: "f1", name: "a.pnt", size: 3 }] })]]);
    expect(await resolveShare("https://pixeldrain.com/l/L1", f)).toEqual({
      kind: "folder",
      name: "Kit",
      files: [{ rel: "a.pnt", url: "https://pixeldrain.com/api/file/f1?download", size: 3 }],
    });
  });
});

describe("MEGA", () => {
  it("reads every link shape", () => {
    expect(parseMegaLink("https://mega.nz/file/bJIkiSrS#wQBJ")).toEqual({ kind: "file", handle: "bJIkiSrS", key: "wQBJ" });
    expect(parseMegaLink("https://mega.nz/folder/z3ZVXBjb#8D-NY0nOo7In1Re_m-OeOA")).toEqual({
      kind: "folder",
      handle: "z3ZVXBjb",
      key: "8D-NY0nOo7In1Re_m-OeOA",
      node: null,
    });
    expect(parseMegaLink("https://mega.nz/folder/z3ZVXBjb#8D-NY0nOo7In1Re_m-OeOA/file/SmhWWZLA")?.kind).toBe("folder");
    expect(parseMegaLink("https://mega.nz/#!abc!def")).toEqual({ kind: "file", handle: "abc", key: "def" });
    expect(parseMegaLink("https://mega.nz/#F!abc!def")).toEqual({ kind: "folder", handle: "abc", key: "def", node: null });
  });

  it("decrypts the recorded folder listing's names with the link's key", async () => {
    const f = fakeFetch([[/g\.api\.mega\.co\.nz\/cs\?id=\d+&n=z3ZVXBjb/, () => new Response(fixture("mega-folder.json"))]]);
    const r = await resolveShare("https://mega.nz/folder/z3ZVXBjb#8D-NY0nOo7In1Re_m-OeOA", f);
    expect(r.kind).toBe("folder");
    if (r.kind !== "folder") return;
    expect(r.name).toBe("Oakley Goggles Pack");
    expect(r.files).toHaveLength(8);
    expect(r.files[0]).toEqual({
      rel: "mods/rider/helmets/2022_Astars_SM10-Armega/goggles/Oakley Gold.pnt",
      url: "https://mega.nz/folder/z3ZVXBjb#8D-NY0nOo7In1Re_m-OeOA/file/SmhWWZLA",
      size: 26457739,
    });
  });

  it("a folder of nothing but empty folders is empty", async () => {
    const f = fakeFetch([[/n=kqNiXCzK/, () => new Response(fixture("mega-folder-empty.json"))]]);
    await expect(resolveShare("https://mega.nz/folder/kqNiXCzK#asQ0kpmZIvo1JSGbcf2lBw", f)).rejects.toThrow(/empty/);
  });

  it("resolves one file inside the folder to its key and download URL", async () => {
    const f = fakeFetch([
      [/n=z3ZVXBjb/, async (req) => {
        const body = (await req.json()) as [{ a: string }];
        return body[0].a === "f" ? new Response(fixture("mega-folder.json")) : json([{ s: 26457739, g: "https://gfs.example.invalid/dl/x" }]);
      }],
    ]);
    const r = await resolveShare("https://mega.nz/folder/z3ZVXBjb#8D-NY0nOo7In1Re_m-OeOA/file/SmhWWZLA", f, false);
    expect(r.kind).toBe("mega");
    if (r.kind !== "mega") return;
    expect(r.name).toBe("Oakley Gold.pnt");
    expect(r.size).toBe(26457739);
    expect(r.key).toHaveLength(16);
  });

  it("decrypts a recorded file link's attributes", async () => {
    const f = fakeFetch([[/g\.api\.mega\.co\.nz/, () => new Response(fixture("mega-file.json"))]]);
    const r = await resolveShare("https://mega.nz/file/bJIkiSrS#wQBJlVM2KhPMzQLcfVTisVgh_eW_669Zd92fOWWVaF8", f);
    expect(r).toMatchObject({ kind: "mega", name: "Axell Hodges 2023 KXF by RkrdM.zip", size: 34958855 });
  });

  it("refuses a link MEGA has taken down, and retries one it is too busy for", async () => {
    await expect(resolveShare("https://mega.nz/file/a#" + "A".repeat(43), fakeFetch([[/mega/, () => json([-9])]]))).rejects.toMatchObject({
      permanent: true,
    });
    await expect(resolveShare("https://mega.nz/file/a#" + "A".repeat(43), fakeFetch([[/mega/, () => json([-3])]]))).rejects.toMatchObject({
      permanent: false,
    });
  });

  it("AES-ECB decryption matches the FIPS-197 vector", async () => {
    const hexb = (s: string) => Uint8Array.from(s.match(/../g)!.map((x) => parseInt(x, 16)));
    const key = hexb("000102030405060708090a0b0c0d0e0f");
    const ct = hexb("69c4e0d86a7b0430d8cdb78070b4c55a");
    expect(await aesEcbDecrypt(key, ct)).toEqual(hexb("00112233445566778899aabbccddeeff"));
  });

  it("decrypts a CTR stream in chunks of any size, at the right counter", async () => {
    const raw = crypto.getRandomValues(new Uint8Array(32));
    const { key, nonce } = megaFileKey(raw);
    const plain = new Uint8Array(100_003).map((_, i) => (i * 31 + 7) & 0xff);
    const counter = new Uint8Array(16);
    counter.set(nonce);
    const k = await crypto.subtle.importKey("raw", key, "AES-CTR", false, ["encrypt"]);
    const cipher = new Uint8Array(await crypto.subtle.encrypt({ name: "AES-CTR", counter, length: 64 }, k, plain));
    const chunks = [7, 16, 1000, 33, 65536, 30000, 3411];
    let o = 0;
    const source = new ReadableStream<Uint8Array>({
      pull(c) {
        const n = chunks.shift();
        if (n === undefined || o >= cipher.length) return c.close();
        c.enqueue(cipher.slice(o, (o += n)));
      },
    });
    const out = new Uint8Array(await new Response(source.pipeThrough(megaDecryptStream(key, nonce))).arrayBuffer());
    expect(out).toEqual(plain);
  });

  it("folds a 32-byte file key", () => {
    const raw = new Uint8Array(32).map((_, i) => i);
    const { key, nonce } = megaFileKey(raw);
    expect([...key]).toEqual([...raw.slice(0, 16)].map((b, i) => b ^ raw[16 + i]));
    expect([...nonce]).toEqual([...raw.slice(16, 24)]);
  });

  it("decodes MEGA's url-safe base64", () => {
    expect([...b64urlDecode("_-8")]).toEqual([0xff, 0xef]);
  });
});

describe("opening a body", () => {
  it("names the file from Content-Disposition, else the URL", () => {
    const r = new Response("x", { headers: { "content-disposition": "attachment; filename*=UTF-8''Track%20One.pkz" } });
    expect(filenameFrom(r, "https://x/y")).toBe("Track One.pkz");
    expect(filenameFrom(new Response("x"), "https://h/a/b/Track_Server.zip?x=1")).toBe("Track_Server.zip");
  });
  it("a web page where a file should be is a permanent failure for a direct link", async () => {
    await expect(openBody(fakeFetch([[/./, () => html("<html>shop</html>")]]), "https://example.com/x")).rejects.toMatchObject({
      permanent: true,
    });
  });
  it("busy hosts are retried later", async () => {
    const f = fakeFetch([[/./, () => new Response("", { status: 503, headers: { "retry-after": "120" } })]]);
    await expect(openBody(f, "https://example.com/x")).rejects.toMatchObject({ permanent: false, retryAfterMs: 120_000 });
  });
});
