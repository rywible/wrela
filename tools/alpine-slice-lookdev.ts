import { rename } from "node:fs/promises";
import { join, resolve } from "node:path";
import { contentKey } from "@wrela/model";

import { type AlpineFramePoll, type AlpineFrameResult, captureAlpineFrame } from "./alpine-slice-capture";
import {
  ALPINE_PROFILES,
  type AlpineComposition,
  type AlpineProfile,
  createAlpineSliceStudy,
  summarizeAlpineBudgets,
} from "./alpine-slice-study";
import { fixtureServer, waitFor, withBrowser } from "./browser";

const option = (name: string, fallback: string) =>
  process.argv.find((argument) => argument.startsWith(`--${name}=`))?.slice(name.length + 3) ?? fallback;
const profile = option("profile", "balanced") as AlpineProfile;
const composition = option("composition", "primary") as AlpineComposition;
if (!Object.hasOwn(ALPINE_PROFILES, profile) || !["primary", "river-bend"].includes(composition))
  throw Error("Unknown alpine review profile/composition");
const study = createAlpineSliceStudy(composition, profile);
await withBrowser(
  async (view, output, errors) => {
    await Bun.write(join(output, "authored-project.json"), JSON.stringify(study.project, null, 2));
    await Bun.write(join(output, "composition-operations.json"), JSON.stringify(study.operations, null, 2));
    let server: Awaited<ReturnType<typeof fixtureServer>> | undefined;
    const frames: AlpineFrameResult[] = [],
      capturedImages: string[] = [];
    let failureMessage: string | undefined;
    let status: "running" | "complete" | "failed" = "running";
    let activeFrame: { index: number; poll?: AlpineFramePoll } | undefined;
    const sourceKey = contentKey(study.project);
    const writeReport = async () => {
      const pending = join(output, "alpine-slice-review.pending.json");
      await Bun.write(
        pending,
        JSON.stringify(
          {
            composition,
            profile,
            sourceKey,
            status,
            frames,
            capturedImages,
            activeFrame,
            errors,
            failure: failureMessage,
            expectedFrames: study.cameras.length,
            captureComplete:
              status === "complete" && !errors.length && capturedImages.length === study.cameras.length,
            ...summarizeAlpineBudgets(frames, study.cameras.length),
            visualAcceptance: "unreviewed",
            hardwareScope: "Current adapter only; additional ordinary hardware must be measured separately.",
          },
          null,
          2,
        ),
      );
      await rename(pending, join(output, "alpine-slice-review.json"));
    };
    // Persist incomplete evidence before browser work, including a hard process timeout.
    await writeReport();
    try {
      server = await fixtureServer(
        `import {createAlpineSliceFixture} from ${JSON.stringify(resolve("tools/fixtures/alpine-slice-lookdev.ts"))};createAlpineSliceFixture(${JSON.stringify(study)}).then(f=>{window.fixture=f;window.ready=true}).catch(e=>window.failure=String(e));`,
        output,
      );
      await view.navigate(String(server.url));
      await waitFor(view, "window.ready||window.failure", 60000);
      const failure = await view.evaluate("window.failure");
      if (failure) throw Error(String(failure));
      for (let index = 0; index < study.cameras.length; index++) {
        activeFrame = { index };
        await writeReport();
        const frame = await captureAlpineFrame(view, index, {
          onProgress: async (poll) => {
            activeFrame = { index, poll };
            await writeReport();
          },
        });
        // Preserve completed measurements even if adapter validation or image readback fails.
        frames.push(frame);
        await writeReport();
        if (
          frame.measurements.some((measurement) => /swiftshader|software|llvmpipe/i.test(measurement.adapter))
        )
          throw Error("Hardware adapter required");
        const image = `${composition}-${frame.id}.png`;
        await Bun.write(join(output, image), await view.screenshot());
        capturedImages.push(image);
        activeFrame = undefined;
        await writeReport();
      }
      if (errors.length) throw Error(errors.join("\n"));
      status = "complete";
    } catch (error) {
      status = "failed";
      failureMessage = error instanceof Error ? error.message : String(error);
      throw error;
    } finally {
      try {
        await writeReport();
      } finally {
        await view.evaluate("window.fixture?.dispose()").catch(() => {});
        server?.stop(true);
      }
    }
    console.log(
      JSON.stringify({
        output,
        composition,
        profile,
        ...summarizeAlpineBudgets(frames, study.cameras.length),
        frames: frames.map((frame) => ({ id: frame.id, budget: frame.budget })),
      }),
    );
    if (!summarizeAlpineBudgets(frames, study.cameras.length).budgetMet) process.exitCode = 1;
  },
  ...study.contract.output,
);
