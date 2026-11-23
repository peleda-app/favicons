---
title: Hardening
description: Operational checklist, signed builds and reporting vulnerabilities.
---

# Hardening

## Operations

- Run as non-root with a read-only root filesystem and drop all capabilities.
- Put a CDN in front: `CDN-Cache-Control` defaults to 30 days for real icons.
- Egress firewall: allow only TCP 80/443 outbound plus DNS, a second line of defence against SSRF.
- Expose `/metrics` only on `server.admin_listen` bound to a private interface.
- Set `client_ip.source` only when a trusted proxy sets that header.

Possible future work: user namespaces or gVisor (`runsc`) for the renderer, proof-of-work or signed URLs for anonymous high-volume clients.

## Verifying builds

Every published image and release binary is signed and carries a signed SLSA build provenance attestation (Sigstore, keyless, issued to this repository's GitHub Actions workflows). Images also carry BuildKit provenance and an SPDX SBOM.

```bash
gh attestation verify oci://ghcr.io/peleda-app/favicons:latest --repo peleda-app/favicons
cosign verify ghcr.io/peleda-app/favicons:latest \
  --certificate-identity-regexp '^https://github.com/peleda-app/favicons/\.github/workflows/' \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com
```

Release binaries come with `SHA256SUMS` and a Sigstore bundle per file:

```bash
gh attestation verify favicons-1.3.0-x86_64-linux-musl --repo peleda-app/favicons
cosign verify-blob favicons-1.3.0-x86_64-linux-musl \
  --bundle favicons-1.3.0-x86_64-linux-musl.sigstore.json \
  --certificate-identity-regexp '^https://github.com/peleda-app/favicons/\.github/workflows/docker\.yml@refs/tags/v' \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com
```

The npm package is published from `.github/workflows/npm.yml` with npm provenance; `npm audit signatures` checks it. Images can be pinned by the verified digest.

## Reporting a vulnerability

Use GitHub's private vulnerability reporting on this repository rather than a public issue. Every instance points there from `/.well-known/security.txt`.
