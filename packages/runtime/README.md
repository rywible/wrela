Runtime integration

Games submit tick commands (`setTarget`, `setFacing`, `playMotion`) and call
`host.advance(seconds, camera)` followed by `host.extract(camera)`. Extraction
never advances physics or changes residency. `evaluate(time, camera)` remains the
editor playhead adapter. Scene time comes from the simulation clock.

Controller-driven locomotion should call `runtime.setRootMotionPolicy(id, "visual")`.
That tick command keeps root translation/rotation in the rendered pose while
physics follows only submitted targets and facing. The default `"physical"`
policy preserves authored motion previews. The policy is included in replay and
runtime saves; older saves default to physical root motion.

`runtime.setFocusCharacter(id)` selects the activation centre. Nearby actors
share bounded physical interests; distant actors retain their exact dormant
state. `bodyState(id)` and `entityLifecycle` cover both states. Replay commands
are tick indexed and coalesced by actor/action. As retained input reaches its
budget, the current checkpoint becomes the oldest accessible replay boundary;
`replayUsage` reports that boundary.

Animation markers

`runtime.registerMotionEvents(definitionId, motionId, markers)` attaches a
release-authored semantic track to an existing compiled clip. Each marker has
`id`, `time` in seconds, and optional primitive-valued `payload`. Tracks are
validated, copied and sorted once, with at most 64 markers per clip and 256
registered tracks. Fixed simulation steps emit each crossed marker once,
including loop boundaries. Only the active clip emits during a blend.

`runtime.drainAnimationEvents()` returns `{ events, dropped }`. Events identify
the actor, definition, clip, marker, simulation tick, cycle and time. The queue
holds at most 1,024 events; overflow is explicit. Consumers should drain every
simulation update and reject dropped events when completeness matters. Editor
seek and save restoration clear transient events; replay does not emit audio or
gameplay side effects. Registered marker tracks are release content, not mutable
saved progress. This is a clip/event interface, not a general animation graph.

Save compatibility

`host.loadRuntime(save)` rejects incompatible world generators or character
artifacts by default. A release can deliberately support a known compatible
change by supplying `{ migration }`: a named plan containing exact `definition`,
`fromArtifactKey`, `toArtifactKey` rules and optional `motionIds` remaps.

`host.prepareRuntimeSave(save, migration)` validates a migrated copy and returns
`{ save, migration: { id, changedEntities } }` without changing the running world.
The original input remains untouched. `loadRuntime(save, { migration })` applies
the same plan and returns its report with restored time. Rules do not bypass
world-generator, live-entity, transform, physics, scale or motion validation.
They cannot rewrite arbitrary saved fields. Compatibility is a release decision;
no migration is inferred from similar-looking geometry.

Resource accounting

`host.resourceUsage.liveBytes` deduplicates buffers across installed products,
cache entries and detail variants. It includes estimated artifact metadata, but
excludes unpublished worker results and JavaScript-engine object overhead.
`cacheBytes` and `installedBytes` are overlapping ownership views and should not
be summed as total live memory.
