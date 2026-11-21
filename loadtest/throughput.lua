local domains = {
  "github.com", "wikipedia.org", "mozilla.org", "rust-lang.org",
  "cloudflare.com", "debian.org", "python.org", "kernel.org",
}
local styles = { "color", "mono" }
local threads = 0

function setup(thread)
  threads = threads + 1
  thread:set("seed", threads)
end

function init()
  math.randomseed(os.time() * 1000 + (seed or 0))
end

function request()
  local path = "/" .. styles[math.random(#styles)] .. "/" .. domains[math.random(#domains)]
  local ip = "10." .. math.random(1, 250) .. "." .. math.random(0, 250) .. "." .. math.random(1, 250)
  return wrk.format("GET", path, { ["X-Forwarded-For"] = ip })
end

function done(summary, latency)
  local e = summary.errors
  io.write(string.format(
    "RESULT rps=%.0f p50=%.2f p99=%.2f p999=%.2f socket_errors=%d non2xx=%d\n",
    summary.requests / (summary.duration / 1e6),
    latency:percentile(50) / 1000,
    latency:percentile(99) / 1000,
    latency:percentile(99.9) / 1000,
    e.connect + e.read + e.write + e.timeout,
    e.status
  ))
end
