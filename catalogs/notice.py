import sys
import tomllib

src, out = sys.argv[1], sys.argv[2]
cats = tomllib.load(open(src, "rb"))["catalogs"]
lines = [
    "Prerendered icon catalogs bundled in the favicons.page image.",
    "The favicons software is MIT licensed. The icons below are not: each",
    "catalog keeps the license of its authors, and logos stay trademarks of",
    "their owners.",
    "",
]
for c in cats:
    lines.append(f"{c['name']} (/{{style}}/{c['id']}/{{slug}})")
    lines.append(f"  license: {c['license']}")
    if c.get("attribution"):
        lines.append(f"  source:  {c['attribution']}")
    lines.append("")
open(out, "w").write("\n".join(lines))
spdx = set()
for c in cats:
    spdx.update(c.get("licenses") or [c["license"]])
licenses = sorted({lic for lic in spdx if " " not in lic} | {"MIT"})
print(" AND ".join(licenses))
