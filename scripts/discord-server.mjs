#!/usr/bin/env node
//
// Bring the Discord server up to the MXB Secure layout.
//
//   node scripts/discord-server.mjs                 # plan only — prints what would change
//   node scripts/discord-server.mjs --apply         # make the changes
//   node scripts/discord-server.mjs --apply --webhooks
//
// The server is the MXB Secure home and the apps are products under it, so the layout is a
// category per product with the community channels above them. Everything below is matched
// by NAME, which makes the script idempotent: run it on a fresh guild and it builds the
// whole thing, run it on the existing one and it adopts the channels already there —
// renaming nothing, deleting nothing, only filling in what's missing and moving a channel
// under the category it belongs to.
//
// It never deletes. A channel that exists here but not in LAYOUT is left exactly as it is,
// reported as "left alone", and keeps its messages. Undoing a run means dragging channels
// back in the Discord UI, not restoring from anything.
//
// Flags:
//   --apply       actually write. Without it nothing is sent but the reads, and the plan is
//                 printed instead — which is the point: read the plan before the first run.
//   --webhooks    ensure each release channel has a webhook and print its URL with the name
//                 of the GitHub secret it belongs in. Off by default because the URLs are
//                 credentials: anyone holding one can post as the app. Never pass it in CI.
//   --rename-guild  also rename the server itself to LAYOUT.guild.name. Separate from
//                 --apply because every member sees it happen.
//   --enforce-permissions  apply the read-only overwrite to channels that already existed.
//                 By default it's only set on channels this script creates, so a channel
//                 you've already tuned by hand doesn't get overwritten by a default.
//
// Env:
//   DISCORD_BOT_TOKEN  a bot in the guild with Manage Channels (and Manage Webhooks for
//                      --webhooks, Manage Server for --rename-guild). Not a webhook URL —
//                      webhooks can only post, they cannot create channels.
//   DISCORD_GUILD_ID   the server's ID. Right-click the server with Developer Mode on.

const API = "https://discord.com/api/v10";

// SEND_MESSAGES. Denied to @everyone on the release channels: they carry one automated
// message per release and a conversation in them buries it. Talking happens in the help
// channel next door, which every release channel's topic points at.
const SEND_MESSAGES = 1n << 11n;

// The layout. `key` is this script's stable handle for a channel — the release wiring
// refers to channels by key, so renaming one here is a rename, not a second channel.
const LAYOUT = {
  guild: {
    name: "MXB Secure",
    description: "Tools for MX Bikes creators: the MXB App, Frost's Studio and the GUID lock.",
  },
  categories: [
    {
      name: "MXB SECURE",
      channels: [
        {
          key: "welcome",
          name: "welcome",
          readOnly: true,
          topic: "MXB Secure is the home of the MXB App, Frost's Studio and the GUID lock. Start here, then pick the product you use.",
        },
        {
          key: "announcements",
          name: "announcements",
          readOnly: true,
          topic: "News that matters across all of MXB Secure. Per-product releases post in each product's own releases channel.",
        },
        {
          key: "rules",
          name: "rules",
          readOnly: true,
          topic: "How this server works, and what happens to people selling other creators' work.",
        },
        {
          key: "status",
          name: "status",
          readOnly: true,
          topic: "mxbsecure.com and api.mxbsecure.com outages, and when they are back.",
        },
      ],
    },
    {
      name: "MXB APP",
      channels: [
        {
          key: "app-releases",
          name: "app-releases",
          readOnly: true,
          webhook: { name: "MXB App Releases", secret: "DISCORD_WEBHOOK_URL" },
          topic: "Every published MXB App release, with the installers. The in-app updater offers you these. Questions in #app-help.",
        },
        {
          key: "app-beta",
          name: "app-beta",
          readOnly: true,
          webhook: { name: "MXB App Betas", secret: "DISCORD_BETA_WEBHOOK_URL" },
          topic: "Beta builds, for testers. The updater never hands you one — turn on Beta updates in Settings → About, or grab an installer here.",
        },
        { key: "app-help", name: "app-help", topic: "Using the MXB App: mods, presets, paints, servers." },
        { key: "app-bugs", name: "app-bugs", topic: "Something broken in the MXB App. Say which version, and attach the log from Settings → About." },
      ],
    },
    {
      name: "FROST'S STUDIO",
      channels: [
        {
          key: "studio-releases",
          name: "studio-releases",
          readOnly: true,
          webhook: { name: "Frost's Studio Releases", secret: "DISCORD_STUDIO_WEBHOOK_URL" },
          topic: "Every published Frost's Studio release, with the installers. Questions in #studio-help.",
        },
        {
          key: "studio-beta",
          name: "studio-beta",
          readOnly: true,
          webhook: { name: "Frost's Studio Betas", secret: "DISCORD_STUDIO_BETA_WEBHOOK_URL" },
          topic: "Beta builds of Frost's Studio, for testers. Not offered by the updater — install from here.",
        },
        { key: "studio-help", name: "studio-help", topic: "Painting bikes and gear, and building tracks, in Frost's Studio." },
        { key: "studio-showcase", name: "studio-showcase", topic: "Paints and tracks you have made. Pictures welcome." },
      ],
    },
    {
      name: "MXB SECURE — LOCK",
      channels: [
        {
          key: "secure-releases",
          name: "secure-releases",
          readOnly: true,
          webhook: { name: "mxbsecure.com Releases", secret: "DISCORD_SECURE_WEBHOOK_URL" },
          topic: "What shipped on mxbsecure.com and in the browser locker. Questions in #secure-help.",
        },
        { key: "secure-help", name: "secure-help", topic: "Locking a file to a buyer's GUID, adding buyers, and what a buyer does with the file." },
        { key: "creators", name: "creators", topic: "For affiliated creators: API keys, buyer lists, and what the shop integration expects." },
      ],
    },
    {
      name: "COMMUNITY",
      channels: [
        { key: "general", name: "general", topic: "MX Bikes, and everything that is not a support question." },
        { key: "paints-and-tracks", name: "paints-and-tracks", topic: "Sharing and finding paints and tracks." },
        { key: "servers", name: "servers", topic: "Running a dedicated server, and finding one worth joining." },
        { key: "off-topic", name: "off-topic", topic: "Not MX Bikes." },
      ],
    },
  ],
};

const CHANNEL_TEXT = 0;
const CHANNEL_CATEGORY = 4;

// --- arguments ------------------------------------------------------------------------

const args = new Set(process.argv.slice(2));
for (const a of args) {
  if (!["--apply", "--webhooks", "--rename-guild", "--enforce-permissions"].includes(a)) {
    console.error(`unknown option: ${a}`);
    process.exit(2);
  }
}
const APPLY = args.has("--apply");
const WITH_WEBHOOKS = args.has("--webhooks");
const RENAME_GUILD = args.has("--rename-guild");
const ENFORCE_PERMS = args.has("--enforce-permissions");

const TOKEN = process.env.DISCORD_BOT_TOKEN;
const GUILD = process.env.DISCORD_GUILD_ID;
if (!TOKEN || !GUILD) {
  console.error("DISCORD_BOT_TOKEN and DISCORD_GUILD_ID must both be set.");
  console.error("The token is a bot token, not a webhook URL — a webhook can only post messages.");
  process.exit(2);
}

// --- transport ------------------------------------------------------------------------

// Discord answers 429 with the seconds to wait rather than a header we can pre-empt, so the
// only correct thing to do is sleep exactly that long and try again. A run touches a few
// dozen endpoints, so this happens.
async function api(method, path, body) {
  for (let attempt = 0; ; attempt++) {
    const res = await fetch(`${API}${path}`, {
      method,
      headers: {
        Authorization: `Bot ${TOKEN}`,
        "Content-Type": "application/json",
        "User-Agent": "mxb-secure-server-setup (https://github.com/Frostn1/mxb-app, 1.0)",
      },
      body: body === undefined ? undefined : JSON.stringify(body),
    });

    if (res.status === 429 && attempt < 5) {
      const retry = Number((await res.json().catch(() => ({}))).retry_after ?? 1);
      await new Promise((r) => setTimeout(r, (retry + 0.25) * 1000));
      continue;
    }

    const text = await res.text();
    if (!res.ok) {
      // 50013 is "Missing Permissions" and is the one failure worth naming, because the fix
      // is in Discord's role settings rather than in this script.
      let hint = "";
      try {
        if (JSON.parse(text).code === 50013) hint = "\n  The bot's role needs Manage Channels (and Manage Webhooks / Manage Server for those flags), and must sit high enough in the role list to touch these channels.";
      } catch {
        // Not JSON — a gateway or proxy error page. The status and body below say enough.
      }
      throw new Error(`${method} ${path} → ${res.status}\n  ${text}${hint}`);
    }
    return text ? JSON.parse(text) : null;
  }
}

// Discord lowercases channel names and turns spaces into dashes, so a channel typed
// "App Releases" in the UI comes back as "app-releases". Compare on that shape or the
// script creates a duplicate of a channel that is already there.
const slug = (s) => s.trim().toLowerCase().replace(/\s+/g, "-");

// --- plan -----------------------------------------------------------------------------

const plan = [];
const note = (verb, what, detail) => plan.push({ verb, what, detail });

const guild = await api("GET", `/guilds/${GUILD}`);
const existing = await api("GET", `/guilds/${GUILD}/channels`);

const categories = new Map(
  existing.filter((c) => c.type === CHANNEL_CATEGORY).map((c) => [slug(c.name), c]),
);
// Text channels are unique by name per guild, not per category, so one flat map is right.
const texts = new Map(
  existing.filter((c) => c.type === CHANNEL_TEXT).map((c) => [slug(c.name), c]),
);

const claimed = new Set();
const created = [];   // {key, channel} — filled in during apply, used by --webhooks
const resolved = new Map(); // key -> existing channel, for channels already present

if (RENAME_GUILD && guild.name !== LAYOUT.guild.name) {
  note("rename server", `"${guild.name}"`, `→ "${LAYOUT.guild.name}"`);
}

for (const cat of LAYOUT.categories) {
  const haveCat = categories.get(slug(cat.name));
  if (!haveCat) note("create category", cat.name, "");

  for (const ch of cat.channels) {
    const have = texts.get(slug(ch.name));
    if (!have) {
      note("create channel", `#${ch.name}`, `in ${cat.name}`);
      continue;
    }
    claimed.add(have.id);
    resolved.set(ch.key, have);

    if (haveCat && have.parent_id !== haveCat.id) {
      const from = existing.find((c) => c.id === have.parent_id);
      note("move channel", `#${ch.name}`, `${from ? from.name : "no category"} → ${cat.name}`);
    } else if (!haveCat) {
      note("move channel", `#${ch.name}`, `→ ${cat.name} (once the category exists)`);
    }
    if ((have.topic ?? "") !== ch.topic) {
      note("set topic", `#${ch.name}`, have.topic ? "replacing the current one" : "it has none");
    }
    if (ch.readOnly && ENFORCE_PERMS) {
      note("lock channel", `#${ch.name}`, "deny Send Messages to @everyone");
    }
  }
}

const untouched = existing.filter(
  (c) => c.type === CHANNEL_TEXT && !claimed.has(c.id),
);

// --- print the plan -------------------------------------------------------------------

console.log(`Server: ${guild.name} (${GUILD})`);
console.log(`Channels now: ${existing.filter((c) => c.type === CHANNEL_TEXT).length} text in ${categories.size} categories\n`);

if (plan.length === 0) {
  console.log("Nothing to change — the server already matches the layout.");
} else {
  console.log(APPLY ? "Applying:" : "Would change (pass --apply to do it):");
  const width = Math.max(...plan.map((p) => p.verb.length));
  for (const p of plan) {
    console.log(`  ${p.verb.padEnd(width)}  ${p.what}${p.detail ? `  ${p.detail}` : ""}`);
  }
}

if (untouched.length) {
  console.log(`\nLeft alone (${untouched.length} channel${untouched.length === 1 ? "" : "s"} not in the layout — nothing is ever deleted):`);
  console.log(`  ${untouched.map((c) => `#${c.name}`).join(", ")}`);
}

if (!RENAME_GUILD && guild.name !== LAYOUT.guild.name) {
  console.log(`\nThe server is still called "${guild.name}". Pass --rename-guild to make it "${LAYOUT.guild.name}".`);
}

if (!APPLY) {
  console.log("\nPlan only. Nothing was changed. Re-run with --apply.");
  process.exit(0);
}

// --- apply ----------------------------------------------------------------------------

console.log("");

if (RENAME_GUILD && guild.name !== LAYOUT.guild.name) {
  await api("PATCH", `/guilds/${GUILD}`, { name: LAYOUT.guild.name, description: LAYOUT.guild.description });
  console.log(`renamed the server to "${LAYOUT.guild.name}"`);
}

// Categories first: a channel cannot be parented to one that does not exist yet.
for (const cat of LAYOUT.categories) {
  if (categories.has(slug(cat.name))) continue;
  const made = await api("POST", `/guilds/${GUILD}/channels`, {
    name: cat.name,
    type: CHANNEL_CATEGORY,
  });
  categories.set(slug(cat.name), made);
  console.log(`created category ${cat.name}`);
}

for (const cat of LAYOUT.categories) {
  const parent = categories.get(slug(cat.name));

  for (const ch of cat.channels) {
    // Denying Send Messages to @everyone: the @everyone role's ID is always the guild's ID.
    const overwrites = ch.readOnly
      ? [{ id: GUILD, type: 0, deny: String(SEND_MESSAGES) }]
      : undefined;

    const have = texts.get(slug(ch.name));
    if (!have) {
      const made = await api("POST", `/guilds/${GUILD}/channels`, {
        name: ch.name,
        type: CHANNEL_TEXT,
        parent_id: parent.id,
        topic: ch.topic,
        ...(overwrites ? { permission_overwrites: overwrites } : {}),
      });
      texts.set(slug(ch.name), made);
      resolved.set(ch.key, made);
      created.push(ch.key);
      console.log(`created #${ch.name}`);
      continue;
    }

    // An existing channel gets its parent and topic brought in line, and its permissions
    // only if asked: whoever set them up may have meant them.
    const patch = {};
    if (have.parent_id !== parent.id) patch.parent_id = parent.id;
    if ((have.topic ?? "") !== ch.topic) patch.topic = ch.topic;
    if (ch.readOnly && ENFORCE_PERMS) patch.permission_overwrites = overwrites;
    if (Object.keys(patch).length) {
      const updated = await api("PATCH", `/channels/${have.id}`, patch);
      resolved.set(ch.key, updated);
      console.log(`updated #${ch.name} (${Object.keys(patch).join(", ")})`);
    }
  }
}

// Order last, in one call: the categories in the order they are declared above, and the
// channels inside each in theirs. Done as a bulk modify so Discord sorts once rather than
// re-flowing the sidebar on every PATCH.
const positions = [];
LAYOUT.categories.forEach((cat, i) => {
  const parent = categories.get(slug(cat.name));
  positions.push({ id: parent.id, position: i });
  cat.channels.forEach((ch, j) => {
    const c = texts.get(slug(ch.name));
    if (c) positions.push({ id: c.id, position: j, parent_id: parent.id });
  });
});
await api("PATCH", `/guilds/${GUILD}/channels`, positions);
console.log("ordered the sidebar");

// --- webhooks -------------------------------------------------------------------------

if (!WITH_WEBHOOKS) {
  const wanted = LAYOUT.categories.flatMap((c) => c.channels).filter((c) => c.webhook);
  console.log(`\nDone. ${wanted.length} release channels still need webhooks — re-run with --webhooks to create them and print the URLs.`);
  process.exit(0);
}

console.log("\nWebhooks — paste each URL into the named GitHub Actions secret:");
console.log("(these are credentials: anyone with one can post as the app. Do not paste them into a public channel or a CI log.)\n");

for (const cat of LAYOUT.categories) {
  for (const ch of cat.channels) {
    if (!ch.webhook) continue;
    const channel = resolved.get(ch.key);
    if (!channel) continue;

    const hooks = await api("GET", `/channels/${channel.id}/webhooks`);
    let hook = hooks.find((h) => h.name === ch.webhook.name);
    if (!hook) {
      hook = await api("POST", `/channels/${channel.id}/webhooks`, { name: ch.webhook.name });
      console.log(`  ${ch.webhook.secret}`);
      console.log(`    ${hook.url}    (new, #${ch.name})\n`);
    } else if (hook.token) {
      console.log(`  ${ch.webhook.secret}`);
      console.log(`    https://discord.com/api/webhooks/${hook.id}/${hook.token}    (existing, #${ch.name})\n`);
    } else {
      // A webhook someone else created: Discord hands back its ID but not its token, and a
      // token cannot be read back. Keep whatever is already in the secret.
      console.log(`  ${ch.webhook.secret}`);
      console.log(`    "${hook.name}" already exists in #${ch.name}, created by someone else — its URL cannot be read back. Keep the value already in this secret, or delete the webhook and re-run.\n`);
    }
  }
}

console.log("Set them with:  gh secret set <NAME> -R Frostn1/mxb-app");
