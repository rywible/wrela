import { join, relative, resolve } from "node:path";
import {
  AUTHORING_STUDIES,
  type AuthoringLookdevReport,
  authoringAcceptance,
  renderAuthoringLookdev,
} from "./authoring-lookdev-report";
import { snapshotSource } from "./source-snapshot";

const selected = process.argv
  .find((arg) => arg.startsWith("--study="))
  ?.slice(8)
  .split(",");
if (selected?.some((id) => !AUTHORING_STUDIES.some((s) => s.id === id)))
  throw new Error("Unknown authoring study");
const root = resolve("output/authoring-lookdev", `${Date.now()}-${process.pid}`);
const source = join(root, "source");
const manifest = await snapshotSource(source, "authoring-lookdev-suite");
const report: AuthoringLookdevReport = {
  sourceFingerprint: manifest.sourceFingerprint,
  startedAt: manifest.startedAt,
  studies: [],
};
const collect = () =>
  new Set(
    Array.from(new Bun.Glob("output/**/*.{png,webm,json}").scanSync({ cwd: source })).filter(
      (path) => !path.endsWith("source-manifest.json"),
    ),
  );
const persist = async () => {
  await Bun.write(
    join(root, "report.json"),
    JSON.stringify({ ...report, acceptance: authoringAcceptance(report) }, null, 2),
  );
  await Bun.write(join(root, "index.html"), renderAuthoringLookdev(report));
};
await persist();
console.log(
  JSON.stringify({ review: join(root, "index.html"), sourceFingerprint: manifest.sourceFingerprint }),
);
for (const study of AUTHORING_STUDIES.filter((s) => !selected || selected.includes(s.id))) {
  const before = collect(),
    started = performance.now();
  const logPath = join(root, `${study.id}.log`);
  console.log(`Capturing ${study.name}`);
  const commands = [
    [study.tool, ...study.args],
    ...("followups" in study ? study.followups.map((tool) => [tool]) : []),
  ];
  let exitCode = 0;
  let log = "";
  for (const command of commands) {
    const child = Bun.spawn([process.execPath, ...command], {
      cwd: source,
      stdout: "pipe",
      stderr: "pipe",
    });
    const [stdout, stderr, code] = await Promise.all([
      new Response(child.stdout).text(),
      new Response(child.stderr).text(),
      child.exited,
    ]);
    log += `${command.join(" ")}\n${stdout}${stderr}\n`;
    if (code !== 0) exitCode = code;
    await Bun.write(logPath, log);
  }
  const artifacts = [...collect()]
    .filter((path) => !before.has(path))
    .map((path) => relative(root, join(source, path)))
    .sort();
  report.studies.push({
    id: study.id,
    exitCode,
    durationSeconds: (performance.now() - started) / 1000,
    artifacts,
    log: relative(root, logPath),
    visualAcceptance: "unreviewed",
    observations: [],
  });
  await persist();
  console.log(
    JSON.stringify({ study: study.id, exitCode, images: artifacts.filter((p) => p.endsWith(".png")).length }),
  );
}
console.log(JSON.stringify({ review: join(root, "index.html"), ...authoringAcceptance(report) }));
if (report.studies.some((s) => s.exitCode !== 0 || !s.artifacts.some((a) => a.endsWith(".png"))))
  process.exitCode = 1;
