# bolt-core-sdk — Documentation

## External Adopters

Start here: **[INTEGRATION_GUIDE.md](INTEGRATION_GUIDE.md)** — how to build
a Bolt-conformant app outside the core ecosystem (contract consumption,
transport adapters, signaling registration, minimal adopter path).

## Doc Routing

- **Canonical routing table:** [`bolt-ecosystem/os/rules/doc-routing.md`](https://github.com/the9ines/bolt-ecosystem/blob/main/os/rules/doc-routing.md)
- **Canonical audit tracker:** [`bolt-ecosystem/docs/AUDIT_TRACKER.md`](https://github.com/the9ines/bolt-ecosystem/blob/main/docs/AUDIT_TRACKER.md)

## Update Rules

- Only update this repo's `docs/CHANGELOG.md` (append-only). `docs/STATE.md` is retired — current state is generated at the ecosystem root (`os/bin/status.sh` → `os/DASHBOARD.md`).
- `docs/AUDIT_TRACKER.md` is a **stub** — do not edit it. The canonical tracker lives in bolt-ecosystem.
- All other governance docs are routed via `os/rules/doc-routing.md` in the ecosystem repo.
