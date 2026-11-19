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
    ("github.com", "Pinned-tab mask icon, CDN asset"),
    ("wikipedia.org", "Classic apple-touch-icon"),
    ("spotify.com", "Icon on a foreign CDN, checked first"),
    ("figma.com", "Mono derived from a dark tile"),
    ("notion.so", "Mono derived from a light tile"),
    ("outlook.live.com", "Vaultwarden alias to microsoft.com"),
    ("user-48213.github.io", "Tracking subdomain collapsed"),
    ("evil-example.com", "Blocklisted domain"),
    ("nope-zzzq-xx.com", "Unreachable site, identicon"),
    ("127.0.0.1", "Not a public domain"),
]
SIZES = [16, 32, 64, 128]
KEEP = ("content-type", "etag", "cache-control", "cdn-cache-control")


def get(path):
    req = urllib.request.Request(BASE + path, headers={"X-Forwarded-For": "198.51.100.7"})
    try:
        with urllib.request.urlopen(req, timeout=30) as r:
            return r.status, dict(r.headers.items()), r.read()
    except urllib.error.HTTPError as e:
        return e.code, dict(e.headers.items()), e.read()


def slug(domain):
    return domain.replace(".", "-")


def main():
    presets = []
    for domain, note in PRESETS:
        path = f"/v1/icon/{domain}?size=128"
        for _ in range(40):
            status, headers, _ = get(path)
            if headers.get("x-icon-fallback-reason") not in ("queued", "overloaded"):
                break
            time.sleep(1)
        responses = {}
        variants = [("color", t, s) for t in ("light", "dark") for s in SIZES]
        variants += [("mono", "light", s) for s in SIZES]
        for style, theme, size in variants:
            query = f"size={size}&style={style}" + (f"&theme={theme}" if style == "color" else "")
            status, headers, body = get(f"/v1/icon/{domain}?{query}")
            key = f"{style}-{theme}-{size}" if style == "color" else f"mono-{size}"
            ext = "webp" if headers.get("content-type") == "image/webp" else "png"
            image = f"icons/{slug(domain)}/{key}.{ext}"
            (OUT / image).parent.mkdir(parents=True, exist_ok=True)
            (OUT / image).write_bytes(body)
            kept = {k.lower(): v for k, v in headers.items() if k.lower() in KEEP or k.lower().startswith("x-icon-")}
            responses[key] = {
                "request": f"GET /v1/icon/{domain}?{query}",
                "status": status,
                "headers": dict(sorted(kept.items())),
                "image": image,
                "bytes": len(body),
            }
        presets.append({"domain": domain, "note": note, "responses": responses})
        first = responses["color-light-128"]["headers"]
        print(f"{domain:24} {first.get('x-icon-source')} {first.get('x-icon-fallback-reason', '')}")
    (OUT / "data").mkdir(exist_ok=True)
    (OUT / "data" / "presets.json").write_text(json.dumps(presets, indent=1) + "\n")


if __name__ == "__main__":
    main()
