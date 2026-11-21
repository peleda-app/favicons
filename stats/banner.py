import json
import sys

STAR = "M234.29,114.85l-45,38.83L203,211.75a16.4,16.4,0,0,1-24.5,17.82L128,198.49,77.47,229.57A16.4,16.4,0,0,1,53,211.75l13.76-58.07-45-38.83A16.46,16.46,0,0,1,31.08,86l59-4.76,22.76-55.08a16.36,16.36,0,0,1,30.27,0l22.75,55.08,59,4.76a16.46,16.46,0,0,1,9.37,28.86Z"
FONT = "Inter, 'Helvetica Neue', Helvetica, Arial, sans-serif"

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


parts = [f'<tspan fill="#fbf0df">{short(stars)}</tspan> on GitHub']
if icons > 0:
    parts.append(f'<tspan fill="#fbf0df">{icons:,}</tspan> icons served')
sep = '<tspan fill="#55555d">   /   </tspan>'
line = (
    f'<svg x="97" y="565" width="26" height="26" viewBox="0 0 256 256" fill="#fbf0df"><path d="{STAR}"/></svg>'
    f'<text x="131" y="586" xml:space="preserve" font-family="{FONT}" font-size="24" fill="#9a9aa3">'
    + sep.join(parts)
    + "</text>"
)
head, tail = open(template).read().rsplit("</svg>", 1)
open(out, "w").write(f"{head}  {line}\n</svg>{tail}")
print(f"stars={stars} icons={icons}")
