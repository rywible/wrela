import { join } from "node:path";
import { pathToFileURL } from "node:url";
import { waitFor, withBrowser } from "./browser";
import { buildStudio } from "./build";

await withBrowser(
  async (view, output, errors) => {
    const directory = await buildStudio(join(output, "game-build"));
    await view.navigate(pathToFileURL(join(directory, "games/switch-gate/index.html")).href);
    await waitFor(view, "window.wrelaGame?.measure().delivery.launchMs > 0");
    const delivery = await view.evaluate<{
      launchMs: number;
      cookedDocuments: string[];
      sourceDocuments: string[];
    }>("wrelaGame.measure().delivery");
    if (delivery.cookedDocuments.length !== 3 || delivery.sourceDocuments.length)
      throw Error(`Exported game failed to use its cooked products: ${JSON.stringify(delivery)}`);
    const result = await view.evaluate<{
      saved: unknown;
      won: { state: { won: boolean } };
      restored: unknown;
      adapter: string;
    }>(`(async()=>{
      await wrelaGame.step(1); wrelaGame.input({move:[1,0],interact:true}); await wrelaGame.step(44);
      const saved=await wrelaGame.save(); if(!saved.state.activated)throw Error('Switch did not activate');
      await wrelaGame.step(160); const won=wrelaGame.inspect(); if(!won.state.won)throw Error('Objective did not complete');
      await wrelaGame.load(saved); const restored=await wrelaGame.save();
      if(JSON.stringify(saved)!==JSON.stringify(restored))throw Error('Save/reopen differs');
      wrelaGame.input({move:[1,0]}); await wrelaGame.step(160);
      return {saved,won,restored,adapter:wrelaGame.measure().measurements.adapter};
    })()`);
    if (/software|swiftshader|llvmpipe/i.test(result.adapter))
      throw Error("Hardware WebGPU adapter required");
    const initialGraphics = await view.evaluate("wrelaGame.measure()");
    await Bun.write(join(output, "initial-graphics.json"), JSON.stringify(initialGraphics, null, 2));
    await view.evaluate(
      "(async()=>{for(let i=0;i<12;i++){await wrelaGame.capture();if(wrelaGame.measure().completeness.complete)break}})()",
    );
    await waitFor(view, "wrelaGame.measure().completeness.complete");
    const capture = await view.evaluate<string>(
      "(async()=>{const blob=await wrelaGame.capture();return await new Promise((resolve,reject)=>{const reader=new FileReader();reader.onload=()=>resolve(reader.result);reader.onerror=()=>reject(reader.error);reader.readAsDataURL(blob)})})()",
    );
    await Bun.write(join(output, "switch-gate-render.png"), Buffer.from(capture.split(",")[1], "base64"));
    await Bun.write(join(output, "switch-gate.png"), await view.screenshot());
    const graphics = await view.evaluate("wrelaGame.measure()");
    if (errors.length) throw Error(errors.join("\n"));
    await Bun.write(
      join(output, "agent-game.json"),
      JSON.stringify({ ...result, delivery, graphics, errors }, null, 2),
    );
    console.log(
      JSON.stringify(
        {
          output,
          adapter: result.adapter,
          objective: "completed",
          persistence: "identical",
          delivery,
          graphics,
        },
        null,
        2,
      ),
    );
  },
  960,
  640,
  "chrome",
  180000,
);
