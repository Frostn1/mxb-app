/** `.md` imports arrive as their text — see `rules` in wrangler.jsonc. */
declare module "*.md" {
  const text: string;
  export default text;
}
