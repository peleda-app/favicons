---
title: HTTPS
description: Built-in Let's Encrypt, fixed certificates and Cloudflare.
---

# HTTPS

favicons can terminate TLS itself and get its certificate from Let's Encrypt:

```toml
[tls]
enabled = true
domains = ["favicon.example.com"]
contact = ["mailto:ops@example.com"]
```

It listens on `:443` and `:80`. Port 80 answers `http-01` challenges and redirects everything else to HTTPS. Until the first certificate arrives it serves a self-signed one. The account, key and certificate are kept in `tls.state_dir`, so restarts reuse them instead of hitting Let's Encrypt rate limits, and renewal runs in the background 30 days before expiry.

Set `tls.cert` and `tls.key` to use a fixed certificate instead, for example a Cloudflare Origin CA certificate.

## Behind Cloudflare

With **Full (strict)**, the first certificate needs the challenge to reach the origin over plain HTTP: if **Always Use HTTPS** is on, add a rule that turns it off for `/.well-known/acme-challenge/*`. Once a valid certificate exists, renewals work through either port.

## Behind a proxy

Set `client_ip.source` to the header your trusted proxy sets. Otherwise every request looks like it comes from the proxy and shares one rate-limit bucket; set it only when such a proxy exists, or clients can spoof their subnet.
