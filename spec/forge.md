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
  A call that makes more than one event writes them as one record of kind 15, so a crash keeps
  all of them or none; a record is at most 64 MiB, and a call whose record would be larger is
  refused.

| Kind | Event | Fields |
|---|---|---|
| 1 | A change opened | change, intent, author, model, session, machine, time |
| 2 | A change wrote a file | change, path, file reference (none: removed), time |
| 3 | A change named its tree (a snapshot) | change, note, tree, time |
| 4 | A change's file was carried onto main (the first layout: it conflicts if conflict names a file) | change, path, base, now, conflict (file references) |
| 5 | A change asked to land | change, time |
| 6 | A change closed | change, why, time |
| 7 | Main landed a revision | place on main, change (0 if imported), revision, tree, time, by, intent, count of files, then each path and file reference |
| 8 | A check was queued | job, check, tree, change (0 for main's), try, time |
| 9 | A check started | job, worker, time |
| 10 | A check finished | job, evidence, passed, time |
| 11 | A check was waived | check, why |
| 12 | A service was promoted | epoch, machine, time |
| 13 | The service was fenced | epoch, time |
| 14 | A check went back to the queue | job, why, time |
| 15 | A call's events, together | the count of events (2 or more), then each event's bytes as a run |
| 16 | A change's file was carried onto main | change, path, base, now, conflict (file references), whether it conflicts (a bool) |

- **F14.** The state is the fold of the log's events from the start, in order. An event that
  doesn't apply (a change that isn't open, a landing out of place) means the log isn't the
  state's: a start stops and says which. An event that doesn't apply changes nothing, and a
  call's events apply all together or not at all.
- **F15.** A kind not in F13 is an error. A new layout of an event is a new kind.

## Changes

- **F16.** A change is an overlay on main: for each file it touched, main's version when it
  touched it (its base), its own version, and, after a carry that conflicts, main's newer version.
  It sees its own version of each file it touched and main's newest version of every other file.
- **F17.** When main lands a new version of a file a change touched, forge carries the change onto
  it: a three-way merge of the lines of the base, the change's version and main's. A clean merge
  becomes the change's version, with main's as its base; a conflict keeps the change's version
  and main's beside it (or that main removed the file). A write of the file resolves the conflict,
  with main's version (or its absence) as its base.
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
  - `GET /api/log?from=X&chain=C&epoch=E&replica=NAME`: the log from byte X (at most 8 MiB of
    it), for a replica (F25); held up to 20 s while the log holds no more.
  - `GET /api/position` and `POST /api/fence`: a store's role, epoch and log (F26), and a newer
    epoch, told (F26a).
  - `POST /api/objects`: for each 32-byte id of the body, the id, its file's length (a `u32`
    little-endian; 4294967295 if the service hasn't it) and its file's bytes.
  - `POST /api/work/next` and `POST /api/work/done`: a worker's asks (F23).
  - `POST /api/stop`: stops the service, for a caller that may land changes.
  - `GET /api/watch?room=R&from=X` and `GET /studio`: rooms (F33-F35).
  - `GET /`: the digest, a page of what forge holds; `GET /health`.
- **F22.** What a caller may do is decided by who it is (F28): every caller forge knows may read;
  writing (opening, editing, checking, submitting and closing changes, and storing blobs) is for
  every caller forge knows unless `--writers` is given, then only for those it names; landing,
  waiving, importing, fencing and stopping are for `--landers`; working (F23) and replicating
  (F24) are for `--fleet`. The machine itself may do everything. Each of those flags is a list,
  separated by commas, of `node:NAME`, `user:LOGIN`, `tag:TAG` and addresses.

## Workers

- **F23.** A worker is a checkout (the service's `work` root) and the name of its machine. For each
  check it makes the checkout hold the check's tree, writing only the files that differ from what
  it held, and runs `tools/check.sh` (with the check's name, unless the check is `suite`) there.
  A check named `index:<package>` runs that tree's own `tools/wrela index --json <package>`
  instead: its output is the package's item index at that tree, which the `items`, `changed` and
  `callers` tools read.
  `workers/<name>` is what its checkout holds: the byte 1, the count of its files, then each path,
  whether it is executable, its blob's id and its size.
- **F23a.** A job runs on one worker at a time, as one try (its `attempt`). A worker's report
  counts only for the job's current try on that worker, while it runs. A job goes back to the
  queue (event 14), as a new try, when its worker starts again without it (the service's own
  worker at a start, a worker on another machine when it next asks for work), and when it has
  run 10 minutes past its limit.
- **F23b.** A check queued on a tree where the same check already passed isn't run again: the
  queuing answers with the job that passed (`reused`). A failure is always run again.

## Replicas

- **F24.** A replica is a store that follows a service: it asks for the log from where its own
  ends, fetches every object the new events name (a tree's, a revision's and evidence's objects
  too), writes each flushed, and then appends the events, as they came, to its own log, flushed. So
  its log is a prefix of the service's, and every object it names is on its disk. A replica's
  start cuts a torn record off its log, as a service's does (F12).
- **F25.** A log's chain at a record's end is BLAKE3 of the chain at the record before it (32 zero
  bytes before the first record) and the record's 8-byte hash: so equal chains at equal ends mean
  equal logs, as far as their hashes tell. A replica asks for the log from byte X with its chain
  there and its epoch (F26); the service answers only if a record of its own log ends at X with
  that chain, else 409, so a replica never follows a log that isn't its own's continuation. The
  ask says the replica holds the first X bytes. A service with `--acks N` sends every answer that
  says something happened only once N replicas hold the log through it.
- **F26.** A store's epoch is the epoch of the last `promoted` event in its log (0 if none), and
  is the machine's that event names. A store's first service appends `promoted` with epoch 1; a
  service serves a store whose epoch is another machine's only to promote it.
  `forge serve --promote` makes a replica's data the service: it asks each of `--peers` for its
  position (`GET /api/position`: role, epoch, log size and chain); asks each to promise an epoch
  above every one it saw (`POST /api/fence`), after which a replica follows no service of an older
  epoch and drops the records it fetched and hasn't kept, and an older service is fenced; asks for
  their positions again; and, if more than half of the stores (the peers that promised, and
  itself) answered and no peer's log isn't a prefix of its own, appends `promoted` with that epoch
  and serves. Else it refuses, and the peers that promised wait for a promotion that succeeds.
  With `--acks N` at least half of the replicas, a landing a service confirmed is on every
  majority, so a promotion can't lose it.
- **F26a.** A service is fenced when it learns of an epoch above its own: from a replica's ask, a
  peer's position, or `POST /api/fence`. A fenced service answers every call after with 503,
  confirms nothing more, appends `fenced` and stops; a store whose log holds `fenced` doesn't serve
  again, nor become a replica. A service started with
  `--peers` asks each for its position at its start and every 10 s, so an old service that comes
  back is fenced before it takes a call.
- **F26b.** A replica follows the service among `--servers` that is serving with the highest
  epoch, and none older than an epoch it promised. It looks again when its service fails, answers
  409 or 503, or is fenced, and when it promises a newer epoch (`POST /api/fence` on the replica's
  own `--listen`, where it also answers `GET /api/position` with the higher of its log's epoch and
  the one it promised).

## Workspaces

- **F27.** A workspace is a directory that holds a change's files, with what it holds in
  `.forge/state`: the byte 1, the change's name, and a run holding F23's list of files. A pull
  writes the files that differ from what it held and removes the ones the change no longer holds. A
  push sends every file that differs from what it held, and every new file that `.forgeignore`
  doesn't name, into the change, and removes from the change the files that are gone.
  `.forgeignore` is written as `.gitignore` is: parts between `/`, `*` within a part, `**` for any
  parts; a rule with a `/` before its end is anchored at the root, and one that ends in `/` names
  directories.

## Callers

- **F28.** A caller is known by its connection's peer address. An address of the machine itself
  (`127.0.0.1`, `::1`) is the machine. An address of the tailnet (`100.64.0.0/10`,
  `fd7a:115c:a1e0::/48`) is known by asking tailscale whose it is (`tailscale whois --json ADDR`,
  or the program `--whois` names, with the same answer): its node is the first label of
  `Node.Name`, its login `UserProfile.LoginName`, and its tags `Node.Tags`. forge keeps an answer
  for 60 s. Any other caller, and a tailnet caller tailscale can't name, is refused with 403.
- **F29.** A call's author and a landing's `by` are the caller's login and node (`LOGIN@NODE`), or
  `local` for the machine; an author the call gives is kept beside it, not in its place.

## Backups

- **F30.** `forge backup --to URL --auth FILE` copies a store (its `data` root, which a service or
  a replica may be using) to an S3 bucket (Cloudflare R2), and keeps copying what is added. It
  sends each request with curl, signed as S3 asks (`--aws-sigv4 aws:amz:auto:s3`), with the
  credentials curl reads from FILE (`user = "KEY:SECRET"`), so forge never holds them.
- **F31.** A backup is segments. Segment X holds the log's whole records from byte X
  (`log/<X as 16 hex digits>`) and every object those records name that no earlier segment holds
  (`pack/<X as 16 hex digits>`: for each, its id, its file's length as a `u32` little-endian, and
  its file's bytes). `head` holds the byte 1, the end of the last segment, and the log's chain
  there (F25). A segment's pack is sent before its log, and both before `head` names it.
- **F32.** `forge restore --from URL --auth FILE` makes an empty `data` root a store again: it reads
  `head`, then each segment in turn, checks each object against its id, writes the objects and
  then the records, flushed, and checks that the log's chain at its end is `head`'s.

## Rooms

- **F33.** A room is a view of the log that callers watch: `main` (landings and main's checks),
  `cN` (change N: its writes, carries, conflicts, checks and landing, and main moving past its
  files), and `checks` (every job). A notice is an event's meaning in a room, at the event's
  record's end in the log.
- **F34.** `GET /api/watch?room=R&from=X` answers with R's notices from X on (at most 8 MiB of log),
  the end they reach (`next`), and who is in the room. It is held up to 20 s while there are
  none. A caller is in a room while it watches it and for 30 s after.
- **F35.** `GET /studio` is a page of who works on what: the rooms and who is in them, the open
  changes, the jobs and the newest landings and notices. `GET /studio?after=X` is held, as a
  watch of every room is, until the log passes X, and the page asks for itself again after each
  answer, so it shows what happens as it happens.
