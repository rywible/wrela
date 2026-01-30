# Wrela Runtime Primitives Spec (Implementation Guide)

This document describes the runtime primitives required to build Wrela Hub and the broader Wrela ecosystem. It is written for a junior engineer to implement step‑by‑step. It includes API design, storage models, FFI mapping, examples, and tradeoffs.

## Goals

- Provide **batteries‑included** primitives for real apps: auth, RBAC, files, jobs, scheduling, search, realtime, rate limiting, admin dashboard, env.
- Keep the Wrela language surface **simple and explicit**.
- Implement primitives in the runtime so any Wrela app can use them with zero extra infra.

## Conventions

- All primitives are **constructable classes** in Wrela (no static methods).
- Async methods return `Pending` so actors can `await` them.
- Most primitives should be **detachable pool classes** (like `Storage`) so they can run work in actor pools.
- `Env` is **not** a pool class (synchronous reads).
- Runtime exposes C FFI functions (`wr_*`) for compiler/runtime wiring.

## Module Overview (Runtime)

Rust modules to add/extend under `crates/runtime/src`:

- `env.rs` — `.env` loader + env var parsing
- `auth.rs` — user/password + JWT + OAuth + email verification tokens
- `rbac.rs` — roles + permissions
- `files.rs` — ACL, signed URLs, streaming I/O
- `jobs.rs` — queue + DLQ
- `schedule.rs` — cron/delay/at
- `search.rs` — document index
- `realtime.rs` — websocket rooms
- `rate_limit.rs` — token bucket
- `admin.rs` — metrics/dashboard endpoint

Also add exports in `lib.rs` and compiler runtime mapping.

---

# 1) Env (non‑pool)

## Wrela API

```wrela
let env = Env()

env.get("PORT")                 // -> String?
env.get_or("PORT", "8080")      // -> String
env.get_as_bool("DEBUG")        // -> Bool?
env.get_as_int("TIMEOUT")        // -> Int?
env.set("TEST_MODE", "1")       // -> Bool
env.load(".env")                // -> Bool (auto on startup)
```

## Design Decisions

- `.env` is **auto‑loaded at runtime startup**.
- Precedence: `.env` < process env.
- `set()` is allowed for tests (in‑process only).
- This class is **not** a detachable pool (sync reads).

## FFI

```
wr_env_get(key)
wr_env_get_or(key, default)
wr_env_get_as_bool(key)
wr_env_get_as_int(key)
wr_env_set(key, val)
wr_env_load(path)
```

## Implementation Notes

- Use `dotenvy` (or simple parser) for `.env`.
- `get_as_bool`: accept `1/true/on` and `0/false/off` (case‑insensitive).
- `get_as_int`: parse base‑10, return nil on failure.

---

# 2) Auth (JWT + OAuth + Email Verification)

## Wrela API

```wrela
let auth = Auth()

auth.create_user(email, username, password) -> Pending<User>
auth.verify_password(user_id, password) -> Pending<Bool>
auth.issue_jwt(user_id, claims?, ttl_secs?) -> Pending<String>
auth.verify_jwt(token) -> Pending<Map?>
auth.issue_email_token(user_id, ttl_secs?) -> Pending<String>
auth.verify_email_token(token) -> Pending<String?>
auth.oauth_login(provider, code) -> Pending<User>
```

## Design Decisions

- Password hashing in runtime (argon2 preferred).
- Email verification required.
- OAuth providers first: GitHub + Google.

## Storage Model

- `auth:user:<id>` → `{ email, username, pw_hash, verified, created_at }`
- `auth:email:<email>` → `user_id`
- `auth:email_token:<token>` → `{ user_id, exp }`
- Optional `auth:jwt:<jti>` if revocation is needed later.

## Tradeoffs

- JWT is stateless and fast, but revocation is hard without a denylist.
- Using expiring tokens for email verification avoids DB growth.

---

# 3) RBAC (Roles/Permissions)

## Wrela API

```wrela
let rbac = RBAC()

rbac.create_role(scope, name, permissions) -> Pending<String>
rbac.assign_role(user_id, role_id, scope_id) -> Pending<Bool>
rbac.check(user_id, permission, scope_id) -> Pending<Bool>
rbac.permissions_for(user_id, scope_id) -> Pending<List<String>>
```

## Design Decisions

- Permissions are **strings**: `"repo:write"`, `"org:admin"`.
- Scopes: `global`, `org`, `project`.

## Storage Model

- `rbac:role:<id>` → `{ scope, name, permissions[] }`
- `rbac:assign:<scope_id>:<user_id>` → `[role_id...]`

---

# 4) Files (ACL + Signed URLs + Streaming)

## Wrela API

```wrela
let files = Files()

files.upload_stream(stream, opts) -> Pending<String>
files.signed_url(file_id, opts) -> Pending<String>
files.metadata(file_id) -> Pending<Map?>
files.delete(file_id) -> Pending<Bool>
files.set_acl(file_id, acl) -> Pending<Bool>
```

**opts**: `{ acl, owner_id, content_type, ttl }`

## Design Decisions

- Runtime‑level ACLs: `public` / `private`.
- Signed URLs required for private access.
- Streaming uploads required for large artifacts.

## Storage Model

- metadata: `files:<id>` → `{ owner_id, acl, size, content_type, created_at }`
- bytes stored in BlobBackend

## Tradeoffs

- Signed URLs reduce server bandwidth, but require clock sync and URL TTLs.

---

# 5) Jobs / Queue

## Wrela API

```wrela
let jobs = Jobs()

jobs.enqueue(queue, payload, opts?) -> Pending<String>
jobs.process(queue, handler) -> Pending<Bool>
jobs.dead_letter(queue) -> Pending<List<Map>>
```

**opts**: `{ delay_secs?, max_retries?, backoff? }`

## Design Decisions

- At‑least‑once processing.
- Exponential backoff + max retries.
- Dead‑letter queue (DLQ) on max retry.
- Integration with actor pools (detach semantics).

---

# 6) Scheduler

## Wrela API

```wrela
let schedule = Schedule()

schedule.cron(expr, job) -> Pending<Bool>
schedule.every(seconds, job) -> Pending<Bool>
schedule.at(timestamp, job) -> Pending<Bool>
```

## Design Decisions

- Cron support built in (simple parser is fine).
- Scheduling jobs uses same queue/worker pipeline.

---

# 7) Search (Document Index)

## Wrela API

```wrela
let search = Search()

search.index(collection, id, text, fields?) -> Pending<Bool>
search.remove(collection, id) -> Pending<Bool>
search.query(collection, query, opts?) -> Pending<List<Map>>
```

**opts**: `{ filters?, sort?, limit?, offset? }`

## Design Decisions

- Generic document index (collection + text + fields).
- Keep model flexible for apps (users, issues, docs, etc.).

---

# 8) Realtime (WebSocket Rooms)

## Wrela API

```wrela
let realtime = Realtime()

realtime.on_connect(handler) -> Pending<Bool>
realtime.join(socket_id, room) -> Pending<Bool>
realtime.leave(socket_id, room) -> Pending<Bool>
realtime.broadcast(room, message) -> Pending<Bool>
realtime.send(socket_id, message) -> Pending<Bool>
```

## Design Decisions

- Room/channel abstraction for ergonomics.
- Message payload should support `Bytes`.

---

# 9) Rate Limiting

## Wrela API

```wrela
let rate = RateLimit()

rate.check(key, opts) -> Pending<Bool>
rate.ip(request) -> String
```

**opts**: `{ burst, per_secs }`

## Design Decisions

- Token bucket per key.
- Built‑in `rate.ip()` helper for HTTP requests.

---

# 10) Admin Dashboard

## Wrela API

```wrela
let admin = Admin()

admin.enable(opts) -> Pending<Bool>
```

**opts**: `{ bind_addr, auth }`

## Design Decisions

- Separate HTTP endpoint served by runtime.
- Exposes metrics, job queue status, backup state.

---

# Cross‑Cutting: Metrics + Backups

- Backup metrics already exist:
  - `METRIC_STORAGE_BACKUP_SUCCESS`
  - `METRIC_STORAGE_BACKUP_FAILURE`
  - `METRIC_STORAGE_BACKUP_LAST_DURATION_NS`
  - `METRIC_STORAGE_BACKUP_LAST_SIZE`
  - `METRIC_STORAGE_BACKUP_LAST_TS`
  - `METRIC_STORAGE_BACKUP_RESTORE_FAILURE`
- Checksums: each backup has `<key>.sha256`.

---

# Implementation Plan (Suggested Order)

Below is a step‑by‑step plan per phase so we can track concrete tasks and not get lost later.

## Phase 1 — Core App Primitives (MVP)

### 1. Env
1) Add `env.rs` with `.env` parser + load on runtime init.
2) Implement getters: `get`, `get_or`, `get_as_bool`, `get_as_int`.
3) Implement `set()` for tests (process‑local only).
4) Add FFI exports in `lib.rs`.
5) Add Wrela stdlib `Env` class in `core.wr`.
6) Tests: `.env` precedence, parsing edge cases.

### 2. Auth
1) Add `auth.rs` runtime module.
2) Data model in KvStore: user, email lookup, email token.
3) Password hashing (argon2/bcrypt).
4) JWT issuing + verification (HS256 by default).
5) Email token issue/verify.
6) FFI exports + Wrela `Auth` class.
7) Tests: password verification, token expiry, email verify flow.

### 3. RBAC
1) Add `rbac.rs`.
2) Store roles + assignments in KvStore.
3) Implement `check()` and `permissions_for()`.
4) FFI exports + Wrela `RBAC` class.
5) Tests: role assignment + permission checks across scopes.

### 4. Files
1) Add `files.rs` module and metadata table in KvStore.
2) Add ACL enforcement (public/private).
3) Signed URL generation + validation.
4) Streaming upload support (HTTP body to blob).
5) FFI exports + Wrela `Files` class.
6) Tests: ACL enforcement, signed URL expiry, streaming upload.

### 5. Jobs
1) Add `jobs.rs` module + queue storage in KvStore.
2) Implement enqueue + retries + DLQ.
3) Integrate with detach actor pool semantics.
4) FFI exports + Wrela `Jobs` class.
5) Tests: retry policy, DLQ, at‑least‑once semantics.

### 6. Scheduler
1) Add `schedule.rs` with cron + delayed jobs.
2) Store scheduled jobs in KvStore.
3) Use same worker pipeline as jobs.
4) FFI exports + Wrela `Schedule` class.
5) Tests: cron timing, delayed jobs.

## Phase 2 — Community Scale

### 7. Search
1) Add `search.rs` and schema for document index.
2) Tokenizer + inverted index storage.
3) Query parsing + ranking.
4) FFI exports + Wrela `Search` class.
5) Tests: indexing, filters, ranking.

### 8. Realtime
1) Add websocket server integration in runtime.
2) Implement rooms/channels + message routing.
3) Backpressure + disconnect handling.
4) FFI exports + Wrela `Realtime` class.
5) Tests: join/leave/broadcast/send.

### 9. Rate Limiting
1) Add `rate_limit.rs` with token bucket.
2) Store counters in KvStore (or in‑memory + periodic flush).
3) FFI exports + Wrela `RateLimit` class.
4) Tests: burst behavior, key isolation.

### 10. Admin Dashboard
1) Add `admin.rs` HTTP endpoint in runtime.
2) Surface metrics: backups, jobs, storage, errors.
3) Basic auth guard.
4) Tests: endpoint access + auth.

---

# Example: Wrela Hub (high‑level flow)

```wrela
let env = Env()
let auth = detach Auth() * 1
let rbac = detach RBAC() * 1
let files = detach Files() * 1
let jobs = detach Jobs() * 1
let schedule = detach Schedule() * 1
let search = detach Search() * 1
let realtime = detach Realtime() * 1
let admin = detach Admin() * 1

admin.enable({ bind_addr: env.get_or("ADMIN_ADDR", "127.0.0.1:9090") })

// Signup
let user = await auth.create_user(email, username, password)
let token = await auth.issue_email_token(user.id)

// Upload build
let file_id = await files.upload_stream(stream, { acl: "public" })
let url = await files.signed_url(file_id, { method: "GET", ttl: 3600 })

// Search index
await search.index("projects", project_id, "Wrela runtime", { tags: ["runtime"] })
```

---

# Open Questions

- OAuth callback flow: app‑level or runtime‑managed?
- Search backend: embedded index vs external (future).
- Realtime scalability across nodes (future: pub/sub layer).

---

If you want, I can turn this into per‑module implementation tickets and start scaffolding the Rust modules + Wrela stdlib classes.
