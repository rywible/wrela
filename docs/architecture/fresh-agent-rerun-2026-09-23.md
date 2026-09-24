# Fresh-agent rerun after workflow fixes

Two new agents ran sequentially with no inherited conversation. Both completed inside the original ten-minute budget. The revision improved substantially; the weathering handoff completed where the previous attempt timed out. This is promising evidence for the workflow, not proof of AAA quality or superiority to another engine.

| Task | Previous pilot | Fresh rerun | Independent checks | Authoring CLI calls |
| --- | --- | --- | --- | --- |
| Widen gate to 2.55 m | 9m 13.704s, completed | 1m 24.767s, completed | 16/16 passed | 3, zero failures |
| Fresh-agent weathering continuation | Timeout at 10m 0.120s | 5m 13.063s, completed | 16/16 passed | 7, one rejected request |

The observed revision time is **6.53× faster**, an 84.7% reduction. No exact speed ratio is assigned to the handoff because its earlier run timed out. Neither agent edited the frozen engine. Both exported editable source, images and portable work through atomic benchmark submission. Work-level review included one extra intent constraint for revision (17 total) and six extra preservation constraints for weathering (22 total); the independent runner checked the registered sixteen in each case.

## Comparison conditions

- Same seed, `104729`, and byte-identical starting gate definition. Same 600-second wall-clock limit and declared 16,000-token budget.
- Updated engine fingerprint: `415704301745e998f20cdde0c9bfcf6b6533a04eb0d4935ef478ddeceb5906a3`. The new workflow supplies task context, bounded semantic intents, batched image review and automatic handoff. Other integrated architecture/rendering changes mean this does not isolate one individual optimization.
- The current sixteen registered checks include the original six plus explicit material ownership, profile, rotation and socket preservation. The earlier pilot is not retroactively credited with these additional checks.
- Timer includes host startup/orchestration and terminal runner validation. Parent supplied task paths and operating rules, then gave neither author implementation hints or feedback while its timer ran. Weathering saw predecessor files and evidence, not predecessor conversation.
- Exact inherited model version, token usage and instrumented intervention counts were unavailable. The token cap is declared, not verified. Single trials do not establish a stable latency distribution, same-model causal speedup or cross-engine ranking.

The [comparison data](../../output/fresh-agent-rerun/comparison.json), [setup audit](../../output/fresh-agent-rerun/comparison-setup.json), [revision attempt](../../output/fresh-agent-rerun/study/attempts/revise-wrela-0/attempt.json), and [handoff attempt](../../output/fresh-agent-rerun/study/attempts/handoff-wrela-0/attempt.json) retain measurements and artifact hashes. The [benchmark report](../../output/fresh-agent-rerun/study/report.json) leaves accepted productivity results and comparative ranking unestablished. Its stricter `constraintQualified` aggregate also requires known token/intervention costs; a zero there does not mean that the independent geometry checks failed.

## Where time went

Revision spent 1.942 seconds in the three recorded CLI calls. Its first candidate image was rendered 52.656 seconds after attempt start. Weathering spent 6.600 seconds in seven CLI calls, with the first candidate image at 84.660 seconds. These spans exclude shell-based source inspection and image viewing. Remaining wall time includes model latency/reasoning, file preparation, inspection and orchestration; it cannot all be called reasoning time.

The weathering agent made three reviewed alternatives rather than stopping at the stock brown timber recipe. It added pale uneven patina, foot staining and modest edge bevels while retaining the prior opening, member profiles, paths, sockets, route and object-level material ownership. One attempted layer relief of 0.12 exceeded the 0.02 schema limit; the rejected request was retained and corrected. It also corrected an overly dark lighting setup. The author's [continuation notes](../../output/fresh-agent-rerun/study/attempts/handoff-wrela-0/continuation-notes.md) retain decisions, failures and review limitations.

## Visual assessment and remaining gaps

The inspected final wood reads more plausibly than the striped starting material: grain follows each member, end faces show crosscut rings, and the patina is visible. The result remains a simple procedural frame with regular grain, limited physical damage and little distinctive construction detail. It is not evidence of AAA asset quality. This assessment is an informed, unblinded inspection; independent artistic acceptance has not been awarded.

The three prescribed views plus the original detail view retain matched baseline/candidate lighting. The two extra neutral/grazing stages were created only in the candidate. Their baseline contact-sheet panels silently used the original fallback stage; the agent disclosed this correctly. Those rows prove candidate appearance under additional lighting, not a matched before/after comparison. The review API should make missing-stage fallback explicit. Material parameter bounds/units and review-stage setup also remain discovery friction, as shown by the invalid relief value and manual lighting iteration.

Evidence: [final weathered overview](../../output/fresh-agent-rerun/study/attempts/handoff-wrela-0/brief-1-final.png), [neutral close-up](../../output/fresh-agent-rerun/study/attempts/handoff-wrela-0/neutral-detail-final.png), [grazing close-up](../../output/fresh-agent-rerun/study/attempts/handoff-wrela-0/grazing-detail-final.png), [full review sheet](../../output/fresh-agent-rerun/study/attempts/handoff-wrela-0/contact-sheet.png), and [unjudged blind gallery](../../output/fresh-agent-rerun/study/blind/index.html).

The next useful experiment is a held-out asset and repeated fresh runs. This rerun shows that the specific gate workflow became much cheaper; it does not yet show that agents can author arbitrary high-quality content equally quickly.
