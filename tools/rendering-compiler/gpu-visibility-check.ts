import { resolve } from "node:path";
import { fixtureServer, waitFor, withBrowser } from "../browser";

const source = `import { verifyGpuVisibility } from ${JSON.stringify(resolve("tools/rendering-compiler/gpu-visibility-fixture.ts"))};
verifyGpuVisibility().then(result=>{window.result=result}).catch(error=>{window.failure=String(error)});`;
await withBrowser(async (view, output, errors) => {
  const server = await fixtureServer(source, output);
  try {
    await view.navigate(server.url.toString());
    await waitFor(view, "window.result || window.failure", 60000);
    const failure = await view.evaluate("window.failure");
    if (failure) throw new Error(String(failure));
    const result = await view.evaluate("window.result");
    if (errors.length) throw new Error(errors.join("\n"));
    console.log(JSON.stringify(result, null, 2));
  } finally {
    server.stop(true);
  }
});
