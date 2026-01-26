# Runtime ABI Versioning Policy

Goals:
- Keep the runtime ABI stable within a v0.x line.
- Make breaking ABI changes explicit and easy to detect.

Policy:
- The runtime ABI is considered stable for a given major version (v0.x).
- Any breaking change to runtime symbols, calling conventions, or value layouts
  requires a major version bump (v1.0.0 or higher).
- Additive changes (new runtime symbols that do not alter existing ones) may be
  released in minor versions.
- Patch releases must not change ABI behavior.

Process:
- Document ABI changes in release notes.
- Update this policy if/when the ABI is versioned independently of the language.
