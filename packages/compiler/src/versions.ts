/** Every persisted product declares its algorithm and format versions. Bump the
 * relevant product version when its meaning or output changes. Cook manifests
 * additionally record the exact compiler source fingerprint supplied by builds. */
export const COMPILER_VERSION = "wrela-compiler-2";
declare const WRELA_COMPILER_SOURCE: string | undefined;
export const BUNDLE_COMPILER_SOURCE =
  typeof WRELA_COMPILER_SOURCE === "string" ? WRELA_COMPILER_SOURCE : undefined;
export const PRODUCT_VERSIONS = Object.freeze({
  geometry: "surface-2",
  binding: "envelope-1",
  motion: "motion-1",
  material: "assignment-1",
  vegetation: "conifer-2",
});
export const COOK_FORMAT_VERSION = 1;
