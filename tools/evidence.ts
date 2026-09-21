import { cpus, platform, release, totalmem } from "node:os";
import { join } from "node:path";

export type SourceManifest = Awaited<ReturnType<typeof sourceManifest>>;

/** Hash the actual checkout, including untracked source. Research is recorded separately. */
export async function sourceManifest(scenario: string) {
  const paths = Array.from(
    new Bun.Glob("{apps,packages,tools}/**/*.{ts,tsx,css,html,wgsl,json}").scanSync("."),
  )
    .filter((path) => !/^tools\/(?:field|transport)-research\//.test(path))
    .concat([
      "package.json",
      "bun.lock",
      ".bun-version",
      "tsconfig.json",
      "tsconfig.browser.json",
      "biome.json",
      "AGENTS.md",
      ".github/workflows/verify.yml",
    ])
    .sort();
  const files = await Promise.all(
    paths.map(async (path) => {
      const bytes = new Uint8Array(await Bun.file(path).arrayBuffer());
      return {
        path,
        bytes: bytes.byteLength,
        sha256: new Bun.CryptoHasher("sha256").update(bytes).digest("hex"),
      };
    }),
  );
  const git = async (args: string[]) => {
    const result = Bun.spawn(["git", ...args], { stdout: "pipe", stderr: "ignore" });
    const output = await new Response(result.stdout).text();
    return (await result.exited) === 0 ? output.trim() : null;
  };
  const [head, status] = await Promise.all([git(["rev-parse", "HEAD"]), git(["status", "--short"])]);
  return {
    schemaVersion: 1,
    scenario,
    startedAt: new Date().toISOString(),
    sourceFingerprint: new Bun.CryptoHasher("sha256").update(JSON.stringify(files)).digest("hex"),
    files,
    git: { head, status },
    tool: { bun: Bun.version },
    host: { platform: platform(), release: release(), cpu: cpus()[0]?.model, memoryBytes: totalmem() },
    excluded: [
      "tools/field-research/** (independent research task)",
      "tools/transport-research/** (independent research task)",
      "output/**",
      "dist/**",
    ],
  };
}

export async function writeEvidence(directory: string, scenario: string) {
  const manifest = await sourceManifest(scenario);
  await Bun.write(join(directory, "source-manifest.json"), JSON.stringify(manifest, null, 2));
  return manifest;
}

export function distribution(values: number[]) {
  if (!values.length) return null;
  const sorted = [...values].sort((a, b) => a - b);
  const percentile = (fraction: number) =>
    sorted[Math.min(sorted.length - 1, Math.ceil(sorted.length * fraction) - 1)];
  return {
    samples: values.length,
    mean: values.reduce((sum, value) => sum + value, 0) / values.length,
    p50: percentile(0.5),
    p95: percentile(0.95),
    p99: percentile(0.99),
    max: sorted[sorted.length - 1],
  };
}
