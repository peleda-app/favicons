#!/usr/bin/env python3
"""Capture real API responses from a running favicons instance for the demo page.

    ./site/capture.py http://127.0.0.1:8080
"""
import json
import pathlib
import sys
import time
import urllib.error
import urllib.request

BASE = (sys.argv[1] if len(sys.argv) > 1 else "http://127.0.0.1:8080").rstrip("/")
OUT = pathlib.Path(__file__).parent

PRESETS = [
    ("github.com", "theSVG catalog mark (CC0)"),
    ("wikipedia.org", "Traced from the site's own icon"),
    ("spotify.com", "Owner's BIMI logo from DNS"),
    ("figma.com", "Owner's BIMI logo from DNS"),
    ("notion.so", "Owner's BIMI logo from DNS"),
    ("mail.google.com", "Subdomain: 301 to google.com"),
    ("user-48213.github.io", "Tracking subdomain: 301 to github.io"),
    ("evil-example.com", "Blocklisted domain"),
    ("nope-zzzq-xx.com", "Unreachable site, identicon"),
    ("127.0.0.1", "Not a public domain"),
]
KEEP = ("content-type", "etag", "cache-control", "cdn-cache-control", "location")


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, *args, **kwargs):
        return None


OPENER = urllib.request.build_opener(NoRedirect)


def get(path):
    req = urllib.request.Request(BASE + path, headers={"X-Forwarded-For": "198.51.100.7"})
    try:
        with OPENER.open(req, timeout=30) as r:
            return r.status, dict(r.headers.items()), r.read()
    except urllib.error.HTTPError as e:
        return e.code, dict(e.headers.items()), e.read()


def kept(headers):
    out = {k.lower(): v for k, v in headers.items() if k.lower() in KEEP or k.lower().startswith("x-icon-")}
    return dict(sorted(out.items()))


def location(headers):
    return next(v for k, v in headers.items() if k.lower() == "location")


def slug(domain):
    return domain.replace(".", "-")


def warm(path):
    for _ in range(40):
        status, headers, _ = get(path)
        if status in (301, 302):
            path = location(headers)
            continue
        if headers.get("x-icon-fallback-reason") not in ("queued", "overloaded"):
            return
        time.sleep(1)


def main():
    presets = []
    for domain, note in PRESETS:
        warm(f"/v1/icon/color/{domain}")
        responses = {}
        for style in ("color", "mono"):
            path = f"/v1/icon/{style}/{domain}"
            status, headers, body = get(path)
            redirect = None
            if status in (301, 302):
                redirect = {"request": f"GET {path}", "status": status, "headers": kept(headers)}
                path = location(headers)
                status, headers, body = get(path)
            image = f"icons/{slug(domain)}/{style}.svg"
            (OUT / image).parent.mkdir(parents=True, exist_ok=True)
            (OUT / image).write_bytes(body)
            responses[style] = {
                "redirect": redirect,
                "request": f"GET {path}",
                "status": status,
                "headers": kept(headers),
                "image": image,
                "bytes": len(body),
            }
        presets.append({"domain": domain, "note": note, "responses": responses})
        first = responses["color"]
        hop = f"301 -> {first['redirect']['headers'].get('location')}" if first["redirect"] else ""
        print(f"{domain:24} {first['headers'].get('x-icon-source')} {first['headers'].get('x-icon-fallback-reason', '')} {hop}")
    (OUT / "data").mkdir(exist_ok=True)
    (OUT / "data" / "presets.json").write_text(json.dumps(presets, indent=1) + "\n")


if __name__ == "__main__":
    main()
