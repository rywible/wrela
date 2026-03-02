import fs from "node:fs";
import path from "node:path";
import { chromium } from "playwright";

function parseArgs(argv) {
  const parsed = {
    url: "",
    screenshotPath: "",
    previewReportPath: "",
    interactionLogPath: "",
    assertionSummaryPath: "",
    screenshotsPath: "",
    consoleErrorSummaryPath: "",
  };

  // Legacy positional form:
  // browser_smoke.mjs <url> <screenshotPath> <reportPath>
  if (argv.length === 5 && !argv[2].startsWith("--")) {
    parsed.url = argv[2];
    parsed.screenshotPath = argv[3];
    parsed.previewReportPath = argv[4];
    parsed.interactionLogPath = path.join(path.dirname(parsed.previewReportPath), "interaction-log.json");
    parsed.assertionSummaryPath = path.join(path.dirname(parsed.previewReportPath), "assertion-summary.json");
    parsed.screenshotsPath = path.join(path.dirname(parsed.previewReportPath), "screenshots.json");
    parsed.consoleErrorSummaryPath = path.join(
      path.dirname(parsed.previewReportPath),
      "console-error-summary.json"
    );
    return parsed;
  }

  for (let i = 2; i < argv.length; i++) {
    const arg = argv[i];
    const next = argv[i + 1];
    if (!next) continue;
    if (arg === "--url") {
      parsed.url = next;
      i++;
    } else if (arg === "--screenshot-path") {
      parsed.screenshotPath = next;
      i++;
    } else if (arg === "--preview-report-path") {
      parsed.previewReportPath = next;
      i++;
    } else if (arg === "--interaction-log-path") {
      parsed.interactionLogPath = next;
      i++;
    } else if (arg === "--assertion-summary-path") {
      parsed.assertionSummaryPath = next;
      i++;
    } else if (arg === "--screenshots-path") {
      parsed.screenshotsPath = next;
      i++;
    } else if (arg === "--console-error-summary-path") {
      parsed.consoleErrorSummaryPath = next;
      i++;
    }
  }

  return parsed;
}

function requirePath(value, flagName) {
  if (!value || value.trim().length === 0) {
    throw new Error(`missing required argument ${flagName}`);
  }
  return path.resolve(value);
}

function toNumber(value) {
  const parsed = Number(value);
  if (Number.isNaN(parsed)) {
    return 0;
  }
  return parsed;
}

async function readHud(page) {
  return page.evaluate(() => {
    const runtime = window.__wrelaRuntime ?? {};
    return {
      session: String(runtime.session ?? ""),
      score: String(runtime.score ?? "0"),
      ack: String(runtime.ack ?? "0"),
      corrections: String(runtime.corrections ?? "0"),
      tick: String(runtime.tick ?? "0"),
      pending: String(runtime.pending ?? "0"),
      divergence: String(runtime.divergence ?? "none"),
      hash: String(runtime.hash ?? "0x0"),
      status: String(runtime.status ?? ""),
    };
  });
}

async function readTextState(page) {
  return page.evaluate(() => {
    if (typeof window.render_game_to_text !== "function") {
      return { error: "render_game_to_text_unavailable" };
    }
    try {
      return JSON.parse(window.render_game_to_text());
    } catch (error) {
      return {
        error: `render_game_to_text_parse_failed: ${String(error)}`,
      };
    }
  });
}

async function advanceTime(page, ms) {
  await page.evaluate((deltaMs) => {
    if (typeof window.advanceTime === "function") {
      window.advanceTime(deltaMs);
    }
  }, ms);
}

async function holdKeyForSteps(page, interactionLog, key, steps) {
  const startedAt = Date.now();
  await page.keyboard.down(key);
  for (let i = 0; i < steps; i++) {
    await advanceTime(page, 16);
  }
  await page.keyboard.up(key);
  interactionLog.steps.push({
    index: interactionLog.steps.length,
    action: "key_steps",
    key,
    steps,
    durationMs: Date.now() - startedAt,
  });
}

async function driveToCollectible(page, interactionLog, targetX, targetY) {
  for (let attempt = 0; attempt < 120; attempt++) {
    const state = await readTextState(page);
    if (state.error) {
      throw new Error(state.error);
    }
    const playerX = toNumber(state?.player?.x ?? 0);
    const playerY = toNumber(state?.player?.y ?? 0);
    const dx = targetX - playerX;
    const dy = targetY - playerY;

    if (Math.abs(dx) <= 12 && Math.abs(dy) <= 12) {
      for (let settle = 0; settle < 6; settle++) {
        await advanceTime(page, 16);
      }
      return;
    }

    if (Math.abs(dx) > 12) {
      await holdKeyForSteps(page, interactionLog, dx > 0 ? "ArrowRight" : "ArrowLeft", 3);
    }
    if (Math.abs(dy) > 12) {
      await holdKeyForSteps(page, interactionLog, dy > 0 ? "ArrowDown" : "ArrowUp", 3);
    }
  }
}

async function triggerRestartAttempt(page, interactionLog, key, strategyLabel, holdSteps = 12) {
  interactionLog.steps.push({
    index: interactionLog.steps.length,
    action: "request_restart_attempt",
    key,
    strategy: strategyLabel,
    holdSteps,
  });
  await page.keyboard.down(key);
  for (let i = 0; i < holdSteps; i++) {
    await advanceTime(page, 16);
  }
  await page.keyboard.up(key);
}

async function requestRestartAndWait(page, interactionLog) {
  const attempts = [
    async () => triggerRestartAttempt(page, interactionLog, "r", "keyboard_r", 12),
    async () => triggerRestartAttempt(page, interactionLog, " ", "keyboard_space", 12),
    async () => {
      interactionLog.steps.push({
        index: interactionLog.steps.length,
        action: "request_restart_attempt",
        key: "r",
        strategy: "synthetic_event_r",
        holdSteps: 8,
      });
      await page.evaluate(() => {
        window.dispatchEvent(new KeyboardEvent("keydown", { key: "r" }));
      });
      for (let i = 0; i < 8; i++) {
        await advanceTime(page, 16);
      }
      await page.evaluate(() => {
        window.dispatchEvent(new KeyboardEvent("keyup", { key: "r" }));
      });
    },
  ];

  let restartState = null;
  for (let attemptIndex = 0; attemptIndex < attempts.length; attemptIndex++) {
    await attempts[attemptIndex]();
    for (let settle = 0; settle < 12; settle++) {
      await advanceTime(page, 16);
    }
    restartState = await readTextState(page);
    if (restartState.error) {
      throw new Error(restartState.error);
    }
    if (toNumber(restartState.score) === 0 && restartState.won === false) {
      return restartState;
    }
  }

  return restartState ?? (await readTextState(page));
}

function buildAssertions(startHud, endHud, startState, pickupState, restartState, metrics, consoleErrors) {
  const sessionReady = endHud.session && endHud.session !== "0" && endHud.session !== "pending";
  const movedDistance =
    Math.abs(toNumber(pickupState?.player?.x) - toNumber(startState?.player?.x)) +
    Math.abs(toNumber(pickupState?.player?.y) - toNumber(startState?.player?.y));
  const movedPlayer = movedDistance > 4;
  const scoreIncreased = toNumber(pickupState?.score) > toNumber(startState?.score);
  const winReached = Boolean(pickupState?.won);
  const restartResetScore = toNumber(restartState?.score) === 0;
  const restartClearsWin = restartState?.won === false;
  const drawCalls = toNumber(metrics?.gpu?.draw_calls ?? 0);
  const gpuUploadBytes = toNumber(metrics?.gpu?.gpu_upload_bytes ?? 0);
  const frameGraphPathUsed = Boolean(restartState?.frame_graph?.path_used);
  const renderPasses = toNumber(restartState?.frame_graph?.last_render_passes ?? 0);

  const assertions = [
    {
      name: "session_ready",
      passed: Boolean(sessionReady),
      details: `session=${endHud.session}`,
    },
    {
      name: "movement_observed",
      passed: movedPlayer,
      details: `moved_distance=${movedDistance.toFixed(2)}`,
    },
    {
      name: "pickup_collected_score_increased",
      passed: scoreIncreased,
      details: `start_score=${startState?.score ?? 0} pickup_score=${pickupState?.score ?? 0}`,
    },
    {
      name: "win_state_reached",
      passed: winReached,
      details: `won_after_pickup=${pickupState?.won === true}`,
    },
    {
      name: "restart_resets_score",
      passed: restartResetScore && restartClearsWin,
      details: `restart_score=${restartState?.score ?? 0} restart_won=${restartState?.won}`,
    },
    {
      name: "draw_calls_positive",
      passed: drawCalls > 0,
      details: `draw_calls=${drawCalls}`,
    },
    {
      name: "gpu_upload_bytes_positive",
      passed: gpuUploadBytes > 0,
      details: `gpu_upload_bytes=${gpuUploadBytes}`,
    },
    {
      name: "frame_graph_path_used",
      passed: frameGraphPathUsed,
      details: `frame_graph_path_used=${frameGraphPathUsed}`,
    },
    {
      name: "render_passes_executed",
      passed: renderPasses > 0,
      details: `render_passes=${renderPasses}`,
    },
    {
      name: "console_errors_absent",
      passed: consoleErrors.length === 0,
      details: `console_error_count=${consoleErrors.length}`,
    },
  ];

  const failed = assertions.filter((assertion) => !assertion.passed);
  return {
    assertions,
    failed,
    summary: {
      total: assertions.length,
      passed: assertions.length - failed.length,
      failed: failed.length,
      scoreDelta: toNumber(pickupState?.score) - toNumber(startState?.score),
      drawCalls,
      gpuUploadBytes,
      frameGraphPathUsed,
      renderPasses,
    },
  };
}

async function run() {
  const raw = parseArgs(process.argv);
  if (!raw.url) {
    throw new Error("missing required argument --url");
  }

  const url = raw.url;
  const screenshotPath = requirePath(raw.screenshotPath, "--screenshot-path");
  const previewReportPath = requirePath(raw.previewReportPath, "--preview-report-path");
  const interactionLogPath = requirePath(raw.interactionLogPath, "--interaction-log-path");
  const assertionSummaryPath = requirePath(raw.assertionSummaryPath, "--assertion-summary-path");
  const screenshotsPath = requirePath(raw.screenshotsPath, "--screenshots-path");
  const consoleErrorSummaryPath = requirePath(
    raw.consoleErrorSummaryPath,
    "--console-error-summary-path"
  );

  const browser = await chromium.launch({
    headless: true,
    args: ["--enable-unsafe-webgpu", "--disable-dawn-features=disallow_unsafe_apis"],
  });
  const context = await browser.newContext({ viewport: { width: 1360, height: 920 } });
  const page = await context.newPage();

  const consoleEvents = [];
  const consoleErrors = [];
  page.on("console", (message) => {
    const location = typeof message.location === "function" ? message.location() : null;
    const event = {
      type: "console",
      level: message.type(),
      text: message.text(),
      location,
      timestamp: new Date().toISOString(),
    };
    consoleEvents.push(event);
    if (event.level === "error") {
      consoleErrors.push(event);
    }
  });

  page.on("pageerror", (error) => {
    const event = {
      type: "pageerror",
      level: "error",
      text: String(error),
      location: null,
      timestamp: new Date().toISOString(),
    };
    consoleEvents.push(event);
    consoleErrors.push(event);
  });

  const interactionLog = {
    url,
    startedAt: new Date().toISOString(),
    steps: [],
  };

  await page.goto(url, { waitUntil: "domcontentloaded", timeout: 45_000 });
  await page.waitForFunction(() => typeof window.__wrelaRuntime === "object", { timeout: 45_000 });
  await page.waitForFunction(() => typeof window.__wrelaMetrics === "object", { timeout: 45_000 });
  await page.waitForFunction(() => typeof window.render_game_to_text === "function", {
    timeout: 45_000,
  });
  await page.waitForFunction(() => typeof window.advanceTime === "function", { timeout: 45_000 });
  await page.waitForFunction(() => {
    try {
      if (typeof window.render_game_to_text !== "function") {
        return false;
      }
      const payload = JSON.parse(window.render_game_to_text());
      return Array.isArray(payload.collectibles) && payload.collectibles.length > 0;
    } catch (_error) {
      return false;
    }
  }, { timeout: 45_000 });

  const startHud = await readHud(page);
  const startTextState = await readTextState(page);
  if (startTextState.error) {
    throw new Error(startTextState.error);
  }

  const candidates = (startTextState.collectibles ?? []).filter((entry) => !entry.collected);
  if (candidates.length === 0) {
    throw new Error("no collectible targets found in render_game_to_text payload");
  }
  const firstTarget = candidates[0];
  interactionLog.steps.push({
    index: interactionLog.steps.length,
    action: "target_collectible",
    collectibleIndex: firstTarget.index,
    x: firstTarget.x,
    y: firstTarget.y,
  });

  await driveToCollectible(page, interactionLog, toNumber(firstTarget.x), toNumber(firstTarget.y));

  let pickupTextState = await readTextState(page);
  if (pickupTextState.error) {
    throw new Error(pickupTextState.error);
  }
  if (toNumber(pickupTextState.score) <= toNumber(startTextState.score)) {
    for (let retry = 0; retry < 30; retry++) {
      await advanceTime(page, 16);
      pickupTextState = await readTextState(page);
      if (pickupTextState.error) {
        throw new Error(pickupTextState.error);
      }
      if (toNumber(pickupTextState.score) > toNumber(startTextState.score)) {
        break;
      }
    }
  }

  const restartTextState = await requestRestartAndWait(page, interactionLog);
  if (restartTextState.error) {
    throw new Error(restartTextState.error);
  }
  const endHud = await readHud(page);
  const metrics = await page.evaluate(() => window.__wrelaMetrics ?? {});

  fs.mkdirSync(path.dirname(screenshotPath), { recursive: true });
  await page.screenshot({ path: screenshotPath, fullPage: true });

  const screenshotPaths = [screenshotPath];
  const assertionResult = buildAssertions(
    startHud,
    endHud,
    startTextState,
    pickupTextState,
    restartTextState,
    metrics,
    consoleErrors
  );

  const assertionSummary = {
    ...assertionResult.summary,
    assertions: assertionResult.assertions,
    failures: assertionResult.failed,
  };
  const consoleErrorSummary = {
    totalEvents: consoleEvents.length,
    errorCount: consoleErrors.length,
    errors: consoleErrors,
  };
  const screenshotSummary = {
    count: screenshotPaths.length,
    paths: screenshotPaths,
  };

  interactionLog.finishedAt = new Date().toISOString();

  const previewReport = {
    schemaVersion: 1,
    url,
    passed: assertionResult.failed.length === 0,
    start: startHud,
    end: endHud,
    textStates: {
      start: startTextState,
      pickup: pickupTextState,
      restart: restartTextState,
    },
    metrics,
    artifacts: {
      previewReportPath,
      interactionLogPath,
      assertionSummaryPath,
      screenshotsPath,
      consoleErrorSummaryPath,
      screenshotPaths,
    },
    assertionSummary,
    consoleErrorSummary,
  };

  fs.mkdirSync(path.dirname(interactionLogPath), { recursive: true });
  fs.writeFileSync(interactionLogPath, JSON.stringify(interactionLog, null, 2));
  fs.writeFileSync(assertionSummaryPath, JSON.stringify(assertionSummary, null, 2));
  fs.writeFileSync(screenshotsPath, JSON.stringify(screenshotSummary, null, 2));
  fs.writeFileSync(consoleErrorSummaryPath, JSON.stringify(consoleErrorSummary, null, 2));
  fs.writeFileSync(previewReportPath, JSON.stringify(previewReport, null, 2));

  await context.close();
  await browser.close();

  if (!previewReport.passed) {
    const failedList = assertionResult.failed.map((item) => item.name).join(", ") || "unknown";
    throw new Error(
      `browser smoke assertions failed: ${failedList}. See ${assertionSummaryPath} and ${previewReportPath}`
    );
  }
}

run().catch((error) => {
  const message = error instanceof Error ? error.message : String(error);
  console.error(message);
  process.exit(1);
});
