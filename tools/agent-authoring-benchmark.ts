import { createBlindReview, reportBenchmark } from "./agent-benchmark/review";
import { engineAvailability, runBenchmarkAttempt } from "./agent-benchmark/runner";
import { prepareBenchmark } from "./agent-benchmark/suite";

export async function runAgentBenchmark(args: string[]) {
  const [command, directory, file] = args;
  if (command === "availability") return engineAvailability();
  if (!directory)
    throw Error(
      "Usage: bench:agents prepare <directory> <config.json> | run <directory> <runner.json> | blind/report <directory> | availability",
    );
  if (command === "prepare") return prepareBenchmark(directory, await Bun.file(file).json());
  if (command === "run") return runBenchmarkAttempt(directory, await Bun.file(file).json());
  if (command === "blind") return createBlindReview(directory);
  if (command === "report") return reportBenchmark(directory);
  throw Error("Unknown benchmark command");
}
if (import.meta.main) {
  try {
    console.log(JSON.stringify(await runAgentBenchmark(process.argv.slice(2)), null, 2));
  } catch (error) {
    console.error(String(error));
    process.exitCode = 1;
  }
}
