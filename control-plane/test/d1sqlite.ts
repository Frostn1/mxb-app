/**
 * A D1 built out of `node:sqlite`, for the tests that are about SQL.
 *
 * The alternative — and what this replaces — was a stub that matched on the first few words
 * of each statement and answered from a `Record`. That is fine when the thing under test is
 * a decision and the query is incidental. It is not fine for anything to do with plugin
 * licenses: the revocation predicates, the filters and the capped count subquery *are* the
 * behaviour, and a stub answers them by agreeing with whatever the code happens to do.
 *
 * So this applies the real migrations to a real SQLite and wears D1's interface over it. It
 * is not an emulator — no sessions, no time travel — only the surface `env.DB` is asked for.
 *
 * `node:sqlite` is built in from node 22.13, which `package.json` asks for and CI installs.
 */

import { createRequire } from "node:module";
import { readdirSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

// `node:sqlite` is newer than the list of node builtins this vite knows, so a static import
// of it is resolved as a package and fails. Required rather than imported for that reason
// alone — `node:module` is on the list, and what it hands back is the real thing.
const { DatabaseSync } = createRequire(import.meta.url)("node:sqlite") as {
  DatabaseSync: typeof import("node:sqlite").DatabaseSync;
};

/** The database handle, named once — `DatabaseSync` is a value here, not a type. */
type Db = InstanceType<typeof DatabaseSync>;

const MIGRATIONS = join(dirname(fileURLToPath(import.meta.url)), "..", "migrations");

/** An in-memory database with every migration applied, wearing D1's interface. */
export function d1(): Env["DB"] {
  const db: Db = new DatabaseSync(":memory:");
  db.exec("PRAGMA foreign_keys = ON");
  for (const file of readdirSync(MIGRATIONS).sort()) {
    if (file.endsWith(".sql")) db.exec(readFileSync(join(MIGRATIONS, file), "utf8"));
  }

  // `all` runs the statement whatever it is, so an UPDATE ... RETURNING comes back the way
  // D1 hands it to `.first()`.
  const rows = (sql: string, args: unknown[]) =>
    db.prepare(sql).all(...(args as never[])) as unknown as Record<string, unknown>[];

  const statement = (sql: string, args: unknown[] = []) => ({
    sql,
    args,
    bind: (...next: unknown[]) => statement(sql, next),
    async first<T>() {
      return (rows(sql, args)[0] ?? null) as T | null;
    },
    async all<T>() {
      return { results: rows(sql, args) as T[], success: true };
    },
    async run() {
      rows(sql, args);
      return { success: true };
    },
  });

  return {
    prepare: (sql: string) => statement(sql),
    async batch(stmts: { sql: string; args: unknown[] }[]) {
      // D1 batches are one transaction, which is the property `mintKeys` leans on: a batch
      // that collides on a code must leave no codes behind.
      db.exec("BEGIN");
      try {
        for (const s of stmts) rows(s.sql, s.args);
        db.exec("COMMIT");
      } catch (e) {
        db.exec("ROLLBACK");
        throw e;
      }
      return stmts.map(() => ({ success: true }));
    },
  } as unknown as Env["DB"];
}

/** An account to hang a license off. The rest of `accounts` is beside the point here. */
export async function addAccount(
  db: Env["DB"],
  id: string,
  riderName: string,
  steamId: string | null = null,
): Promise<void> {
  await db
    .prepare(
      `INSERT INTO accounts (id, rider_name, steam_id, token_hash, created_at)
       VALUES (?, ?, ?, ?, unixepoch())`,
    )
    .bind(id, riderName, steamId, `hash_${id}`)
    .run();
}

/** Give a plugin a build, which is what makes the bundle route have anything to serve. */
export async function publishBundle(
  db: Env["DB"],
  pluginId: string,
  version: string,
  sha256: string,
): Promise<void> {
  await db
    .prepare(`UPDATE plugins SET version = ?, bundle_key = ?, bundle_sha256 = ? WHERE id = ?`)
    .bind(version, `plugins/${pluginId}-${version}.zip`, sha256, pluginId)
    .run();
}
