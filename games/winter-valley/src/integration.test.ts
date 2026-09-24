import { expect, test } from "bun:test";
import { referenceProject } from "@wrela/examples";
import type { Vec3 } from "@wrela/model";
import { BrowserSceneHost } from "@wrela/runtime/scene-host";
import { WINTER_HOME, WINTER_MARKERS, WINTER_STEP, WinterValleyGame } from "./rules";

test("the bunny can walk the authored winter trail through real collision and return home", async () => {
  const host = new BrowserSceneHost(referenceProject());
  try {
    await host.prepare("winter-valley", "neutral-stage", "interactive");
    const runtime = host.runtime,
      world = host.world,
      actor = "bunny-instance";
    if (!runtime || !world) throw new Error("World runtime did not prepare");
    runtime.setFocusCharacter(actor);
    runtime.setRootMotionPolicy(actor, "visual");
    world.setInterest({ id: "winter-trail", position: [-16, 0, 8], visualRadius: 90, collisionRadius: 64 });
    expect((await world.prepare()).ready).toBe(true);
    await host.moveCharacter(actor, WINTER_HOME);
    const game = new WinterValleyGame();
    let previousMoving = false;
    for (const destination of [...WINTER_MARKERS.map((marker) => marker.position), WINTER_HOME]) {
      for (let step = 0; step < 1500; step++) {
        const foot = host.characterPosition(actor),
          body = runtime.bodyState(actor);
        const dx = destination[0] - foot[0],
          dz = destination[2] - foot[2],
          distance = Math.hypot(dx, dz);
        const moving = distance > 1.6;
        game.input({ move: moving ? [dx / distance, dz / distance] : [0, 0], interact: !moving });
        const move = game.movement(),
          ground = world.queryGround(foot[0] + move[0], foot[2] + move[1]);
        if (ground.status !== "ready") {
          await world.prepare({ scope: "collision" });
          continue;
        }
        if (moving !== previousMoving) {
          runtime.playMotion(actor, moving ? "hop" : "idle", 0.18);
          previousMoving = moving;
        }
        if (moving) runtime.setFacing(actor, Math.atan2(dx, dz));
        runtime.setTarget(actor, [
          foot[0] + move[0],
          ground.height + body.position[1] - foot[1] + 0.025,
          foot[2] + move[1],
        ] as Vec3);
        if (host.advance(WINTER_STEP)) game.step(host.characterPosition(actor));
        if (world.metrics.pending) await world.prepare({ scope: "collision" });
        const marker = WINTER_MARKERS.find((candidate) => candidate.position === destination);
        if (marker ? game.inspect().restored.includes(marker.id) : game.inspect().phase === "won") break;
        if (step === 1499)
          throw new Error(`Cannot reach ${destination}: ${JSON.stringify(host.characterPosition(actor))}`);
      }
    }
    expect(game.inspect().phase).toBe("won");
    expect(game.inspect().seconds).toBeLessThan(150);
    const save = host.saveRuntime();
    await host.resetRuntime();
    await host.loadRuntime(save);
    expect(Math.hypot(...host.characterPosition(actor).filter((_, index) => index !== 1))).toBeLessThan(3.5);
  } finally {
    host.dispose();
  }
}, 30000);
