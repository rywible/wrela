#!/bin/sh
# Checks a PR against the merge gate in CLAUDE.md and prints each condition. Read only: it never
# merges, approves, comments or resolves anything. When the gate passes it prints the merge
# command, bound to the head commit it checked: a push after the check makes GitHub refuse it.
#
# Usage: tools/merge-gate.sh <pr-number>
# Exit:  0 the gate passes, 1 it doesn't, 2 usage or GitHub API error.
#
# The gate: the PR is open, ready for review and targets main; the `ci` check passed on the head
# commit; CodeRabbit and Greptile each reviewed the head commit; every review thread is resolved;
# no `needs-owner` label.
#
# "Reviewed the head commit" needs evidence: the bots' own checks go green when they skip a PR or
# are rate-limited, and a bot's reply in a thread is filed as a review on the head commit.
#   CodeRabbit: its commit status (or check run) on the head says "Review completed", or it posted
#               a review of the head ("Actionable comments posted: N").
#   Greptile:   its summary comment's "Last reviewed commit" is the head, or its check run on the
#               head says "Greptile has reviewed the Pull Request".
# A review still running on the head fails its row whatever else is there: a re-triggered review
# of an unchanged head may yet post threads.
#
# CodeRabbit puts outside-the-diff and nitpick comments in the review's body, not in threads, so
# the gate can't tell whether they were handled; it prints their counts as a reminder (`note`).

set -u

usage() {
  echo "usage: tools/merge-gate.sh <pr-number>" >&2
  exit 2
}
die() {
  echo "merge-gate: $*" >&2
  exit 2
}

[ $# -eq 1 ] || usage
PR="$1"
case "$PR" in '' | *[!0-9]*) usage ;; esac
command -v gh >/dev/null 2>&1 || die "needs the GitHub CLI (gh)"

# Each query is captured on its own, never at the head of a pipeline, so a failed `gh` stops the
# gate instead of reading as "nothing found".
if [ -n "${GH_REPO:-}" ]; then
  REPO="$GH_REPO" # OWNER/REPO; `gh repo view` ignores GH_REPO, so it's read here
else
  REPO="$(gh repo view --json nameWithOwner --jq .nameWithOwner)" ||
    die "can't tell which repository this is (run it inside the clone, or set GH_REPO=owner/repo)"
fi

# One `key=value` per line; PR titles can't contain newlines.
INFO="$(gh pr view "$PR" --repo "$REPO" --json state,isDraft,baseRefName,headRefOid,labels,title --jq '
  "state=\(.state)", "draft=\(.isDraft)", "base=\(.baseRefName)", "head=\(.headRefOid)",
  "labels=\([.labels[].name] | join(","))", "title=\(.title)"')" ||
  die "can't read PR #$PR in $REPO"

STATE='' DRAFT='' BASE='' HEAD='' LABELS='' TITLE=''
while IFS= read -r line; do
  case "$line" in
    state=*) STATE="${line#state=}" ;;
    draft=*) DRAFT="${line#draft=}" ;;
    base=*) BASE="${line#base=}" ;;
    head=*) HEAD="${line#head=}" ;;
    labels=*) LABELS="${line#labels=}" ;;
    title=*) TITLE="${line#title=}" ;;
  esac
done <<EOF
$INFO
EOF

# The head SHA is spliced into a jq filter below, so it must be exactly 40 hex digits.
case "$HEAD" in *[!0-9a-f]*) die "unexpected head commit '$HEAD'" ;; esac
[ ${#HEAD} -eq 40 ] || die "unexpected head commit '$HEAD'"

FAILED=''
row() { printf '%-11s %-5s %s\n' "$1" "$2" "$3"; }
pass() { row "$1" ok "$2"; }
fail() {
  row "$1" FAIL "$2"
  FAILED="$FAILED $1"
}
count() { if [ -z "$1" ]; then echo 0; else printf '%s\n' "$1" | wc -l | tr -d ' '; fi; }
oneline() { printf '%s\n' "$1" | tr '\n' ' ' | sed 's/ *$//'; }

echo "PR #$PR in $REPO: $TITLE"
echo "head $HEAD"
echo

case "$STATE" in
  OPEN) pass state "open" ;;
  MERGED) fail state "already merged; the gate is for open PRs, so the rows below are for the record" ;;
  *) fail state "$(echo "$STATE" | tr '[:upper:]' '[:lower:]'); the gate is for open PRs" ;;
esac
if [ "$DRAFT" = true ]; then fail draft "still a draft (the bots skip drafts)"; else pass draft "ready for review"; fi
if [ "$BASE" = main ]; then pass base "targets main"; else fail base "targets '$BASE', not main"; fi

# ci: the latest run of the aggregate job on the head commit.
RUNS="repos/$REPO/commits/$HEAD/check-runs?per_page=100"
CI="$(gh api "$RUNS&check_name=ci" --jq '
  [.check_runs[] | select(.app.slug == "github-actions")] | sort_by(.started_at) | last
  | if . == null then "no ci check on the head commit"
    elif .status != "completed" then .status else .conclusion end')" || die "can't read check runs"
OTHER="$(gh api --paginate "$RUNS" --jq '
  .check_runs[] | select(.status == "completed" and (.conclusion | IN("success", "neutral", "skipped") | not))
  | "\(.name)=\(.conclusion)"')" || die "can't read check runs"
if [ "$CI" = success ]; then pass ci "success"; else fail ci "$CI"; fi
if [ -n "$OTHER" ]; then row "" "" "not passing on the head: $(oneline "$OTHER")"; fi

# CodeRabbit. The combined status holds the latest status per context, as `state description`.
CR_STATUS="$(gh api "repos/$REPO/commits/$HEAD/status" --jq '
  [.statuses[] | select(.context | test("^coderabbit"; "i")) | "\(.state) \(.description // "")"]
  | first // ""')" ||
  die "can't read commit statuses"
CR_CHECK="$(gh api --paginate "$RUNS" --jq '
  .check_runs[] | select(.app.slug == "coderabbitai") | "\(.output.title // "") \(.output.summary // "")"')" ||
  die "can't read check runs"
CR_RUNNING="$(gh api --paginate "$RUNS" --jq '
  .check_runs[] | select(.app.slug == "coderabbitai" and .status != "completed") | .name')" ||
  die "can't read check runs"
CR_REVIEWS="$(gh api --paginate "repos/$REPO/pulls/$PR/reviews?per_page=100" --jq "
  .[] | select(.commit_id == \"$HEAD\" and .user.login == \"coderabbitai[bot]\"
               and (.body | contains(\"Actionable comments posted\"))) | .id")" ||
  die "can't read reviews"
case "$CR_STATUS" in pending* | *"in progress"* | *"In progress"*) CR_RUNNING="${CR_RUNNING}status" ;; esac
CR_SEEN="status: ${CR_STATUS:-none}; reviews of the head: $(count "$CR_REVIEWS")"
if [ -n "$CR_RUNNING" ]; then
  fail coderabbit "a review is still running ($CR_SEEN)"
else
  case "$CR_STATUS $CR_CHECK" in
    *"Review completed"*) pass coderabbit "$CR_SEEN" ;;
    *) if [ -n "$CR_REVIEWS" ]; then pass coderabbit "$CR_SEEN"; else fail coderabbit "$CR_SEEN"; fi ;;
  esac
fi
# Reviews come back oldest first; the last one of the head is the current review.
CR_LAST="$(printf '%s\n' "$CR_REVIEWS" | tail -n 1)"
if [ -n "$CR_LAST" ]; then
  CR_BODY="$(gh api "repos/$REPO/pulls/$PR/reviews/$CR_LAST" --jq '.body')" ||
    die "can't read review $CR_LAST"
  CR_EXTRA="$(printf '%s\n' "$CR_BODY" | grep -oE '(Outside diff range|Nitpick) comments \([0-9]+\)')"
  if [ -n "$CR_EXTRA" ]; then
    row "" note "the review's body has $(printf '%s\n' "$CR_EXTRA" | paste -sd ',' - | sed 's/,/, /g'): not threads, so handle each and reply"
  fi
fi

# Greptile.
GR_LAST="$(gh api --paginate "repos/$REPO/issues/$PR/comments?per_page=100" --jq '
  .[] | select(.user.login == "greptile-apps[bot]" and (.body | contains("greptile_summary")))
  | .body | [scan("Last reviewed commit:[^\\n]*?/commit/([0-9a-f]{40})")] | last | .[0] // empty')" ||
  die "can't read PR comments"
GR_LAST="$(printf '%s\n' "$GR_LAST" | tail -n 1)"
GR_CHECK="$(gh api --paginate "$RUNS" --jq '
  .check_runs[] | select(.app.slug == "greptile-apps") | .output.summary // "" | split("\n")[0]')" ||
  die "can't read check runs"
GR_RUNNING="$(gh api --paginate "$RUNS" --jq '
  .check_runs[] | select(.app.slug == "greptile-apps" and .status != "completed") | .name')" ||
  die "can't read check runs"
GR_SEEN="last reviewed commit: $(printf '%.7s' "${GR_LAST:-none}"); check: $(oneline "${GR_CHECK:-none}")"
case "$GR_CHECK" in *"Greptile has reviewed"*) GR_CHECKED=yes ;; *) GR_CHECKED=no ;; esac
if [ -n "$GR_RUNNING" ]; then
  fail greptile "a review is still running ($GR_SEEN)"
elif [ "$GR_LAST" = "$HEAD" ] || [ "$GR_CHECKED" = yes ]; then
  pass greptile "$GR_SEEN"
else
  fail greptile "$GR_SEEN"
fi

# Review threads: all must be resolved, outdated ones included (the ruleset counts those too).
# shellcheck disable=SC2016 # the $s are GraphQL variables
THREADS="$(gh api graphql --paginate -f owner="${REPO%%/*}" -f name="${REPO#*/}" -F number="$PR" -f query='
  query($owner: String!, $name: String!, $number: Int!, $endCursor: String) {
    repository(owner: $owner, name: $name) {
      pullRequest(number: $number) {
        reviewThreads(first: 100, after: $endCursor) {
          pageInfo { hasNextPage endCursor }
          nodes { isResolved }
        }
      }
    }
  }' --jq '.data.repository.pullRequest.reviewThreads.nodes[] | select(.isResolved | not) | "unresolved"')" ||
  die "can't read review threads"
if [ -z "$THREADS" ]; then pass threads "all resolved"; else fail threads "$(count "$THREADS") unresolved"; fi

case ",$LABELS," in
  *,needs-owner,*) fail labels "$LABELS (needs-owner: the owner decides)" ;;
  *) pass labels "${LABELS:-none}" ;;
esac

echo
if [ -z "$FAILED" ]; then
  echo "gate: pass"
  echo "merge with: gh pr merge $PR --repo $REPO --squash --auto --match-head-commit $HEAD"
  exit 0
fi
echo "gate: FAIL ($(echo "$FAILED" | sed 's/^ //; s/ /, /g'))"
exit 1
