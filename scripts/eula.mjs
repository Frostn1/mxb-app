// Every app's EULA and anti-decompilation notice, from one template.
//
//   bun scripts/eula.mjs          write apps/<app>/src-tauri/legal/{EULA.txt,NOTICE.txt,VERSION} for every app
//   bun scripts/eula.mjs --check  fail if any app's files differ from what the template makes
//
// The template is legal/EULA.template.txt (and NOTICE.template.txt, which the EULA opens with);
// what differs per app is legal/apps.json. Change the text there, never in an app's copy, and
// bump `version` there so every app asks for the new agreement to be accepted again. A new app
// gets an entry in apps.json; `"private": true` for one kept out of this repo (its build checks it). An app embeds its two files as plain strings (see the Converter's
// `src-tauri/src/legal.rs`), so the notice is readable in the binary.
import { readFileSync, writeFileSync, mkdirSync, existsSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const read = (p) => readFileSync(join(root, p), "utf8").replace(/\r\n/g, "\n");
const config = JSON.parse(read("legal/apps.json"));
const eulaTemplate = read("legal/EULA.template.txt");
const noticeTemplate = read("legal/NOTICE.template.txt");

function fill(text, values) {
  const out = text.replace(/\{\{([A-Z_]+)\}\}/g, (m, key) => {
    if (!(key in values)) throw new Error(`no value for ${m}`);
    return values[key];
  });
  if (/\{\{[A-Z_]+\}\}/.test(out)) return fill(out, values);
  return out;
}

export function render(app) {
  const model = config.models[app.model];
  if (!model) throw new Error(`${app.dir}: unknown model ${app.model}`);
  const values = {
    PRODUCT: app.product,
    TITLE: app.product.toUpperCase(),
    VERSION: config.version,
    COVERS: app.covers,
    PURPOSE: app.purpose,
    CONTENT: app.content,
    COMPETING: app.competing,
    OUTPUT: app.output,
  };
  values.GRANT = fill(model.grant, values);
  values.TERM = model.term;
  const notice = fill(noticeTemplate, values);
  values.NOTICE = notice.trimEnd();
  return { eula: fill(eulaTemplate, values), notice, version: config.version };
}

const check = process.argv.includes("--check");
let stale = 0;
for (const app of config.apps) {
  // A private app (Frost Workflow) lives in a private repo with its own copy of the files and is
  // only placed here to build; where it isn't placed, there is nothing to write or check.
  if (app.private && !existsSync(join(root, app.dir, "src-tauri"))) continue;
  const { eula, notice, version } = render(app);
  const dir = join(root, app.dir, "src-tauri", "legal");
  for (const [name, text] of [["EULA.txt", eula], ["NOTICE.txt", notice], ["VERSION", version]]) {
    const path = join(dir, name);
    const current = existsSync(path) ? readFileSync(path, "utf8").replace(/\r\n/g, "\n") : null;
    if (current === text) continue;
    if (check) {
      console.error(`${app.dir}/src-tauri/legal/${name} is not what legal/ makes. Run: bun scripts/eula.mjs`);
      stale++;
    } else {
      mkdirSync(dir, { recursive: true });
      writeFileSync(path, text);
      console.log(`wrote ${app.dir}/src-tauri/legal/${name}`);
    }
  }
}
if (stale) process.exit(1);
if (check) console.log(`EULAs match the template (${config.apps.length} apps, version ${config.version}).`);
