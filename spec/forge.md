# forge's store and service

forge is wrela's version control and check service, written in Wrela (`forge/`) and run by the
service host (`runtime/service`, `wrela-service`). This file gives its durable formats and what it
promises, so its data can be read, checked and recovered without it. These rules are normative:
`forge/` implements exactly them. Each rule has a number (F1, F2, …) that tests cite; a number is
never reused.

## The store

- **F1.** A store is a directory (the service's `data` root) holding `objects/`, `events` and
  `workers/`. Nothing else in it is forge's.
- **F2.** An object is a file under `objects/`, named by its id: the first two hex digits of the id
  are a directory, the other 62 the file's name (`objects/ab/cdef…`). An object's file never
  changes once written.
- **F3.** An object's bytes are its kind's byte, then its payload. Its id is the BLAKE3 hash
  (default mode, 32 bytes) of those bytes, written as 64 lowercase hex digits, its bytes in order.
  A reader checks every object it reads against its id.
- **F4.** The kinds are 1 (a blob), 2 (a tree), 3 (a revision) and 4 (evidence). A blob's payload
  is a file's bytes.

## Encoding

- **F5.** A whole number is an unsigned LEB128 varint: 7 bits a byte, low bits first, the high bit
  set on each byte but the last; at most 64 bits. A run of bytes, or of UTF-8 text, is its length
  as a number, then its bytes. An id is its 32 bytes. A bool is one byte, 0 or 1.
- **F6.** A file reference is one byte, 0 (no file), 1 (a file) or 2 (an executable file); for 1
  and 2, the blob's id and the file's size as a number follow.

## Trees, revisions and evidence

- **F7.** A tree's payload is the byte 1 (its version), the count of its entries, then each entry:
  a byte for its kind (1 a file, 2 an executable file, 3 a directory), its name as text, the id of
  its blob or tree, and its size (a file's bytes; 0 for a directory). Entries are ordered by their
  names' bytes, and a name is in a tree once. A name is not empty, `.` or `..`, and holds no `/`.
- **F8.** The files a tree holds are its files, and its directories' files under their names and a
  `/`. A store's trees hold files, executable files and directories; there are no links.
- **F9.** A revision's payload is the byte 1, then: its tree's id; the id of the revision before
  it on main (32 zero bytes for the first); its place on main (1 for the first); the change that
  landed it (`c` and its number, or empty); its author; its intent; the time it landed
  (milliseconds since 1970); the agent's model and session and the machine (each empty if not
  known); where it came from (`git <commit>` for an imported commit, else empty); and the count of
  its evidence, then each evidence object's id.
- **F10.** Evidence's payload is the byte 1, then: the check (`suite`, or one `tools/check.sh`
  name); the id of the tree it checked; the machine; the exit status as the number of its 32 bits
  (two's complement); whether it ran past its limit; how long it ran in milliseconds; the id of
  the blob that holds its output; and when it ended. It passed when its status is 0 and it didn't
  run past its limit.

## The log

- **F11.** `events` is the log: records appended one after another. A record is its payload's
  length (4 bytes, little-endian), the payload (at most 64 MiB), and the first 8 bytes of the
  payload's BLAKE3 hash.
- **F12.** A reader reads records from the start and stops at the first that isn't whole or whose
  hash differs. The bytes from there on are a torn record: a start cuts the file back to the
  records before it.
- **F13.** Each record's payload is one event: a byte for its kind, then its fields, in this order.

| Kind | Event | Fields |
|---|---|---|
| 1 | A change opened | change, intent, author, model, session, machine, time |
| 2 | A change wrote a file | change, path, file reference (none: removed), time |
| 3 | A change named its tree (a snapshot) | change, note, tree, time |
| 4 | A change's file was carried onto main | change, path, base, now, conflict (file references) |
| 5 | A change asked to land | change, time |
| 6 | A change closed | change, why, time |
| 7 | Main landed a revision | place on main, change (0 if imported), revision, tree, time, by, intent, count of files, then each path and file reference |
| 8 | A check was queued | job, check, tree, change (0 for main's), try, time |
| 9 | A check started | job, worker, time |
| 10 | A check finished | job, evidence, passed, time |
| 11 | A check was waived | check, why |

- **F14.** The state is the fold of the log's events from the start, in order. An event that
  doesn't apply (a change that isn't open, a landing out of place) means the log isn't the
  state's: a start stops and says which.
- **F15.** A kind not in F13 is an error. A new layout of an event is a new kind.

## Changes

- **F16.** A change is an overlay on main: for each file it touched, main's version when it
  touched it (its base), its own version, and, after a carry that conflicts, main's newer version.
  It sees its own version of each file it touched and main's newest version of every other file.
- **F17.** When main lands a new version of a file a change touched, forge carries the change onto
  it: a three-way merge of the lines of the base, the change's version and main's. A clean merge
  becomes the change's version, with main's as its base; a conflict keeps the change's version
  and main's beside it. A write of the file resolves the conflict, with main's version as its base.
- **F18.** A change lands when it touched a file, main hasn't moved past any of its files, none of
  them conflicts, and the suite passed on its exact tree (or the suite was waived). Landing makes
  main's files its files, as a new revision whose evidence is the suite's.

## The service

- **F19.** Every answer that says something happened is sent after it is durable: its objects
  written and flushed, then its events appended to the log and flushed. A write that fails stops
  the service.
- **F20.** Objects are written before the events that refer to them; a commit's events are
  appended in the order the commits were made.
- **F21.** The service serves HTTP/1.1 (RFC 9112) with bodies by `content-length`. Agents and
  people call it through forge's command, `forge <tool> key=value...`, whose `forge help` lists
  each tool (`forge/core.wrela`'s `TOOLS`):
  - `POST /api/call`: `{"tool": name, "args": {...}}`, answered with the tool's JSON (422 if the
    tool refused).
  - `GET /api/object/<id>`: a blob's bytes, or another object's.
  - `POST /api/object`: the body as a blob; answered with its id.
  - `POST /api/import`: `{"git": dir, "ref": name}`, into an empty store.
  - `GET /api/log?from=X&replica=NAME`: the log from byte X (at most 8 MiB of it), for a replica;
    held up to 20 s while the log holds no more.
  - `POST /api/objects`: for each 32-byte id of the body, the id, its file's length (a `u32`
    little-endian; 4294967295 if the service hasn't it) and its file's bytes.
  - `POST /api/work/next` and `POST /api/work/done`: a worker's asks (F23).
  - `POST /api/stop`: stops the service, for a caller that may land changes.
  - `GET /`: the digest, a page of what forge holds; `GET /health`.
- **F22.** A caller lands changes only if forge lets it: a caller from the machine itself, or one
  `--landers` names.

## Workers

- **F23.** A worker is a checkout (the service's `work` root) and the name of its machine. For each
  check it makes the checkout hold the check's tree, writing only the files that differ from what
  it held, and runs `tools/check.sh` (with the check's name, unless the check is `suite`) there.
  `workers/<name>` is what its checkout holds: the byte 1, the count of its files, then each path,
  whether it is executable, its blob's id and its size.

## Replicas

- **F24.** A replica is a store that follows a service: it asks for the log from where its own
  ends, fetches every object the new events name (a tree's, a revision's and evidence's objects
  too), writes each flushed, and then appends the events, as they came, to its own log, flushed. So
  its log is a prefix of the service's, and every object it names is on its disk.
- **F25.** Asking for the log from byte X says the replica holds the first X bytes. A service with
  `--acks N` answers a call that lands a revision only once N replicas hold the log through it;
  other calls are answered as F19 says.
- **F26.** A replica becomes the service by running `forge serve` on its data: its state is the
  fold of its log (F14). The old service must not take calls after that; nothing yet stops it but
  its operator.

## Workspaces

- **F27.** A workspace is a directory that holds a change's files, with what it holds in
  `.forge/state`: the byte 1, the change's name, and a run holding F23's list of files. A pull
  writes the files that differ from what it held and removes the ones the change no longer holds. A
  push sends every file that differs from what it held, and every new file that `.forgeignore`
  doesn't name, into the change, and removes from the change the files that are gone.
  `.forgeignore` is written as `.gitignore` is: parts between `/`, `*` within a part, `**` for any
  parts; a rule with a `/` before its end is anchored at the root, and one that ends in `/` names
  directories.
