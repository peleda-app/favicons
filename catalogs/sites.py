#!/usr/bin/env python3
import argparse
import csv
import datetime
import gzip
import io
import ipaddress
import sys
import urllib.request
import zipfile

SOURCES = {
    "majestic": {
        "name": "Majestic Million",
        "url": "https://downloads.majestic.com/majestic_million.csv",
        "license": "CC BY 3.0",
        "license_url": "https://creativecommons.org/licenses/by/3.0/",
        "home": "https://majestic.com/reports/majestic-million",
    },
    "umbrella": {
        "name": "Cisco Umbrella Popularity List",
        "url": "http://s3-us-west-1.amazonaws.com/umbrella-static/top-1m.csv.zip",
        "license": "free of charge",
        "license_url": "https://umbrella.cisco.com/blog/cisco-umbrella-1-million",
        "home": "https://umbrella.cisco.com/blog/cisco-umbrella-1-million",
    },
    "crux": {
        "name": "Chrome UX Report top origins",
        "url": "https://raw.githubusercontent.com/zakird/crux-top-lists/main/data/global/current.csv.gz",
        "license": "CC BY-SA 4.0",
        "license_url": "https://creativecommons.org/licenses/by-sa/4.0/",
        "home": "https://github.com/zakird/crux-top-lists",
    },
}
PSL_URL = "https://publicsuffix.org/list/public_suffix_list.dat"
DEPTH = 1_000_000


def fetch(url):
    req = urllib.request.Request(url, headers={"User-Agent": "favicons-sites-ci"})
    with urllib.request.urlopen(req, timeout=300) as r:
        return r.read()


class Suffixes:
    def __init__(self, text):
        self.rules, self.wild, self.exceptions = set(), set(), set()
        for line in text.splitlines():
            line = line.strip()
            if not line or line.startswith("//"):
                continue
            rule = line.split()[0].lower()
            try:
                rule = rule.encode("idna").decode("ascii")
            except UnicodeError:
                pass
            if rule.startswith("!"):
                self.exceptions.add(rule[1:])
            elif rule.startswith("*."):
                self.wild.add(rule[2:])
            else:
                self.rules.add(rule)

    def registrable(self, host):
        labels = host.split(".")
        for i in range(len(labels)):
            candidate = ".".join(labels[i:])
            parent = ".".join(labels[i + 1:])
            if candidate in self.exceptions:
                return candidate
            if candidate in self.rules or (parent and parent in self.wild):
                return ".".join(labels[i - 1:]) if i > 0 else None
        return ".".join(labels[-2:]) if len(labels) >= 2 else None


def host_of(entry):
    entry = entry.strip().lower()
    if "://" in entry:
        entry = entry.split("://", 1)[1]
    entry = entry.split("/", 1)[0].split(":", 1)[0].rstrip(".")
    if not entry or "." not in entry:
        return None
    try:
        ipaddress.ip_address(entry)
        return None
    except ValueError:
        pass
    if not all(c.isalnum() or c in "-." for c in entry):
        return None
    return entry


def majestic(raw):
    rows = csv.reader(io.StringIO(raw.decode("utf-8", "replace")))
    next(rows, None)
    for row in rows:
        if len(row) > 2 and row[0].isdigit():
            yield int(row[0]), row[2]


def umbrella(raw):
    with zipfile.ZipFile(io.BytesIO(raw)) as z:
        text = z.read(z.namelist()[0]).decode("utf-8", "replace")
    for row in csv.reader(io.StringIO(text)):
        if len(row) > 1 and row[0].isdigit():
            yield int(row[0]), row[1]


def crux(raw):
    rows = csv.reader(io.StringIO(gzip.decompress(raw).decode("utf-8", "replace")))
    next(rows, None)
    for row in rows:
        if len(row) > 1 and row[1].isdigit():
            yield int(row[1]), row[0]


PARSERS = {"majestic": majestic, "umbrella": umbrella, "crux": crux}


def ranks(entries, suffixes):
    best = {}
    for rank, entry in entries:
        if rank > DEPTH:
            continue
        host = host_of(entry)
        domain = host and suffixes.registrable(host)
        if domain and rank < best.get(domain, DEPTH + 1):
            best[domain] = rank
    return best


def combine(lists, limit):
    score = {}
    for best in lists.values():
        for domain, rank in best.items():
            score[domain] = score.get(domain, 0.0) + 1.0 / rank
    web = lists["majestic"].keys() | lists["crux"].keys()
    return sorted((d for d in score if d in web), key=lambda d: (-score[d], d))[:limit]


def sources_md(counts, limit, today):
    lines = [
        "# Top sites",
        "",
        f"The {limit:,} most popular registrable domains, combined on {today} from the",
        "lists below with the Dowdall rule used by [Tranco](https://tranco-list.eu):",
        "a domain scores 1/rank in every list it appears in, and the scores add up.",
        "Only domains that Majestic or the Chrome UX Report know as websites are",
        "kept, so the DNS-based Umbrella list ranks sites without adding API hosts.",
        "Their icons are prerendered into this image. The combined list is",
        "`domains.txt`, licensed CC BY-SA 4.0 because it is derived from the Chrome",
        "UX Report.",
        "",
        "| Source | Domains used | License |",
        "|--------|--------------|---------|",
    ]
    for key, meta in SOURCES.items():
        lines.append(
            f"| [{meta['name']}]({meta['home']}) | {counts.get(key, 0):,} | "
            f"[{meta['license']}]({meta['license_url']}) |"
        )
    lines += [
        "",
        "Logos remain trademarks of their owners.",
        "",
    ]
    return "\n".join(lines)


def main():
    ap = argparse.ArgumentParser(description="Build the top-sites list for the favicons.page image")
    ap.add_argument("--limit", type=int, default=100_000)
    ap.add_argument("--out", default="domains.txt")
    ap.add_argument("--sources", default="SOURCES.md")
    ap.add_argument("--input", nargs=2, action="append", metavar=("SOURCE", "FILE"), default=[],
                    help="use a downloaded copy instead of fetching, e.g. --input majestic majestic.csv")
    ap.add_argument("--psl", help="a downloaded public_suffix_list.dat")
    args = ap.parse_args()

    local = {k: v for k, v in args.input}
    suffixes = Suffixes(open(args.psl, encoding="utf-8").read() if args.psl else fetch(PSL_URL).decode())
    lists, counts = {}, {}
    for key, meta in SOURCES.items():
        raw = open(local[key], "rb").read() if key in local else fetch(meta["url"])
        best = ranks(PARSERS[key](raw), suffixes)
        if not best:
            sys.exit(f"{key}: no domains parsed")
        lists[key] = best
        counts[key] = len(best)
        print(f"{key}: {len(best):,} registrable domains", file=sys.stderr)

    top = combine(lists, args.limit)
    with open(args.out, "w", encoding="utf-8") as f:
        f.write("\n".join(top) + "\n")
    today = datetime.date.today().isoformat()
    with open(args.sources, "w", encoding="utf-8") as f:
        f.write(sources_md(counts, len(top), today))
    print(f"wrote {len(top):,} domains to {args.out}", file=sys.stderr)


if __name__ == "__main__":
    main()
