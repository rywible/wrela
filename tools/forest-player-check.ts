import { join } from "node:path";
import { waitFor, withBrowser } from "./browser";

await withBrowser(
  async (view, output, errors) => {
    await view.navigate("http://127.0.0.1:4181/player?forest=1");
    await waitFor(view, "window.wrelaPlayer?.ready", 120000);
    const before = await view.evaluate<{ subject: string; camera: { position: number[] } }>(
      "wrelaPlayer.inspect()",
    );
    if (before.subject !== "forest-edge") throw Error("Wrong forest entry");
    await view.click("[data-forest-walk]");
    await view.evaluate(
      "new Promise(resolve=>{let n=0;function next(){if(++n>=60)resolve(true);else requestAnimationFrame(next)}requestAnimationFrame(next)})",
    );
    const after = await view.evaluate<typeof before>("wrelaPlayer.inspect()");
    if (after.camera.position[2] >= before.camera.position[2] - 0.2) throw Error("Trail did not advance");
    await view.click("[data-forest-walk]");
    const measure = await view.evaluate<{ completeness: { complete: boolean }; frame: { adapter: string } }>(
      "wrelaPlayer.measure()",
    );
    if (!measure.completeness.complete || /software|swiftshader|llvmpipe/i.test(measure.frame.adapter))
      throw Error("Incomplete hardware frame");
    await Bun.write(join(output, "forest-player.png"), await view.screenshot());
    await Bun.write(
      join(output, "forest-player.json"),
      JSON.stringify({ before, after, measure, errors }, null, 2),
    );
    await view.evaluate("wrelaPlayer.dispose()");
    console.log(JSON.stringify({ output, before, after, complete: measure.completeness.complete }));
    if (errors.length) throw Error(errors.join("\n"));
  },
  1920,
  1080,
  "chrome",
  180000,
);
