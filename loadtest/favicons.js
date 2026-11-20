import http from "k6/http";
import { check, sleep } from "k6";
import { Counter, Rate } from "k6/metrics";

const BASE = __ENV.BASE_URL || "http://127.0.0.1:8080";
const SCALE = Number(__ENV.SCALE || 1);
const DURATION = __ENV.DURATION || "30s";

const POPULAR = [
  "github.com",
  "wikipedia.org",
  "mozilla.org",
  "rust-lang.org",
  "cloudflare.com",
  "debian.org",
  "python.org",
  "kernel.org",
];
const STYLES = ["color", "mono"];

const honestOk = new Rate("honest_real_icon");
const serverErrors = new Counter("server_errors");
const attackerLimited = new Rate("attacker_limited");

const pick = (a) => a[Math.floor(Math.random() * a.length)];
const randomLabel = (n) => {
  const chars = "abcdefghijklmnopqrstuvwxyz0123456789";
  let s = "";
  for (let i = 0; i < n; i++) s += chars[Math.floor(Math.random() * chars.length)];
  return s;
};
const honestIp = () =>
  `10.${1 + Math.floor(Math.random() * 250)}.${Math.floor(Math.random() * 250)}.${1 + Math.floor(Math.random() * 250)}`;
const ATTACKER_IP = () => `203.0.113.${1 + Math.floor(Math.random() * 250)}`;

const rate = (n) => Math.max(1, Math.round(n * SCALE));

const OUT = __ENV.OUT_DIR || ".";

export const options = {
  discardResponseBodies: true,
  summaryTrendStats: ["avg", "min", "med", "max", "p(95)", "p(99)"],
  setupTimeout: "120s",
  scenarios: {
    hot_cache: {
      executor: "constant-arrival-rate",
      exec: "hotCache",
      rate: rate(400),
      timeUnit: "1s",
      duration: DURATION,
      preAllocatedVUs: 50,
      maxVUs: 400,
    },
    junk_flood: {
      executor: "constant-arrival-rate",
      exec: "junkFlood",
      rate: rate(300),
      timeUnit: "1s",
      duration: DURATION,
      preAllocatedVUs: 30,
      maxVUs: 300,
    },
    attacker: {
      executor: "constant-arrival-rate",
      exec: "attacker",
      rate: rate(600),
      timeUnit: "1s",
      duration: DURATION,
      preAllocatedVUs: 50,
      maxVUs: 600,
    },
    honest_during_attack: {
      executor: "constant-arrival-rate",
      exec: "honest",
      rate: rate(50),
      timeUnit: "1s",
      duration: DURATION,
      preAllocatedVUs: 10,
      maxVUs: 100,
    },
  },
  thresholds: {
    "http_req_duration{scenario:hot_cache}": ["p(95)<50", "p(99)<150"],
    "http_req_failed{scenario:hot_cache}": ["rate<0.01"],
    "http_req_duration{scenario:honest_during_attack}": ["p(95)<100"],
    "http_req_duration{scenario:junk_flood}": ["p(95)<50"],
    honest_real_icon: ["rate>0.99"],
    attacker_limited: ["rate>0.5"],
    server_errors: ["count==0"],
  },
};

function icon(domain, ip, style = "color") {
  const res = http.get(`${BASE}/v1/icon/${style}/${domain}`, {
    headers: { "X-Forwarded-For": ip },
    tags: { name: "icon" },
    timeout: "10s",
  });
  if (res.status >= 500 && res.status !== 503) serverErrors.add(1);
  return res;
}

export function setup() {
  const deadline = Date.now() + 90_000;
  const pending = new Set(POPULAR);
  while (pending.size > 0 && Date.now() < deadline) {
    for (const d of [...pending]) {
      const res = icon(d, honestIp());
      const source = res.headers["X-Icon-Source"];
      const reason = res.headers["X-Icon-Fallback-Reason"];
      if (source === "cache" || source === "origin") pending.delete(d);
      else if (reason && reason !== "queued" && reason !== "overloaded") pending.delete(d);
    }
    if (pending.size > 0) sleep(1);
  }
  const warm = POPULAR.filter((d) => {
    const res = icon(d, honestIp());
    return res.headers["X-Icon-Source"] === "cache";
  });
  if (warm.length === 0) throw new Error("no popular domain could be warmed up");
  return { warm };
}

export function hotCache(data) {
  const res = icon(pick(data.warm), honestIp(), pick(STYLES));
  check(res, {
    "hot: 200": (r) => r.status === 200,
    "hot: served from cache": (r) => r.headers["X-Icon-Source"] === "cache",
  });
}

export function junkFlood() {
  const junk = pick([
    () => `${randomLabel(8)}.invalid`,
    () => `10.0.0.${Math.floor(Math.random() * 255)}`,
    () => "localhost",
    () => `${randomLabel(12)}.blocked-loadtest.com`,
    () => `${randomLabel(40)}`,
  ])();
  const res = icon(junk, honestIp());
  check(res, {
    "junk: answered without fetching": (r) =>
      r.status === 429 ||
      ["invalid-domain", "blocklisted"].includes(r.headers["X-Icon-Fallback-Reason"]),
  });
}

export function attacker() {
  const res = icon(`${randomLabel(10)}${randomLabel(6)}.com`, ATTACKER_IP());
  attackerLimited.add(res.status === 429);
  check(res, {
    "attacker: never a server error": (r) => r.status < 500 || r.status === 503,
  });
}

export function honest(data) {
  const res = icon(pick(data.warm), honestIp(), pick(STYLES));
  honestOk.add(res.status === 200 && res.headers["X-Icon-Source"] === "cache");
}

export function handleSummary(data) {
  const m = data.metrics;
  const val = (name, key) => {
    const metric = m[name];
    if (!metric || metric.values[key] === undefined) return "n/a";
    const v = metric.values[key];
    return typeof v === "number" ? (Number.isInteger(v) ? String(v) : v.toFixed(2)) : String(v);
  };
  const scen = (s, key) => val(`http_req_duration{scenario:${s}}`, key);
  const failed = Object.entries(data.metrics)
    .filter(([, v]) => v.thresholds && Object.values(v.thresholds).some((t) => !t.ok))
    .map(([k]) => k);
  const md = [
    "## favicons load test",
    "",
    `Total requests: **${val("http_reqs", "count")}** (${val("http_reqs", "rate")}/s)`,
    "",
    "| Scenario | p95 ms | p99 ms |",
    "|----------|-------:|-------:|",
    `| hot_cache | ${scen("hot_cache", "p(95)")} | ${scen("hot_cache", "p(99)")} |`,
    `| junk_flood | ${scen("junk_flood", "p(95)")} | ${scen("junk_flood", "p(99)")} |`,
    `| honest_during_attack | ${scen("honest_during_attack", "p(95)")} | ${scen("honest_during_attack", "p(99)")} |`,
    "",
    `Honest clients got a real icon: **${(Number(val("honest_real_icon", "rate")) * 100).toFixed(2)}%**`,
    `Attacker requests rate-limited: **${(Number(val("attacker_limited", "rate")) * 100).toFixed(2)}%**`,
    `Server errors (5xx except 503): **${val("server_errors", "count")}**`,
    "",
    failed.length ? `❌ Failed thresholds: ${failed.join(", ")}` : "✅ All thresholds passed",
    "",
  ].join("\n");
  return {
    stdout: md,
    [`${OUT}/summary.md`]: md,
    [`${OUT}/summary.json`]: JSON.stringify(data, null, 2),
  };
}
