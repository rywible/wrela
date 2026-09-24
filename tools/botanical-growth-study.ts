import { mkdir } from "node:fs/promises";
import { resolve } from "node:path";
import { analyzeBotanicalGrowth } from "@wrela/compiler/botanical-analysis";
import { growBotanical } from "@wrela/compiler/botanical-growth";
import {
  botanicalDevelopmentSchema,
  botanicalSpeciesTraits,
  type GrowthEnvironment,
  type GrowthSpecies,
} from "@wrela/model";
import { snapshotSource } from "./source-snapshot";

if (!process.argv.includes("--snapshot-run")) {
  const destination = resolve("output/botanical-growth-studies", `${Date.now()}-${process.pid}`, "source");
  await snapshotSource(destination, "botanical-growth-study");
  const child = Bun.spawn([process.execPath, "tools/botanical-growth-study.ts", "--snapshot-run"], {
    cwd: destination,
    stdout: "inherit",
    stderr: "inherit",
  });
  console.log(JSON.stringify({ snapshot: destination }));
  process.exit(await child.exited);
}

const output = resolve("output/botanical-growth", `${Date.now()}`);
await mkdir(output, { recursive: true });
const base = botanicalDevelopmentSchema.parse({ species: "lodgepole-pine" }).environment;
const environments: Record<string, GrowthEnvironment> = {
  open: base,
  neighbor: {
    ...base,
    neighbors: [{ id: "neighbor", center: [2, 5, 0], radius: 3.5, height: 9, opacity: 0.93 }],
  },
  dry: { ...base, water: 0.25 },
  poor: { ...base, fertility: 0.3 },
};
const reports: (ReturnType<typeof analyzeBotanicalGrowth> & { environment: string })[] = [];
for (const species of ["lodgepole-pine", "paper-birch"] as GrowthSpecies[])
  for (const [environment, settings] of Object.entries(environments)) {
    const svg: string[] = [
      '<svg xmlns="http://www.w3.org/2000/svg" width="2560" height="2560" viewBox="0 0 2560 2560"><rect width="2560" height="2560" fill="#eef0e7"/>',
    ];
    for (let seed = 0; seed < 32; seed++) {
      let checkpoint: ReturnType<typeof growBotanical> | undefined;
      for (const [age, steps] of [4, 8, 16, 24].entries()) {
        const source = botanicalDevelopmentSchema.parse({ species, steps, environment: settings });
        checkpoint = growBotanical(1009 + seed * 7919, source, checkpoint);
        const report = analyzeBotanicalGrowth(checkpoint);
        reports.push({ ...report, environment });
        if (report.violations.length) throw Error(JSON.stringify(report));
        const x = (seed % 8) * 320 + age * 80 + 40,
          y = Math.floor(seed / 8) * 640 + 610;
        const scale = 36;
        svg.push(
          `<g><text x="${x - 35}" y="${y + 15}" font-size="10" fill="#354638">${seed} / ${steps}</text>`,
        );
        for (const shoot of checkpoint.shoots)
          if (shoot.state !== "pruned") {
            const p = (v: number[]) =>
              `${(x + (v[0] + v[2] * 0.35) * scale).toFixed(2)},${(y - v[1] * scale).toFixed(2)}`;
            svg.push(
              `<path d="M${p(shoot.start)} L${p(shoot.end)}" stroke="${shoot.state === "dead" ? "#887b61" : "#665b46"}" stroke-width="${Math.max(0.25, shoot.radius * scale * 2).toFixed(2)}"/>`,
            );
            if (shoot.foliage.retained > 0)
              svg.push(
                `<circle cx="${(x + (shoot.end[0] + shoot.end[2] * 0.35) * scale).toFixed(2)}" cy="${(y - shoot.end[1] * scale).toFixed(2)}" r="${Math.max(0.5, shoot.foliage.retained * 1.1)}" fill="#4b6a40" opacity=".6"/>`,
              );
          }
        svg.push("</g>");
      }
    }
    svg.push("</svg>");
    await Bun.write(resolve(output, `${species}-${environment}.svg`), svg.join("\n"));
    console.log(JSON.stringify({ species, environment, completed: reports.length }));
  }
await Bun.write(
  resolve(output, "study.json"),
  JSON.stringify(
    {
      version: 1,
      scope:
        "Structural validation and uncalibrated developmental descriptors. Contact sheets are orthographic graph views, not rendering acceptance or botanical calibration.",
      traits: botanicalSpeciesTraits,
      environments,
      ages: [4, 8, 16, 24],
      seeds: 32,
      limited: reports.filter((report) => report.limited).length,
      qualification: "uncalibrated-structural-study",
      reports,
    },
    null,
    2,
  ),
);
console.log(output);
