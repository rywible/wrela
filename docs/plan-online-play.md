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
  serves them; they still carry the site's cookies and skip the browser's cache, as every
  `std::http::fetch` does in toolchain 0.17.0 (docs/toolchain-gaps.md). Posting is the player's
  explicit choice, after the console says exactly what becomes public, and a setting posts every
  new personal best.
- **Fast.** Watching a board's run starts with nothing to wait for; a posted run shows its place
  on the results.
- **The rest of the platform reads and works as well as its games**: sign-in and sign-up as a
  player expects them, a pause menu that's quick to use, signs a moving player can read, the
  game itself behind the console's home, and plain, warm copy.

Not in this plan: friends, follows or lists (Ryan: "i don't want to build friends functionality
yet"); verification in parallel and Durable Object sharding (design.md, "Fast under load": before
a launch's crowd, as toolchain changes); live runs (design.md's anti-cheat layer 2).

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

(Filled in as the reviews come.)
