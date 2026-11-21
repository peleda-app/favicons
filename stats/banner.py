import json
import sys

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


parts = [f'<tspan fill="#fbf0df">★ {short(stars)}</tspan> on GitHub']
if icons > 0:
    parts.append(f'<tspan fill="#fbf0df">{icons:,}</tspan> icons served')
line = (
    '<text x="98" y="586" xml:space="preserve" font-family="Inter, \'Helvetica Neue\', Helvetica, Arial, sans-serif" '
    'font-size="24" fill="#9a9aa3">' + '<tspan fill="#66666f">   ·   </tspan>'.join(parts) + "</text>"
)
head, tail = open(template).read().rsplit("</svg>", 1)
svg = f"{head}  {line}\n</svg>{tail}"
open(out, "w").write(svg)
print(f"stars={stars} icons={icons}")
