/**
 * The handful of node APIs the SQLite-backed test database needs — declared, not installed.
 *
 * Pulling in `@types/node` would type-check the whole program against a runtime the worker
 * does not have, and the first casualty would be the rule that keeps `Buffer` and `fs` out
 * of `src/`: they would start type-checking and fail only once deployed. This declares the
 * four functions `d1sqlite.ts` calls and nothing else.
 */

declare module "node:sqlite" {
  export class StatementSync {
    all(...params: unknown[]): unknown[];
    run(...params: unknown[]): unknown;
  }
  export class DatabaseSync {
    constructor(path: string);
    exec(sql: string): void;
    prepare(sql: string): StatementSync;
  }
}

declare module "node:fs" {
  export function readdirSync(path: string): string[];
  export function readFileSync(path: string, encoding: string): string;
}

declare module "node:url" {
  export function fileURLToPath(url: string): string;
}

declare module "node:path" {
  export function join(...parts: string[]): string;
  export function dirname(path: string): string;
}

interface ImportMeta {
  url: string;
}

declare module "node:module" {
  export function createRequire(path: string): (id: string) => unknown;
}
