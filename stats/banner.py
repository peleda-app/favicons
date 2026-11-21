import json
import os
import subprocess
import sys

STAR = "M234.29,114.85l-45,38.83L203,211.75a16.4,16.4,0,0,1-24.5,17.82L128,198.49,77.47,229.57A16.4,16.4,0,0,1,53,211.75l13.76-58.07-45-38.83A16.46,16.46,0,0,1,31.08,86l59-4.76,22.76-55.08a16.36,16.36,0,0,1,30.27,0l22.75,55.08,59,4.76a16.46,16.46,0,0,1,9.37,28.86Z"
DOWNLOAD = "M228,144v64a12,12,0,0,1-12,12H40a12,12,0,0,1-12-12V144a12,12,0,0,1,24,0v52H204V144a12,12,0,0,1,24,0Zm-108.49,8.49a12,12,0,0,0,17,0l40-40a12,12,0,0,0-17-17L140,115V32a12,12,0,0,0-24,0v83L96.49,95.51a12,12,0,0,0-17,17Z"
FONT = "Inter, 'Helvetica Neue', Helvetica, Arial, sans-serif"
SIZE = 24
ICON = 26


def font_file():
    path = os.environ.get("BANNER_FONT")
    if path:
        return path
    try:
        out = subprocess.run(["fc-match", "-f", "%{family}|%{file}", "Inter"], capture_output=True, text=True).stdout
        family, file = out.split("|", 1)
        return file if "Inter" in family else None
    except Exception:
        return None


def measurer():
    path = font_file()
    if path:
        try:
            from PIL import ImageFont

            font = ImageFont.truetype(path, SIZE)
            return lambda text: font.getlength(text)
        except Exception:
            pass
    return lambda text: len(text) * SIZE * 0.56


template, stats_path, stars, out = sys.argv[1:5]
stats = json.load(open(stats_path))
icons = (
    int(stats.get("total", 0))
    + sum(int(s.get("count", 0)) for s in stats.get("sources", {}).values())
    + sum(int(i.get("count", 0)) for i in stats.get("instances", []))
)
stars = int(stars or 0)


def short(n):
    if n >= 10_000:
        return f"{n / 1000:.0f}k"
    if n >= 1000:
        return f"{n / 1000:.1f}k"
    return str(n)


width = measurer()
items = [(STAR, short(stars), " on GitHub")]
if icons > 0:
    items.append((DOWNLOAD, f"{icons:,}", " icons served"))

x, baseline, parts = 97.0, 586, []
for i, (path, number, label) in enumerate(items):
    if i:
        parts.append(f'<text x="{x + 18:.1f}" y="{baseline}" font-family="{FONT}" font-size="{SIZE}" fill="#55555d">/</text>')
        x += 18 + width("/") + 22
    parts.append(f'<svg x="{x:.1f}" y="{baseline - 21}" width="{ICON}" height="{ICON}" viewBox="0 0 256 256" fill="#fbf0df"><path d="{path}"/></svg>')
    x += ICON + 8
    parts.append(
        f'<text x="{x:.1f}" y="{baseline}" xml:space="preserve" font-family="{FONT}" font-size="{SIZE}" fill="#9a9aa3">'
        f'<tspan fill="#fbf0df">{number}</tspan>{label}</text>'
    )
    x += width(number + label)

head, tail = open(template).read().rsplit("</svg>", 1)
open(out, "w").write(f"{head}  {''.join(parts)}\n</svg>{tail}")
print(f"stars={stars} icons={icons}")
