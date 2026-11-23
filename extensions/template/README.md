# Extension template

1. Copy this folder and change `id` in `extension.json` to `yourname.something`
   (lower-case, dot-separated; ids starting `mxb.`, `mxbsecure.` or `frost.` are reserved).
2. In MXB App: **Extensions → Build → Developer mode**, then **Load folder…** and pick
   this folder. Load it again after each change.
3. When it works, make yourself a signing key once and pack it:

   ```
   node scripts/extensions/pack.mjs --keygen my-key.pem
   node scripts/extensions/pack.mjs path/to/this/folder --key my-key.pem
   ```

   Keep `my-key.pem` private and backed up: the app pins the key a signed extension was first
   installed with, and refuses an update signed by any other.

The full reference is [docs/extensions.md](../../docs/extensions.md).
