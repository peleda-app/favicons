import json
import os
import sys
import time
import urllib.request
from datetime import datetime, timedelta, timezone

KEEP = 10
HIT = {"hit", "stale", "revalidated", "updating"}


def fetch(url, data=None, headers=None):
    req = urllib.request.Request(url, data=data, headers={"user-agent": "favicons-stats", **(headers or {})})
    with urllib.request.urlopen(req, timeout=30) as r:
        return json.load(r)


def load(path, fallback_url):
    try:
        with open(path) as f:
            return json.load(f)
    except (OSError, ValueError):
        pass
    try:
        return fetch(f"{fallback_url}?v={int(time.time())}")
    except Exception:
        return {}


def iso(t):
    return t.strftime("%Y-%m-%dT%H:%M:%SZ")


def cloudflare_hits(token, zone, since, until):
    query = """query($zone: String!, $since: Time!, $until: Time!, $path: String!) {
      viewer { zones(filter: {zoneTag: $zone}) {
        httpRequestsAdaptiveGroups(limit: 50, filter: {datetime_geq: $since, datetime_lt: $until, requestSource: "eyeball", clientRequestPath_like: $path}) {
          count dimensions { cacheStatus }
        } } } }"""
    total = 0
    for path in ("/color/%", "/mono/%"):
        body = json.dumps({"query": query, "variables": {"zone": zone, "since": iso(since), "until": iso(until), "path": path}}).encode()
        r = fetch("https://api.cloudflare.com/client/v4/graphql", body, {"authorization": f"Bearer {token}", "content-type": "application/json"})
        if r.get("errors"):
            raise RuntimeError(r["errors"][0].get("message", "graphql error"))
        for zone_data in r["data"]["viewer"]["zones"]:
            for g in zone_data["httpRequestsAdaptiveGroups"]:
                if g["dimensions"]["cacheStatus"] in HIT:
                    total += g["count"]
    return total


def main():
    state_path, out_path, live_url, pages_url = sys.argv[1:5]
    now = datetime.now(timezone.utc).replace(second=0, microsecond=0)
    state = load(state_path, pages_url)
    total = int(state.get("total", 0))
    instances = state.get("instances", [])
    sources = state.get("sources", {})

    try:
        live = fetch(f"{live_url}?v={int(time.time())}")
        entry = next((i for i in instances if i["id"] == live["instance"]), None)
        if entry is None:
            entry = {"id": live["instance"], "started_at": live.get("started_at"), "count": 0}
            instances.append(entry)
        entry["count"] = max(int(entry.get("count", 0)), int(live["icons"]))
        entry["seen_at"] = iso(now)
    except Exception as e:
        print(f"live stats unavailable: {e}", file=sys.stderr)

    instances.sort(key=lambda i: i.get("seen_at", ""), reverse=True)
    for gone in instances[KEEP:]:
        total += int(gone.get("count", 0))
    instances = instances[:KEEP]

    token, zone = os.environ.get("CF_API_TOKEN"), os.environ.get("CF_ZONE_ID")
    if token and zone:
        cf = sources.get("cloudflare", {"label": "served from the Cloudflare cache", "count": 0})
        until = now - timedelta(minutes=5)
        since = datetime.fromisoformat(cf["until"].replace("Z", "+00:00")) if cf.get("until") else until - timedelta(minutes=10)
        since = max(since, until - timedelta(hours=23))
        if until > since:
            try:
                cf["count"] = int(cf.get("count", 0)) + cloudflare_hits(token, zone, since, until)
                cf["until"] = iso(until)
            except Exception as e:
                print(f"cloudflare analytics unavailable: {e}", file=sys.stderr)
        sources["cloudflare"] = cf

    out = {"generated_at": iso(now), "total": total, "instances": instances, "sources": sources}
    with open(out_path, "w") as f:
        json.dump(out, f, indent=1)
    print(json.dumps(out))


if __name__ == "__main__":
    main()
