# Runtime Configuration (v0)

Environment variables:
- `WRELA_MAILBOX_CAP`: per-actor mailbox capacity (default: 256)
- `WRELA_ENQUEUE_TIMEOUT_MS`: max time to wait when enqueueing actor messages (default: 10)
- `WRELA_BATCH_LIMIT`: max messages processed per actor tick before yielding (default: 64)

Notes:
- Enqueue timeouts only apply to sending into a full mailbox. Actor method execution is not
  limited by this timeout.
- `await` waits for enqueue up to the timeout; if it cannot enqueue, the pending resolves
  to `nil`.
- `fire` waits for enqueue up to the timeout; if it cannot enqueue, the message is dropped.
