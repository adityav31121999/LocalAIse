# LocalAIse v0.2.0 — Enterprise Scaling Plan

**Target:** Enable team deployment, compliance, and operational maturity while preserving local-first architecture.

---

## 🎯 Scope & Principles

| Principle | Application |
|-----------|-------------|
| **Local-first remains default** | All enterprise features opt-in; single-user mode unchanged |
| **No external dependencies by default** | SSO, audit, metrics integrate with existing infra (OIDC, Syslog, Prometheus) |
| **Air-gap compatible** | Every feature works offline; cloud sync is a separate plugin |
| **Backward compatible** | v0.1.3 projects open without migration |

---

## 🏗️ Epic Breakdown

### Epic 1: Multi-Project Workspace & Team Access
| Ticket | Description | Effort |
|--------|-------------|--------|
| `ENT-101` | **Workspace container** — `workspaces/` dir with `workspace.json` (name, projects[], settings, members) | M |
| `ENT-102` | **Project sharing** — Symlink-based project references across workspaces; per-project ACL (read/write/admin) | M |
| `ENT-103` | **User profiles** — Local user DB (SQLite) with display name, avatar, preferences; no auth yet | S |
| `ENT-104` | **Invite flow** — Generate `workspace.invite` file (signed JWT) for air-gap transfer; CLI `localaise invite accept` | M |

### Epic 2: Authentication & Authorization
| Ticket | Description | Effort |
|--------|-------------|--------|
| `ENT-201` | **OIDC/OAuth2 client** — Configure via `workspace.json`; PKCE; token refresh; maps `sub` → local user | L |
| `ENT-202` | **SSO enforcement flag** — `auth.required: true` blocks unauthenticated Tauri commands | M |
| `ENT-203` | **RBAC** — Roles: `owner`, `admin`, `member`, `viewer`; permissions matrix per command | M |
| `ENT-204` | **Service accounts** — Long-lived tokens for CI/CD, bots; scoped to project/workspace | S |
| `ENT-205` | **Session management** — Idle timeout, concurrent session limit, revocation list | S |

### Epic 3: Audit & Compliance
| Ticket | Description | Effort |
|--------|-------------|--------|
| `ENT-301` | **Structured audit log** — JSONL at `workspaces/{id}/audit/YYYY-MM-DD.jsonl`; fields: `ts, user, action, resource, outcome, metadata` | M |
| `ENT-302` | **Immutable append-only** — Optional WORM mode (filesystem attr + hash chain) | M |
| `ENT-303` | **Syslog/HTTP forwarder** — Configurable batch shipper (RFC 5424 / Elastic / Splunk HEC) | M |
| `ENT-304` | **Data retention policy** — TTL per data type (chats, research, uploads); auto-purge job | S |
| `ENT-305` | **Export/DSAR tool** — `localaise compliance export --user --format=json|zip` for GDPR/CCPA | M |

### Epic 4: Data Governance & Security
| Ticket | Description | Effort |
|--------|-------------|--------|
| `ENT-401` | **Encryption at rest** — Per-workspace AES-256-GCM key (derived from master key + workspace salt); master key in OS keyring / HSM / env var | L |
| `ENT-402` | **Model registry signing** — Cosign/Notary verification of model artifacts; policy: `allow_unsigned: false` | M |
| `ENT-403` | **Network egress control** — Allowlist domains/CIDRs for research fetch; deny-by-default mode | M |
| `ENT-404` | **Secrets manager integration** — HashiCorp Vault, AWS Secrets Manager, Azure Key Vault providers for API keys | L |
| `ENT-405` | **SBOM generation** — `localaise sbom` → CycloneDX JSON for all deps (Rust + npm) | S |

### Epic 5: Scalability & Performance
| Ticket | Description | Effort |
|--------|-------------|--------|
| `ENT-501` | **Project sharding** — Large projects split across multiple `data/` subdirs; unified virtual FS view | L |
| `ENT-502` | **Async indexing pipeline** — Background worker pool (configurable concurrency); progress API | M |
| `ENT-503` | **Research connection pooling** — Shared HTTP client pool with per-host limits, retries, circuit breaker | M |
| `ENT-504` | **GPU/CPU resource quotas** — cgroups v2 integration; per-project `max_vram_gb`, `max_cpu_cores` | L |
| `ENT-505` | **Horizontal CLI** — `localaise daemon start` runs headless server; Tauri UI connects via local gRPC | XL |

### Epic 6: Observability & Operations
| Ticket | Description | Effort |
|--------|-------------|--------|
| `ENT-601` | **Prometheus metrics endpoint** — `/metrics` on admin port; counters: `requests_total`, `tokens_consumed`, `research_runs`, `errors_total` | M |
| `ENT-602` | **Health/readiness probes** — `/healthz` (liveness), `/readyz` (model loaded, index ready) | S |
| `ENT-603` | **Structured logging** — `tracing` + `tracing-subscriber` JSON; correlation IDs across Tauri commands | M |
| `ENT-604` | **Admin CLI** — `localaise admin {users,projects,audit,config} ...` for headless management | L |
| `ENT-605` | **Backup/Restore** — `localaise backup create --output=/path` (encrypted tarball); point-in-time restore | M |

### Epic 7: Deployment & Distribution
| Ticket | Description | Effort |
|--------|-------------|--------|
| `ENT-701` | **Docker image** — Multi-stage `Dockerfile` (builder → runtime); non-root; distroless base | M |
| `ENT-702` | **Helm chart** — K8s deployment with PVC, ConfigMap, Secret, Ingress; values for air-gap | L |
| `ENT-703` | **Offline install bundle** — `localaise-<ver>-offline.tar.gz` with all deps, models, WASM | M |
| `ENT-704` | **Auto-update policy** — `update.channel: stable|candidate|none`; signed releases; admin approval gate | M |

### Epic 8: Integration & Extensibility
| Ticket | Description | Effort |
|--------|-------------|--------|
| `ENT-801` | **REST API gateway** — Optional HTTP server (axum) exposing `/api/v1/*` with OpenAPI spec | XL |
| `ENT-802` | **Webhook system** — Events: `research.completed`, `project.created`, `user.added`; retry + dead-letter | M |
| `ENT-803` | **Plugin SDK (stable)** — WASM component model; host functions: FS, LLM, Net, Audit; marketplace manifest | XL |
| `ENT-804` | **Notion/Confluence/GDrive sync** — Optional connectors (opt-in, per-project) | L |

---

## 📦 Release Phasing

| Phase | Target | Tickets |
|-------|--------|---------|
| **0.2.0-alpha** | Internal dogfood | ENT-101, 102, 103, 201, 202, 301, 401, 601, 602 |
| **0.2.0-beta** | Design partners | + ENT-104, 203, 204, 302, 303, 402, 403, 501, 502, 603, 604, 701 |
| **0.2.0-rc** | Security audit | + ENT-205, 304, 305, 404, 405, 503, 504, 605, 702, 703 |
| **0.2.0** | GA | + ENT-505, 704, 801, 802 (803, 804 → 0.3.0) |

---

## 🔧 Technical Architecture Changes

```
localAIse/
├── workspaces/                    ← NEW
│   ├── acme-corp/
│   │   ├── workspace.json         # config, members, auth, policies
│   │   ├── audit/                 # JSONL logs
│   │   ├── keys/                  # per-ws encryption keys (encrypted)
│   │   └── projects/              # symlinks → /projects/*
│   └── ...
├── projects/                      ← unchanged (shared storage)
│   ├── project-a/
│   └── project-b/
├── users.db                       ← NEW (SQLite, local users + sessions)
├── config/
│   ├── enterprise.toml            # global enterprise settings
│   └── secrets.toml               # vault refs (not actual secrets)
└── plugins/                       ← NEW (WASM components)
```

**Key Rust crates to add:**
- `jsonwebtoken`, `oidc` — auth
- `sqlx` (sqlite) — user/db
- `tracing`, `tracing-subscriber` — observability
- `axum` — admin API / webhooks
- `wasmtime` (component model) — plugins
- `cosign`/`notary` — model verification
- `reqwest` + `tower` — HTTP client with pooling/circuit breaker

---

## 🧪 Testing Strategy

| Layer | Approach |
|-------|----------|
| **Unit** | AuthZ matrix, audit serialization, encryption round-trip, RBAC logic |
| **Integration** | Multi-user workspace scenarios, OIDC flow (mock provider), backup/restore |
| **Chaos** | Network partition during research, model server OOM, disk full during indexing |
| **Compliance** | Immutable audit log tamper detection, DSAR export completeness, retention purge |
| **Performance** | 100-project workspace, 10GB project, 50 concurrent research runs |

---

## 📋 Documentation Deliverables

1. **Enterprise Admin Guide** — Deployment, auth config, audit, backup
2. **Security Hardening Checklist** — CIS-style benchmarks
3. **API Reference** — OpenAPI spec for REST + Tauri commands
4. **Plugin Development Guide** — WASM component SDK
5. **Air-Gap Installation Runbook** — Offline bundle, model import, key management

---

## ⚠️ Risks & Mitigations

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Encryption perf on large projects | Medium | High | Benchmark early; offer `encrypt: false` for non-sensitive workspaces |
| OIDC complexity for air-gap | High | Medium | Provide `localaise auth oidc-device-flow` + pre-registered clients |
| Plugin sandbox escape | Low | Critical | WASM component model + capability-based host functions only |
| Migration from 0.1.3 | Medium | High | Automated `localaise migrate` command; dry-run mode |
| Docker image size | Medium | Medium | Distroless + multi-stage; target <500MB |

---

## 🎯 Success Metrics (Post-GA)

| Metric | Target |
|--------|--------|
| Time to onboard 10-user team | <30 min |
| Audit log write latency (p99) | <5 ms |
| Research throughput (concurrent) | 20+/project |
| Encryption overhead (indexing) | <10% |
| Zero critical CVEs in deps | Continuous |

---

## 👥 Suggested Team Allocation

| Role | Focus |
|------|-------|
| 1 Backend (Rust) | Auth, audit, encryption, workspace core |
| 1 Backend (Rust) | Scalability: sharding, pooling, quotas, daemon |
| 1 Full-stack | Admin CLI, REST API, webhook system |
| 1 DevOps | Docker, Helm, offline bundle, CI/CD hardening |
| 1 Security | Threat model, pen-test, compliance docs |
| 1 QA | Integration/chaos/compliance test suites |

---

## 📅 Rough Timeline (6 engineers)

| Week | Milestone |
|------|-----------|
| 1-2 | Workspace core, user DB, project sharing |
| 3-4 | OIDC auth, RBAC, session mgmt |
| 5-6 | Audit log, Syslog forwarder, retention |
| 7-8 | Encryption at rest, model signing, egress control |
| 9-10 | Sharding, async indexing, connection pooling |
| 11-12 | Prometheus, health probes, structured logging |
| 13-14 | Docker, Helm, offline bundle, admin CLI |
| 15-16 | Beta with design partners, security audit |
| 17-18 | RC, bug bash, docs, GA |

---

## 📝 Appendix: Workspace Configuration Schema (Draft)

```json
{
  "$schema": "https://localaise.dev/schemas/workspace-v1.json",
  "name": "acme-corp",
  "version": 1,
  "projects": [
    { "name": "project-a", "path": "../projects/project-a", "acl": "read-write" },
    { "name": "project-b", "path": "../projects/project-b", "acl": "read-only" }
  ],
  "members": [
    { "sub": "user:alice", "role": "owner" },
    { "sub": "user:bob", "role": "member" },
    { "sub": "svc:ci-bot", "role": "service-account", "scopes": ["research:run"] }
  ],
  "auth": {
    "provider": "oidc",
    "issuer": "https://auth.acme.corp",
    "client_id": "localaise-workspace",
    "required": true,
    "device_flow": true
  },
  "audit": {
    "enabled": true,
    "worm": false,
    "forward": {
      "type": "syslog",
      "endpoint": "udp://logs.acme.corp:514",
      "batch_size": 100
    },
    "retention_days": 2555
  },
  "encryption": {
    "enabled": true,
    "kms": "os-keyring"
  },
  "network": {
    "egress_allowlist": ["*.wikipedia.org", "api.github.com", "10.0.0.0/8"],
    "deny_by_default": true
  },
  "resources": {
    "max_concurrent_research": 10,
    "gpu_quota_gb": 24,
    "cpu_quota_cores": 16
  },
  "update": {
    "channel": "stable",
    "require_admin_approval": true
  }
}
```

---

## 📝 Appendix: Audit Log Event Schema (Draft)

```json
{
  "timestamp": "2026-11-23T14:32:11.123Z",
  "workspace_id": "acme-corp",
  "user_id": "user:alice",
  "session_id": "sess_abc123",
  "action": "research.run.started",
  "resource": {
    "type": "project",
    "id": "project-a"
  },
  "outcome": "success",
  "metadata": {
    "question": "Q4 budget forecasting methods",
    "mode": "online",
    "budget_preset": "deep",
    "plan_steps": 7
  },
  "correlation_id": "corr_xyz789"
}
```

---

## 📝 Appendix: RBAC Permission Matrix (Draft)

| Command | Owner | Admin | Member | Viewer | Service Account |
|---------|-------|-------|--------|--------|-----------------|
| Workspace settings | ✅ | ✅ | ❌ | ❌ | ❌ |
| Invite users | ✅ | ✅ | ❌ | ❌ | ❌ |
| Manage roles | ✅ | ✅ | ❌ | ❌ | ❌ |
| Create project | ✅ | ✅ | ✅ | ❌ | ❌ |
| Delete project | ✅ | ✅ | ❌ | ❌ | ❌ |
| Run research | ✅ | ✅ | ✅ | ❌ | ✅* |
| Read research | ✅ | ✅ | ✅ | ✅ | ✅* |
| Export research | ✅ | ✅ | ✅ | ✅ | ✅* |
| Manage API keys | ✅ | ✅ | ❌ | ❌ | ❌ |
| View audit log | ✅ | ✅ | ❌ | ❌ | ❌ |
| Backup/Restore | ✅ | ✅ | ❌ | ❌ | ❌ |

*Scoped to permitted projects only

---

**Document Version:** 0.1 (Draft)  
**Last Updated:** 2026-11-23  
**Author:** LocalAIse Core Team  
**Status:** Planning — not yet implemented