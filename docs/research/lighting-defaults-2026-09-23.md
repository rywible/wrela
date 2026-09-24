# Automatic cache and product lighting integration — September 23, 2026

The player GI URL flag and Studio bounce-lighting switch have been removed. Both retain their common renderer lighting. The bounded static transport solver now attempts surface caching automatically; options are diagnostic overrides. Empty/unadmitted products retain no surface-cache buffer. A changed receiver normal or selected representation invalidates the product; omitted options and explicit defaults reuse it.

**This change does not add production world GI or improve the current world image.** It removes product feature fragmentation and promotes a measured optimization inside the experimental solver. The next world delivery and limitations are stated in the [lighting contract](../architecture/indirect-lighting.md).

Validation: 49 tests passed (493 assertions), both TypeScript checks passed, and workspace boundary checks passed. Targeted formatter/linter checks passed on the transport and fixture changes; the broader touched-file check reports pre-existing import/format issues in player, scene-host, Studio UI and a concurrent Studio controller edit, preserved rather than reformatted across concurrent work.

A frozen source was captured with fingerprint `fc6ef490e92695481db9441aeec8e380a6e88081a4cb4b746ba676da8aa718a1`. The surface fixture now omits the surface-cache option entirely. At 1280×960 on Apple Metal, all image gates and browser/GPU error checks passed, including source intensity edits, moved camera, rebasing, uncached restore and constant-source controls. The enclosed scene stayed dark. These are small static room tests, not world performance evidence or AAA visual approval.

| Scene | Uncached GPU frame ms | Automatic cache ms | Maximum linear RGB difference | Cached visible receivers |
| --- | ---: | ---: | ---: | ---: |
| matte | 10.289 | 7.111 | 0.0017090 | 58.0% |
| metal | 9.535 | 6.980 | 0.0056152 | 58.0% |
| sealed | 14.352 | 10.027 | 0.0000000 | 50.6% |

GPU frame numbers are conventional medians of 24 raw timestamps in ABBA order. Build/upload work is excluded. Cache storage adds 2,291,632 bytes in the open rooms and 3,437,440 in the sealed room. Cache construction took 1.23–1.63 seconds here; total field construction 1.99–2.78 seconds. The largest cooperative CPU slice was 6.9 ms, so the 2 ms slice target is not a deadline.

Raw report: `output/lighting-defaults-2026-09-23/source/output/browser-1790201503750-68300/surface-lighting.json`. Frozen source and captures are retained under `output/lighting-defaults-2026-09-23/`. Earlier captures and rejected approaches remain unchanged in their historical reports.
