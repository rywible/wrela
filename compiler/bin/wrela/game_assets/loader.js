import initClientRuntime, { start_client } from "./client-runtime.js";

async function boot() {
  await initClientRuntime(new URL("./client-runtime_bg.wasm", import.meta.url));
  start_client({
    appMode: "__APP_MODE__",
    readyStatusLine: "__READY_STATUS_LINE__",
  });
}

boot();
