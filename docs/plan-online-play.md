# Online play and the platform's polish: plan

*What the console and the capybara do next, after the console moved to static assets and its own
API Worker (rywible/wrela#348, #349): the way onto the world's boards with no friction, every
board's run watchable at once and raceable as a ghost, an industry-standard sign-up that records
the player's agreement, a clearer pause menu, readable signs, the game playing behind the
console's home, and copy a person would write. Decided by Ryan on 2026-10-31; the design is
games/capybara/design.md's "Online play". Written for an implementing agent who has not read the
conversation that produced it. Each step names its files, what to write, how to test it and what
must hold before the next. Ryan asked for it as one pull request, reviewed before it merges by
`/simplify`, the council (the Codex CLI), an xhigh code review and a security review.*

## 1. Goals, and what this plan doesn't do

The leaderboard is the heart of the capybara (Ryan: "the heart of this particular game is the
leaderboard, and the friction to that is making an account"). So:

- **Offline stays offline.** Play never touches the network. An account is the explicit step that
  makes things online.
- **Reading is open; being on the boards takes an account and a choice.** Anyone may see the
  World's boards, watch their runs, race them as ghosts and search names. These public reads need
  no account, the server ties none to them (it reads no session for one), and the edge cache
  serves them; since toolchain 0.18.0 they go with `std::http::fetch_public`, with no cookie, and the
  browser's cache keeps them too (docs/toolchain-gaps.md). Posting is the player's
  explicit choice, after the console says exactly what becomes public, and a setting posts every
  new personal best.
- **Fast.** Watching a board's run starts with nothing to wait for; a posted run shows its place
  on the results.
- **The rest of the platform reads and works as well as its games**: sign-in and sign-up as a
  player expects them, a pause menu that's quick to use, signs a moving player can read, the
  game itself behind the console's home, and plain, warm copy.

Not in this plan: friends, follows or lists (Ryan: "i don't want to build friends functionality
yet"); live runs (design.md's anti-cheat layer 2). Verification at the edge and a sharded server,
first left out, came in on 2026-10-31 (steps 11–14).

## 2. The steps, in order

Each step lands as commits on one branch. A step's tests pass before the next starts. Every
user-facing string a step writes follows §3.

### Step 1. The capybara's star goes

`games/capybara/cast.wrela:768-782` draws a yuzu-yellow star over the capybara's back whenever
it's in the air with a double jump left, which is nearly every jump (Ryan: "i don't like the star
when you're mid jump. that's unncessary"). Remove it and its line in design.md ("On the capybara:
while a double jump is still to come…"). Keep the `SPARK` shape: dust.wrela's sparkles use it.
The double jump's confetti and flip stay. Test: the capybara's tests and `wrela check
games/capybara`; a still of a jump shows no star.

### Step 2. Signs a moving player can read

The signs stand on the plane of play (signs.wrela, cast.wrela:542-543), so they pass at the
camera's full speed; their words are small (the note line `0.2·m`, 11 px on a 720 px screen, at
72% ink) and drawn before the water's copy, which bends them. Ryan: "it's hard to read the
instructions as they go by".

- Larger words: the heading and the button glyphs about a third larger, the note line at the
  heading's ink, not 72%, and no smaller than 16 px on a 720 px screen (`signs.wrela:110-121`).
- The words drawn after the water's copy, so water never bends them (`main.wrela:549-631`).
- **The sign's prompt stays with the player:** while the capybara is within a sign's reach and
  hasn't learned its move, the sign's heading and buttons show as a card at the top of the screen,
  steady while the world moves, and fade a second after the sign leaves the screen. A learned move's
  card doesn't show. This is the fix that matters; the larger words help the rest.
- The first signs no closer than 8 m apart (`signs.wrela:164`'s test), so each is read alone.

Test: signs.wrela's tests (sizes, spacing, which sign's card shows at a position, learned moves
hide it); a still at 1280×720 with the card showing. Proposed until Ryan plays it.

### Step 3. One sign-in and sign-up, as players expect it

Today (console/account.wrela:1214-1310): a Choose page of four buttons ("Sign in with a passkey",
"Create an account", "Use a recovery code", "Not now"), then a passkey, knighting, and ten recovery
codes before anything else. No agreement to the terms is asked or recorded.

- **One page, two clear paths.** "Sign in" (a passkey, the primary button) and "Create account".
  Recovery is a small link under sign-in ("Lost your passkey?").
- **Create account asks for agreement first.** One required checkbox, unchecked: "I agree to the
  Terms and the Privacy Policy" (each a link that opens the page and comes back), and one more:
  "I'm 13 or older" (children's privacy, design.md's "Open"; a lawyer may change it). The button
  is disabled until both are ticked. Then the passkey, then knighting.
- **The server records the agreement**: the terms' and the privacy policy's versions and the
  time, with the account (platform/account.wrela's `Account`, written in `RegisterSecrets`,
  platform/server.wrela:1455). `Request::Begin` for `REGISTER` carries the versions the player
  agreed to; the server refuses a registration without the current ones. legal.wrela gets a version
  constant for each document.
- **Recovery codes after, not in the way** (design.md, proposed): sign-up ends at knighting. The
  account page shows "Save your recovery codes" until they're saved, and the first posted run's
  results remind once. The server still makes the codes at registration and returns them then; the
  console keeps them in memory until they're shown and saved (never in storage), and offers to show
  new ones (a fresh passkey sign-in, as today) once those are gone.
- Every page: a visible state while a request is under way (the button says what's happening), an
  error that says what to do, keyboard, controller and screen reader all work (the semantic tree,
  as today).

Test: platform/server_tests.wrela (registration refused without agreement; the agreement stored;
versions checked); console account tests (the button disabled until both boxes; codes shown
after). Proposed until Ryan sees it.

### Step 4. Posting, as an explicit choice

The game reaches the server only through the console (platform/link.wrela); an account's token
lets it submit runs (design.md).

- **The first post explains, once.** Before an account's first post, the console shows exactly
  what becomes public (design.md's list: the drawn name; the run's course, category and time; its
  replay; when it was posted) and what isn't (no address, device or email), with "Post" and "Not
  now". Agreeing is recorded with the account (a version and a time), as the terms are.
- **"Post my best runs"**, an account setting kept by the server, off until turned on (proposed),
  offered on that first screen and in the account's page. With it on, the game posts a run that
  beats the account's place on its board when the run ends, in the background; a run made offline
  waits in the game's storage (beside `capybara/uploaded`) and is posted when the console is next
  online.
- **Posting by hand** stays: a run on "Yours" can be posted with U / a controller's Y, behind the
  same first explanation.
- **The results show the place**: "Posting…", then the world place the run took, or why it didn't.

Test: server tests (the setting and the consent stored and read; a post before consent refused);
the capybara's online tests (a better run queued and posted; one not better isn't; offline runs
wait).

### Step 5. Every board's run, watched at once

- **Replays by their hash.** The server serves a verified run's replay at a path named by its
  SHA-256 (wire.wrela's public reads), immutable, cached for a year (today 5 minutes). A board's
  page lists each place's replay hash. A deleted account's replays go from the board; their bytes
  stay reachable by hash until the cache forgets them, which the privacy policy says.
- **Prefetched.** Opening a board fetches the replays of its visible rows and of the three places
  above the player's, in the background; choosing a row plays from its first frame
  (watch.wrela) with nothing to wait for.
- **A slice around a place**: a public read of the places around an account's (wire.wrela), for
  the ghosts and for the board's opening view.

Test: server tests (the hash path, its cache headers, a board page naming hashes, the slice);
the capybara's tests (a fetched replay plays; a board's rows prefetch).

### Step 6. Racing the runs above

- **Any board run as the ghost.** The ghost's option gains "The run above yours" (the default
  once there's an account with a place, proposed) and "The world record"; from a board's row,
  "Race" sets that run as the ghost and starts the level. The ghost machinery (replay.wrela's
  `trail`, cast.wrela's `ghost`) already runs any replay.
- **Where it gained.** After a run against a ghost, the results name the checkpoint where the
  most time went ("0.8 s behind at the slide"), from the splits the clock already keeps.

Test: the capybara's tests (a board replay as the ghost; the split summary picks the largest
loss).

### Step 7. Searching names

- **Server:** a public read of names by prefix, by part (title, noun, epithet), at most 20
  results, each with its places (wire.wrela, server.wrela; names are unique and indexed already).
- **Game:** a search on the World's board: the parts complete from platform/names.wrela's lists as
  they're typed; a result opens that account's places, each watchable and raceable.

Test: server tests (prefix search, limits, a deleted account gone); the capybara's tests (part
completion from the lists).

### Step 8. The pause menu

Today (screens.wrela:403-446, main.wrela:995-1012): six items (Resume, Restart, Levels, Runs,
Options, Controls) in a 40-unit card, a facts panel beside it, and Runs as four tabs ("Yours",
"Tool-assisted", "World", "Whole game"). Ryan: "the pause menu needs work".

- **The essentials first:** Resume, Restart, then the level's runs and the world's board as one
  "Leaderboard" (the World, with Yours as a filter, and the tool-assisted runs one more), then
  Levels, Options, Controls.
- **The attempt beside it, plainly:** the level, the time, the best, the snacks; the world place
  if there is one.
- **Fewer words, larger targets**: rows taller, the focus clear at a glance, every item reachable
  in two presses from the menu.
- The watch bar names the account and its place, with Race beside it (step 6).

Test: the capybara's menu tests (order, focus, two presses to each item; semantic tree); a still
of the menu at 1280×720. Proposed until Ryan plays it.

### Step 9. The game behind the console's home

Today the home shows the game's hero, a still (console/paint.wrela). docs/toolchain-gaps.md's "A
program can't show another program playing behind its own canvas" says why: the browser host puts
an opened program behind an opaque canvas and `host::show` hides the opener.

- **Toolchain** (rywible/wrela-compiler): `host::underlay(id, on)`: the browser host draws an
  opened program's surface behind the opener, the opener's canvas transparent where it draws
  nothing; muted, taking no input. A release, then this workspace pins it.
- **The game plays itself:** the catalog gains each game's attract address
  (studio/release/catalog.wrela: `capybara/#attract`); the capybara's `#attract` start plays a
  replay on loop, its music off: the route search's human line (routes.wrela's `HUMAN_ANY`), level
  after level, never the robot's.
- **The console's home** opens the attract program behind its hero, fades the hero out once the
  game says it's ready, and closes it when a game is played.

Test: the console's tests (the attract program opened, shown under, closed on play); the
capybara's (`#attract` plays the human line and loops). If the toolchain's release isn't out
when the rest is ready, this step waits for a pull request of its own.

As built (toolchain 0.17.0): the attract start is the play build opened at `capybara/#attract`
(the catalog's `capybara#attract`); the game starts neutral and decides once, when its address
answers, whether it plays itself (games/capybara/attract.wrela's `role_of`). Under 0.16.0 it was a
build of its own with `ATTRACT` set, since that host opened no program at an address with a
fragment (docs/toolchain-gaps.md, fixed by 0.17.0). The console hides the game behind a sheet or
the account's panel and shows it again, closing it only when the game changes, a game starts or
shows, or the host strains. The native host has no address to give, so the attract tool test
calls the export `play_itself` (docs/toolchain-gaps.md, "A driven build can't be given an
address"). The look is proposed until Ryan sees it.

### Step 10. Copy

Much of the site's copy reads as generated (Ryan: "it reads as ai slop writing"). The patterns
in the survey: colon splices ("A day at the zoo: nine habitats to run, swim and slide through.";
"Up and down in turn: the dolphin kick"), stilted constructions ("Reading saves", "This deletes
your passkeys' records… at once"), and stale names ("upload one from the Players board").

- docs/voice.md: the voice in a page: short, plain, warm, second person; how errors, buttons and
  empty states are worded; the patterns to avoid, each with a before and after.
- Every user-facing string in the console (main.wrela, account.wrela, knighting.wrela), the
  capybara (screens.wrela, online.wrela, the levels' names and signs), the release's catalog and
  card (studio/release/catalog.wrela, site.wrela), and the docs' front page (docsite) rewritten
  to it. The legal pages' substance is a lawyer's; their wording follows the voice.

Test: the strings' tests updated; a list of every string changed, before and after, in the pull
request's description.

### Steps 11–14. On Cloudflare: delivery and a sharded server

Folded in on 2026-10-31 (Ryan: "Fold into #351"), from a plan another session made and a council
(gpt-6.1-sol and gpt-6-astra, three rounds, through the Codex CLI) reviewed. Its rule from Ryan:
**no backwards compatibility**, since no accounts exist yet; the wire may change, and nothing is
migrated. They need **toolchain 0.18.0** (rywible/wrela-compiler): build payloads at content
addresses (`addresses.json`, `/b/`), the runtime scripts in one directory, named server instances
with a `route` in the Worker, edge instances, `call`/`reply`, `send` with an outbox (at least once,
an explicit `ack`), `me()`, `std::crypto::hmac`, a verifier artifact loaded with the Worker Loader
(`call_build`), and a public fetch with no cookies that the browser may cache. This PR pins 0.18.0
and merges only once its assets are published.

**Step 11. Unchanged files stay cached across deploys.** Payloads (WASM, shaders, their `.lines`,
streamed files) go to `/b/<sha256>.<ext>` and the runtime scripts to `/r/<sha256 of the set>/`,
each `public, max-age=31536000, immutable`; `releases/<id>/` keeps only a release's own files
(pages, `manifest.json`, `addresses.json`, `files.json`, `catalog.tsv`, `prefetch-<id>.txt`,
`release.txt`). Bootstrap preloads and the console's prefetch name the `/b/` and `/r/` URLs. A
Worker's assets are each kept release's own files plus the union of the blobs they name, each
uploaded once. `MAX_FILES` 100,000; `KEEP` back to 14 days and the docs keep releases again (both
cheap now: a kept release adds only its own few files). Checks: two releases differing only in the
docs give the console and the capybara no new `/b/` or `/r/` URL; a cold game load in Chrome
(rendering, helper threads, audio, streamed files) has no failed request.

**Step 12. Zstandard.** Ryan adds the zone's Compression Rule once (Zstandard, Brotli, Gzip);
docs/toolchain.md records it. The deploy checks the console's and capybara's WASM and runtime come
back `content-encoding: zstd`. Kept only if zstd's bytes are within 5% of Brotli's (curl's
`size_download`), recorded in docs/bench.md.

**Step 13. Early Hints.** Ryan turns on the zone's Early Hints once. `_headers` mirrors each
bootstrap page's preloads as `Link: <…>; rel=preload; as=fetch; crossorigin`. Checks: a 103 in
`curl -sv --http2`; the WASM's `initiatorType` is `early-hints` in Chrome; its request start before
and after, in docs/bench.md.

**Step 14. The sharded server** (it replaces the one-object engine; nothing is migrated). One
`WrelaServer` class, named instances:

| Instance | Holds | Written by |
| --- | --- | --- |
| `account/<handle>` | the account, passkeys, sessions, recovery-code hashes, game tokens, runs and replays, bests and whole-game sums, upload counts; and (from steps 3–4) the sign-up agreement, the posting consent and the best-runs setting, each its own record | itself |
| `names/<00..3f>` | name → owner and generation (uniqueness) | itself |
| `search/<noun's first letter>` | normalized name → handle, in name order, for search by prefix | each owning account (an upsert; removed on rename or delete) |
| `codes/<00..3f>` | recovery-code hash → handle | its accounts |
| `board/<game>/<board>/<course>/<category>` | every account's best; pages and the slice around a place are range reads | its accounts |
| `whole/<game>/<board>/<category>` | each account's whole-game sum | its accounts |

- **Routing** (`route`, in the Worker): the session cookie `__Host-session = <handle>.<secret>` and
  game tokens route to their account; sign-in by its passkey's user handle; `Register` by the handle
  its challenge named; recovery to `codes/<prefix>`, then the account; boards, slices and replays to
  their instance behind the Cache API; uploads to the edge. Each instance checks its own secrets.
- **Replays by their run**, not a hash index: `RunRef { handle, n }` names a run (Watch, the board's
  rows, the ghost), served at an immutable public path with the handle in it, cached for a year, so
  no hash index is needed. A replay's SHA-256 is still how the game checks the bytes it got.
- **Search**: the shard key is the noun's first letter, because the noun is what players remember;
  a search with a noun reads one shard and filters by title there, a title alone fans out over the
  shards (bounded), and an epithet alone is refused, as now.
- **Ceremonies**: a challenge is `nonce ‖ expiry ‖ HMAC-SHA256(key, ceremony ‖ ceremony-cookie
  token ‖ handle ‖ nonce ‖ expiry)` with a Worker secret (two, for rotation); `Register` finding its
  handle's instance already made is a replay, refused; a sign-in nonce is kept spent for its life.
- **Uploads at the edge**: `Admit` at the account (the token, the hourly limit, consent, the name
  kept, a duplicate's original outcome by board version); the edge plays it (the current board in
  its own module, an older one through `call_build` on its archived verifier in R2, else "board
  closed"); `Record` at the account commits the run and its best and `send`s upserts to the board
  and whole board; the edge `call`s the board for the place. A failed board call answers retryably,
  and the retry takes the duplicate path. The queue and its limits go.
- **Names**: `Claim` returns a generation; `Keep` and `Release` need the owner and generation; a
  name change sends upserts to the account's boards and search.
- **Delete and export** are the account's: delete fences new records, sends removals to its boards,
  whole boards, names, search and code shards, and clears its store only once each is acknowledged.
- **Limits**: per-client rate limits as now; recovery per code shard (30 a minute each).
- **Sign-ups stay closed on the live server** until Ryan says otherwise (rywible/wrela#352: "until
  his LLC and the legal pages are settled"). The sharded server keeps #352's switch: the console's
  server build closes sign-ups at start, `Register` and its `Begin` are refused with a plain reason,
  and the console's Create account shows that sign-ups are closed for now. Sign-in, recovery and
  everything else work. Reopening is one line, and only on Ryan's word.
- Tests: the platform's fake host made multi-instance with every assertion carried over, plus the
  council's list (a replayed challenge, upload versus delete, a delayed release after a new owner, a
  held removal, 1,001 ranked accounts with a top one deleted, a failed board call and its retry), and
  `wrangler dev` end to end with two browsers.

As built (2026-10-31, toolchain 0.18.0): platform/server.wrela is the core (the instances' kinds and
names, what they say to each other: `Inner` calls, `Back` replies, `Note` messages in a `Letter`
with the sender's time), edge.wrela the edge, accounts.wrela an account, shards.wrela the names,
codes, replays and search shards, ranking.wrela a board and a whole board; the console's server
(console/server/serve.wrela) carries out each instance's commands. Where it differs from the table
and the list above:

- **One more kind, `replays/<00..3f>`** (replay's hash → the account that posted it), so a copy of
  another account's posted replay is still refused, as the one-object server refused it; without it a
  world record's public replay could be posted by anyone. The edge asks it once a run plays, before
  the account records it. A deleted account's replays are let go, and never its own again, so an
  upload of its still under way when it was deleted never comes back.
- **A challenge is 32 bytes:** an 8-byte nonce, the expiry, and the first 16 bytes of the HMAC-SHA256
  tag (RFC 2104 §5), so it's a WebAuthn challenge's usual size and the W3C's test vectors still
  register and sign in. What it covers adds the agreement a sign-up began with, so a sign-up's finish
  carries the same (`Register { handle, challenge, agreed, .. }`); a sign-in's names no handle (its
  passkey does), and its nonce is kept spent in the account it signs in to. The keys are the API
  Worker's secrets `WRELA_CEREMONY_KEY` and `WRELA_CEREMONY_KEY_OLD`, given as request headers,
  since a program reads no Worker variable (docs/toolchain-gaps.md); docs/toolchain.md says how
  they're set and rotated.
- **A removal is acknowledged by a message back** (`Note::Removed`), since the outbox tells its sender
  nothing (docs/toolchain-gaps.md); a deletion answers `Done` once every removal is sent, sessions
  and passkeys gone at once, and its store clears when the last one comes back. A message to itself
  (`Note::Finish`) finishes a deletion an instance dropped part way.
- **A name change sends nothing to the boards or search:** a name is kept for good before a run can
  go up (a reroll is refused once it's kept), so no board or search holds a name that can change.
- **An older board's runs** go through `call_build` on the verifier `games/capybara/boards.wrela`'s
  `ARCHIVED` names (empty today: version 3 is the first with one kept); the upload names its board
  (`Upload { board }`, link.wrela's `Ask::Upload { board }`). The release writes each build's verifier
  path to release.json, from the catalog's `console.boards` and `console.board` (a test checks it is
  replay.wrela's `BOARD`), and the deploy puts its files in the `wrela-builds` bucket with Wrangler
  before any Worker deploys.
- **The game's side** is the least it needed to compile against the new link (link.wrela): a place's
  run is a `RunRef`, `Ask::Watch { run }` reads a replay (`Ask::Replay` is gone; the game finds a
  hash's run among the places it shows), `Answer::Verified` and `Answer::Run` carry a `RunRef`, and
  `Ask::Upload` carries its board. A separate pass finished the game's side (§5, "The game's and
  the console's side").

Checked: `wrela test platform` (58 tests: every assertion of the one-object server carried over where
it still applies, the queue's gone with it; and the council's list, each broken in the code it guards
and seen to fail first), `wrela test console`, `console/server`, `studio/release`, `studio/deploy`
and the capybara's online, search and boards tests. End to end on `wrangler@4.151.0 dev` with curl
and a throwaway client (2026-10-31): the shipped build routes every kind of request to its instance
and refuses sign-ups; a copy with sign-ups opened signed up two accounts, posted the Wetlands' route
run (played at the edge: 1,311 ticks, place 1), read its board, slice, searches and its replay at its
path (`public, max-age=31536000, immutable`, then from the edge's cache), refused the second
account's copy, and after a deletion moved the second account up, forgot the run (past the cache) and
left the account's object holding its mark alone with its outbox empty; and a build of board 4
played a board-3 run, and refused a bad one, through the board-3 build's verifier in the local
bucket through the Worker Loader (the council's remaining check N6). Passkey sign-in itself, and
two browsers' sessions, still want a person.

## 3. How a string is written

docs/voice.md (step 10) holds the voice; until it's written, a step's strings follow these: short;
a person's words, not a document's; second person; a button says what it does ("Post run", not
"Submit"); an error says what happened and what to do; no colon to join two clauses; no
"seamless", "effortless", "at once" or "journey".

## 4. Before it merges

1. `wrela suite` passes, on the pinned toolchain.
2. `/simplify` on the branch, and its fixes.
3. The council: the Codex CLI reviews the pull request's diff against this plan; its findings
   are answered in §5.
4. An xhigh code review, and its fixes.
5. A security review: the new public reads (replays by hash, slices, name search), the consent and
   agreement records, the registration's checks, rate limits.
6. The deploy passes; the live site checked: a board's run plays, sign-up asks for agreement.

## 5. The reviews' findings

**`/simplify`** (2026-10-31; four reviewers: reuse, simplification, efficiency, altitude): about
50 findings, deduplicated and applied, among them a race on duplicate replays across accounts,
re-posts answered with their first outcome rather than matched by wording, posting only once a name
is kept, the server's search and slice round trips, per-frame allocation in the game, one offline
gate, a least-recently-used replay cache with the ghost pinned, and the attract start moved onto the
play build at `capybara/#attract` (toolchain 0.17.0). Skipped: consent folded into the account record
(kept apart, so each write touches only its own).

**The council, round 1** (2026-10-31, through the Codex CLI; verdict "don't merge"): 16 findings.
Fixed in the platform and console: the first-post explanation opening on a hidden console (the game
now shows it, from shared words in `platform/posting.wrela`); account enumeration through the slice
(one answer for any absence); an explicit "no place" answer; retryable failures told apart from
rejections; a run marked verified only once its ranking committed; the consent and the setting as
separate records; outcomes keyed by board version; the account pages' semantic tree; a failed
settings load no longer read as an opt-out. Moved to toolchain 0.18.0: public reads without cookies
and with the browser's cache. Not done: a backfill of the search index (there is no data, per
Ryan). Still to do, in the game: the posting screen, keeping and retrying waiting runs, both
categories' bests, every visible row prefetched, the leaderboard opening at the player's place,
racing past place 1,000, and a replay that fails to arrive. Steps 11–14 then rebuild the server,
and the review battery runs again over all of it.

**The game's and the console's side** (2026-10-31, after steps 11–14; the council's findings
against them, each fixed where its tests reach it and the test seen to fail with the fix broken):

1. *The first-post explanation and the recovery-codes reminder must show.* The console can't come
   over the game it shows in toolchain 0.18.0 (docs/toolchain-gaps.md, "A program can't come back
   over the program it shows"), so the console's dialog over the game is not built; recorded as a
   gap. Fixed in the game instead, as design.md had it: the screen was held (`consenting`) but never
   drawn. Now games/capybara/screens.wrela's `posting_card` draws posting.wrela's words over the
   leaderboard, keeps the run, and Post (recorded through the console, `Ask::Consent`) or Not now
   (or leaving the leaderboard) goes back to it; after a post, while the codes wait, the goal's
   card and the leaderboard's foot say `SAVE_CODES` until the next attempt.
2. *Public reads through `fetch_public`.* Already done (console/account.wrela's `request`: boards,
   slices, searches by `GET` of `PUBLIC`, replays at `run_path`); account requests keep `fetch`.
3. *A newcomer's first post, and both categories' places.* `Unplaced` was already "no place yet".
   Fixed: a best run asks the slice round the account's place on each board it counts for, any% and
   100%, and goes only once both are known and it beats one; the account's bests then come from
   the server's played time in `Verified`.
4. *Transient failures keep waiting runs.* Fixed: a post by hand waits in storage like a best run;
   `Failed` waits twice as long each time (30 s to 30 min), its count stored with the run, offline
   and signed-out answers wait 30 s uncounted, and only `Rejected` drops it. The console already
   answered 429, 5xx and network failures as `Failed`. The online stores now reuse main.wrela's
   `Storing`, so a load that failed is made again and never stored over.
5. *A failed settings or consent fetch isn't an opt-out.* The console already kept it unknown and
   asked again (`Known`, `posting_known`). Fixed in the game: nothing is posted, and no screen
   shown, while `posting_known` is false.
6. *Every row shown prefetched.* Fixed: all eight rows (seven under the search's line) of the
   window shown, and every search result shown; `HELD` 40, so both categories of a board and of the
   search, the places above, the ghost's and a race's stay held; a test ties the rows shown
   (`window`, `shown_rows`) to the rows fetched.
7. *The leaderboard opens at the player's place.* Fixed: a world's board opens on the slice round
   the account's place (or its page, if it's on it), the three places above over it and the focus
   on it; up and down move past the rows shown, the rows from the top asking for the next page
   (`more`); Home or a trigger goes between the top and the player's place.
8. *The run above past place 1,000.* Fixed: the ghost's run above, the account's place and the
   places above it fetched ahead are found by position in the page or the slice, not by number.
9. *Replay selection's states.* Fixed: "Loading the run…" while a chosen replay comes, "Can't load
   that run. Check your connection and try again." when it doesn't (and it isn't fetched ahead again
   until chosen); choosing another, another board or leaving gives it up, and a race whose run can't
   come is dropped.
10. *Accessibility of the agreement and the posting screen.* The 0.18.0 host has a semantic tree
   (`std::semantics`, `host::publish`). The console's Create account already published its boxes,
   links, dialog and buttons with their state and took their presses (`console/account.wrela`'s
   tests). The game's posting screen now does too (`posting_tree`: a modal dialog, its words as
   text, the box a checkbox with `checked`, Post and Not now buttons with the focus, their presses
   and the dialog's dismissal handled).

Played (2026-10-31, `wrela run games/capybara` in the Claude app's browser pane, the screen forced
up by a local change not committed, since a game opened alone has no console or account): the
screen draws over the leaderboard as designed; the dialog takes the keyboard (Tab, Shift+Tab,
Space, Enter, Esc), the drawn focus follows, the box's state reads back as `aria-checked`, Not now
and Esc return to the leaderboard and Post closes it, the keys going back to the game after. It
found a trap in the sim, fixed: the host's focus-out (an `Assist::Focus` on node 0) underflowed
world.wrela's `item_of`. Not played: the whole loop in the console against `wrangler dev`, which
needs a passkey sign-up (a person) and a release's layout of the console with its games.

Kept: offline play makes no request (the asks still need a console, and the background ones an
account); nothing is posted before the account agrees, by the screen's Post or the console's page;
sign-ups stay closed on the live server (console/server/serve.wrela's `close_sign_ups`).
