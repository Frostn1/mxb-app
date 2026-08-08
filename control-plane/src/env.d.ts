/**
 * Secrets, declared for the type checker.
 *
 * `wrangler types` generates `Env` from `wrangler.jsonc`, and secrets deliberately do not
 * appear there — putting their *values* in config is the mistake the secret store exists to
 * prevent. So their names are declared here instead, by interface merging, and the values
 * come from `wrangler secret put`.
 *
 * They are optional on purpose. A deployment without them is a valid deployment — paint
 * sync, the registry and the roster all work — it simply cannot provision, and the
 * provisioning endpoints answer 503 rather than crashing on a missing key.
 */
declare global {
  interface Env {
    /** IAM key scoped to launching and managing `mxb:managed` instances in one region. */
    AWS_ACCESS_KEY_ID?: string;
    AWS_SECRET_ACCESS_KEY?: string;
  }
}

export {};
