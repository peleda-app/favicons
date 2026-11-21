#!/usr/bin/env bash
set -uo pipefail
tools="${1:-compare/tools.json}"
idle="${IDLE_SECS:-30}"

bytes() {
  awk '{ v=$1; u=$2;
    m["B"]=1; m["KiB"]=1024; m["MiB"]=1048576; m["GiB"]=1073741824; m["kB"]=1000; m["MB"]=1e6; m["GB"]=1e9;
    printf "%d", v * m[u] }' <<<"$(sed -E 's/^([0-9.]+)([A-Za-z]+).*/\1 \2/' <<<"$1")"
}

compressed() {
  local ref=$1 idx digest manifest
  idx=$(docker buildx imagetools inspect --raw "$ref") || return 1
  digest=$(jq -r '.manifests[]? | select(.platform.architecture=="amd64" and .platform.os=="linux") | .digest' <<<"$idx" | head -1)
  if [ -n "$digest" ]; then manifest=$(docker buildx imagetools inspect --raw "${ref%:*}@$digest"); else manifest=$idx; fi
  jq '[.layers[].size] | add' <<<"$manifest"
}

version() {
  local repo=$1 v
  v=$(gh api "repos/$repo/releases/latest" -q .tag_name 2>/dev/null) && [ -n "$v" ] && { echo "$v"; return; }
  gh api "repos/$repo/tags?per_page=1" -q '.[0].name' 2>/dev/null
}

out="[]"
while IFS= read -r row; do
  id=$(jq -r .id <<<"$row"); repo=$(jq -r .repo <<<"$row"); image=$(jq -r .image <<<"$row"); port=$(jq -r .port <<<"$row")
  echo "== $id" >&2
  meta=$(gh api "repos/$repo" -q '{stars: .stargazers_count, language: .language, license: .license.spdx_id, pushed_at: .pushed_at}' 2>/dev/null || echo '{}')
  ver=$(version "$repo")
  comp=$(compressed "$image" || echo null)
  mem=null; size=null; digest=null
  if docker pull -q --platform linux/amd64 "$image" >/dev/null; then
    size=$(docker image inspect -f '{{.Size}}' "$image")
    digest=$(docker image inspect -f '{{index .RepoDigests 0}}' "$image" | sed 's/.*@//')
    docker rm -f probe >/dev/null 2>&1
    if docker run -d --name probe --platform linux/amd64 -p "127.0.0.1:$port:$port" "$image" >/dev/null; then
      up=false
      for _ in $(seq 1 60); do
        code=$(curl -s -o /dev/null -m 2 -w '%{http_code}' "http://127.0.0.1:$port/") || true
        if [ "$code" != "000" ]; then up=true; break; fi
        sleep 1
      done
      if $up && [ "$(docker inspect -f '{{.State.Running}}' probe)" = true ]; then
        sleep "$idle"
        samples=()
        for _ in 1 2 3; do
          samples+=("$(bytes "$(docker stats --no-stream --format '{{.MemUsage}}' probe)")")
          sleep 5
        done
        mem=$(printf '%s\n' "${samples[@]}" | sort -n | sed -n 2p)
      else
        echo "$id did not come up" >&2
        docker logs probe 2>&1 | tail -20 >&2
      fi
      docker rm -f probe >/dev/null 2>&1
    fi
    docker image rm -f "$image" >/dev/null 2>&1
  fi
  out=$(jq --argjson row "$row" --argjson meta "$meta" --arg ver "$ver" --argjson comp "${comp:-null}" \
    --argjson size "${size:-null}" --arg digest "${digest:-}" --argjson mem "${mem:-null}" \
    '. + [$row + $meta + {version: (if $ver == "" then null else $ver end), image_compressed_bytes: $comp, image_bytes: $size, image_digest: (if $digest == "" then null else $digest end), idle_memory_bytes: $mem}]' <<<"$out")
done < <(jq -c '.[]' "$tools")
jq -n --argjson tools "$out" --arg at "$(date -u +%Y-%m-%dT%H:%M:%SZ)" --argjson idle "$idle" \
  '{generated_at: $at, platform: "linux/amd64", idle_seconds: $idle, tools: $tools}'
