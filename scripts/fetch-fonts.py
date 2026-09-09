#!/usr/bin/env python3
"""Re-vendor Barlow + Barlow Condensed into packages/shared/src/fonts and regenerate packages/shared/src/fonts.css.

The app must render offline, so the faces are bundled rather than pulled from
Google at runtime. Only the latin and latin-ext subsets are kept — between them
they cover all six locales the app ships.
"""
import os, re, subprocess, sys

UA = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 Chrome/120 Safari/537.36"
URL = ("https://fonts.googleapis.com/css2?family=Barlow:wght@400;500;600;700"
       "&family=Barlow+Condensed:wght@600;700&display=swap")
KEEP = {"latin", "latin-ext"}
ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

css = subprocess.run(["curl", "-s", "--max-time", "30", "-A", UA, URL],
                     capture_output=True, text=True).stdout
if "@font-face" not in css:
    sys.exit("could not fetch the font stylesheet")

os.makedirs(os.path.join(ROOT, "packages/shared/src/fonts"), exist_ok=True)
parts = re.split(r"/\*\s*([a-z-]+)\s*\*/", css)
out, i = [], 1
while i < len(parts) - 1:
    subset, block, i = parts[i], parts[i + 1], i + 2
    if subset not in KEEP:
        continue
    fam = re.search(r"font-family:\s*'([^']+)'", block).group(1)
    wt = re.search(r"font-weight:\s*(\d+)", block).group(1)
    url = re.search(r"url\((https://[^)]+\.woff2)\)", block).group(1)
    rng = re.search(r"unicode-range:\s*([^;]+);", block).group(1).strip()
    name = (fam.replace(" ", "") + "-" + wt + "-" + subset + ".woff2").lower()
    subprocess.run(["curl", "-s", "--max-time", "30", "-A", UA, "-o",
                    os.path.join(ROOT, "packages/shared/src/fonts", name), url], check=True)
    out.append("@font-face {\n  font-family: '%s';\n  font-style: normal;\n"
               "  font-weight: %s;\n  font-display: swap;\n"
               "  src: url('/fonts/%s') format('woff2');\n  unicode-range: %s;\n}"
               % (fam, wt, name, rng))

with open(os.path.join(ROOT, "packages/shared/src/fonts.css"), "w") as fh:
    fh.write("/* Barlow + Barlow Condensed (SIL Open Font License 1.1), vendored so the app\n"
             "   renders correctly offline. latin + latin-ext only: those cover all six\n"
             "   shipped locales. Regenerate with scripts/fetch-fonts.py. */\n\n"
             + "\n\n".join(out) + "\n")
print("wrote packages/shared/src/fonts.css with %d faces" % len(out))
