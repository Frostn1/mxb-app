/**
 * Mint redeemable plugin keys.
 *
 *   bun scripts/mint-plugin-key.ts --plugin replaycam --months 1 --count 5 --note "August batch"
 *
 * Prints the codes and the SQL that creates them. It deliberately does NOT talk to D1
 * itself: applying it is a separate, deliberate act, and the remote database does not take
 * `migrations apply` (it restarts at 0001 and aborts), so statements go in by hand:
 *
 *   wrangler d1 execute mxb-control-plane --remote --file=/tmp/keys.sql
 *
 * A key is one-shot and grants months on a license. Renewing is another key — until there
 * is a billing webhook, at which point it inserts these same rows and nothing downstream
 * has to know the difference.
 */

const args = new Map<string, string>();
for (let i = 2; i < Bun.argv.length; i += 2) {
  const flag = Bun.argv[i];
  if (!flag?.startsWith("--")) continue;
  args.set(flag.slice(2), Bun.argv[i + 1] ?? "");
}

const plugin = args.get("plugin") ?? "replaycam";
const months = Number(args.get("months") ?? "1");
const count = Number(args.get("count") ?? "1");
const note = args.get("note") ?? "";

if (!Number.isInteger(months) || months < 1 || months > 24) {
  console.error("--months must be a whole number of months, 1..24");
  process.exit(1);
}
if (!Number.isInteger(count) || count < 1 || count > 500) {
  console.error("--count must be 1..500");
  process.exit(1);
}

/**
 * Crockford base32 minus the letters that get misread aloud or in a screenshot: no I, L, O,
 * U. These get typed by hand off a Discord message, and a code that reads `1` as `I` half
 * the time turns every sale into a support conversation.
 */
const ALPHABET = "0123456789ABCDEFGHJKMNPQRSTVWXYZ";

function group(): string {
  const bytes = crypto.getRandomValues(new Uint8Array(4));
  return [...bytes].map((b) => ALPHABET[b % ALPHABET.length]).join("");
}

function code(): string {
  return `FRST-${group()}-${group()}-${group()}`;
}

const codes = new Set<string>();
while (codes.size < count) codes.add(code());

const sql = [...codes]
  .map(
    (c) =>
      `INSERT INTO plugin_keys (code, plugin_id, months, note, created_at) VALUES ('${c}', '${plugin}', ${months}, ${note ? `'${note.replace(/'/g, "''")}'` : "NULL"}, unixepoch());`,
  )
  .join("\n");

console.log(`-- ${count} key(s) for ${plugin}, ${months} month(s) each\n${sql}\n`);
console.error(`\nCodes:\n${[...codes].join("\n")}\n`);
