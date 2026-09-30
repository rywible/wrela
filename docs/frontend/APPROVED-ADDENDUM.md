# Approved Identifier Policy Addendum

Ryan approved this policy on September 30, 2026 at 18:35 UTC in `Sentinel_b88ecc551e6081918d23dd91c7dfd6e7`, relayed by Farquad to this task. It resolves the invisible-character policy left open in the original approved specification.

Use Unicode 16.0.0 XID_Start/XID_Continue plus underscore. Identifiers exclude Default_Ignorable_Code_Point, including join controls. Preserve offending source bytes and diagnose them; do not strip or normalize identifiers. Strings and comments may contain these characters.

The original SPEC.md remains immutable at SHA-256 `6e2115727eb2ee55ec2d99a4401c806c7ed2597111499e51677e311587439531`. Read it together with this approved addendum. Its eight acceptance criteria and feature boundary remain authoritative; detailed design proposals do not add acceptance scope. Farquad owns final acceptance, merge, post-merge verification and closure.

Corpus V11 includes invisible characters in a string/comment, while I13 and I14 require rejection with preserved bytes for U+200C and U+200D in identifiers. These are planned corpus expectations, not executed production results.
