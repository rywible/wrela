# Wrela HTTP Framework Plan

Date: 2026-01-30

## Goals
- Ship a **first-class HTTP framework** with the runtime, optimized for Wrela users (no Rust exposure).
- Keep the **actor/await semantics intact**: only actor methods can `await`.
- Provide **explicit, readable API names** and a **great DX** with strong performance defaults.
- Keep internal vs public HTTP servers **separate** for deployment (Fly.io private networking).

## Non-Goals (initial)
- WebSockets, SSE, streaming bodies
- Complex routing (regex, glob) beyond static paths and simple params
- Full middleware DSL in Wrela

## High-Level Architecture

### Wrela Surface API (class-only)
```wrela
A HttpServer:
    can serve_get_requests(path: String, handler: HttpHandler) -> Nothing
    can serve_post_requests(path: String, handler: HttpHandler) -> Nothing
    can serve_requests(method: String, path: String, handler: HttpHandler) -> Nothing
    can serve_on(addr: String) -> Nothing
    can stop() -> Nothing

A HttpHandler:
    can handle(req: HttpRequest) -> HttpResponse

A HttpRequest:
    has:
        method: String
        path: String
        headers: Map
        query: Map
        params: Map
        body: Bytes

A HttpResponse:
    has:
        status: Number
        headers: Map
        body: Bytes

    can ok(text: String) -> HttpResponse
    can json(value: Any) -> HttpResponse
    can header(name: String, value: String) -> HttpResponse
```

### Runtime Bridge (Rust)
- Maintain a route registry: `(method, path) -> (actor_handle, method_id)`.
- On each HTTP request:
  1. Convert to `HttpRequest` value.
  2. `wr_actor_send` to handler.
  3. `wr_pending_await` to resolve `HttpResponse`.
  4. Convert to Axum response.
- `HttpServer.serve_on()` blocks (runs Axum with `runtime_block_on`).
- Optional `HttpServer.start_on()` can be added later to spawn without blocking.

## Internal vs Public Servers
- Keep the **internal Raft HTTP server** separate and private.
- Rename config for clarity:
  - `WRELA_INTERNAL_HTTP_BIND_ADDR` (alias `WRELA_RAFT_BIND_ADDR`)
  - `internal_http_enabled` (alias `WRELA_RAFT_HTTP_ENABLED`)
- Public server uses its own bind address and lifecycle.

## Performance Strategy
- **Concurrency via handler pools**: handlers are actors; use `Pool.of()` for fan-out.
- **Zero-copy body** where possible (`Bytes` in Wrela; `Bytes` in Rust).
- Avoid JSON parse unless user calls `HttpResponse.json()` or `req.json()`.
- Keep registry lookups fast (hash map of method+path).

## DX / “Awesome” Features
### Built-in helpers
- `HttpResponse.ok(text)`
- `HttpResponse.json(value)`
- `HttpResponse.text(text, status?)`
- `HttpResponse.empty(status?)`

### Default headers
- Always include `Server: Wrela` (or `Wrela/<version>` if available).
- For `HttpResponse.json`, set `Content-Type: application/json`.
- For `HttpResponse.text/ok`, set `Content-Type: text/plain; charset=utf-8`.
- If `HttpResponse.body` is non-empty and no `Content-Type` is set, default to `application/octet-stream`.
- If user sets `headers`, they override defaults (user header wins).

### Convenience request helpers
- `req.header(name)`
- `req.param(name)`
- `req.query(name)`
- `req.text()` / `req.json()` (lazy parse)

### Middleware toggles (runtime-backed, Wrela-facing)
- `server.with_request_id()`
- `server.with_logging()`
- `server.with_compression()` (gzip)
- `server.with_timeout(ms)`
- `server.with_max_body(bytes)`

## Routing Semantics
- Start with **exact match**.
- Add **param routing**: `/users/:id` (stored in `req.params`).
- Path matching rules:
  - No regex in v1.
  - Optional trailing slash normalize.

## Error Model
- `HttpHandler.handle` returns `HttpResponse` only.
- Unhandled errors become `500` with a simple body.
- If handler panics or returns invalid response, return `500` and log diagnostics.

## Example Usage (Wrela)
```wrela
A HelloHandler:
    can handle(req: HttpRequest) -> HttpResponse:
        return HttpResponse.ok("hello")

A UserHandler:
    can handle(req: HttpRequest) -> HttpResponse:
        id = req.param("id")
        return HttpResponse.json({"id": id})

to run() -> Nothing:
    server = HttpServer()
    server.serve_get_requests("/", detach HelloHandler() * 1)
    server.serve_get_requests("/users/:id", detach Pool.of(UserHandler, size=8, backpressure=queue(256)) * 1)
    server.serve_on("0.0.0.0:8080")
```

## Implementation Steps
1) **Runtime module**: `crates/runtime/src/http/` with router + server glue.
2) **Route registry** keyed by `(method, path)`; add param parsing later.
3) **Wrela stdlib classes**: `HttpServer`, `HttpHandler`, `HttpRequest`, `HttpResponse`, `Bytes` helpers.
4) **Bridge functions** to call Wrela handlers from Rust using actor send/await.
5) **Public server lifecycle** using `runtime_block_on`.
6) **Docs + examples** emphasizing handler pools for concurrency.

## Open Questions
- Do we want `HttpServer` itself to be a detached actor, or a plain class?
- How should `Bytes` be represented in Wrela (String vs byte array)?
- Should `serve_on` return a Result for bind failures?
- Do we require explicit `stop()` or rely on process shutdown?
