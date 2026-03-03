import initClientRuntime, { start_client } from "./client-runtime.js";

function setBootStatus(status, detail) {
  const statusEl = document.getElementById("boot-status");
  const detailEl = document.getElementById("boot-detail");
  if (statusEl) {
    statusEl.textContent = status;
  }
  if (detailEl) {
    detailEl.textContent = detail;
  }
}

function hideBootOverlay() {
  const overlay = document.getElementById("boot-overlay");
  if (overlay) {
    overlay.style.display = "none";
  }
}

function showBootError(error) {
  const message =
    error instanceof Error ? `${error.name}: ${error.message}` : String(error);
  setBootStatus("Failed to start demo", "See browser console for details.");
  const detailEl = document.getElementById("boot-detail");
  if (detailEl) {
    detailEl.style.color = "#ffc7c7";
    detailEl.textContent = message;
  }
}

function readRuntimeState() {
  if (typeof window.render_game_to_text !== "function") {
    return null;
  }
  const raw = window.render_game_to_text();
  if (typeof raw !== "string" || raw.length === 0) {
    return null;
  }
  try {
    return JSON.parse(raw);
  } catch {
    return null;
  }
}

function stateIndicatesReady(state) {
  if (!state || typeof state !== "object") {
    return false;
  }
  const status = typeof state.status === "string" ? state.status : "";
  const isLoading = /^loading\b/i.test(status.trim());
  const renderedEnemyInstances = Number(
    state?.combat_camera?.rendered_enemy_instance_count ?? 0
  );
  return !isLoading && renderedEnemyInstances > 0;
}

function watchForRuntimeReady() {
  const startAt = performance.now();
  let lastDetail = "";
  const tick = () => {
    const state = readRuntimeState();
    if (state) {
      const runtimeStatus =
        typeof state.status === "string" ? state.status : "";
      if (runtimeStatus && runtimeStatus !== lastDetail) {
        setBootStatus("Starting world...", runtimeStatus);
        lastDetail = runtimeStatus;
      }
      if (stateIndicatesReady(state)) {
        hideBootOverlay();
        return;
      }
    }
    if (performance.now() - startAt > 45000) {
      setBootStatus(
        "Still loading...",
        "Shader compilation is taking longer than expected."
      );
    }
    requestAnimationFrame(tick);
  };
  requestAnimationFrame(tick);
}

async function boot() {
  setBootStatus(
    "Loading WebGPU runtime...",
    "Preparing assets and shaders. This can take a few seconds on first load."
  );
  try {
    await initClientRuntime(new URL("./client-runtime_bg.wasm", import.meta.url));
    setBootStatus("Starting world...", "Initializing game systems.");
    start_client({
      appMode: "__APP_MODE__",
      readyStatusLine: "__READY_STATUS_LINE__",
    });
    watchForRuntimeReady();
  } catch (error) {
    showBootError(error);
    throw error;
  }
}

boot();
