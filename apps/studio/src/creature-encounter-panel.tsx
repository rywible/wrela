import { useEffect } from "react";
import type { PreviewState, StudioController } from "./controller";

export function CreatureEncounterPanel({
  controller,
  encounter,
}: {
  controller: StudioController;
  encounter: NonNullable<PreviewState["encounter"]>;
}) {
  useEffect(() => {
    const keys = new Set<string>();
    const move = () => {
      if (!controller.getSnapshot().encounter) return;
      controller.creature.encounter.input({
        move: [
          Number(keys.has("KeyD") || keys.has("ArrowRight")) -
            Number(keys.has("KeyA") || keys.has("ArrowLeft")),
          Number(keys.has("KeyS") || keys.has("ArrowDown")) - Number(keys.has("KeyW") || keys.has("ArrowUp")),
        ],
      });
    };
    const down = (event: KeyboardEvent) => {
      if (
        event.target instanceof HTMLInputElement ||
        event.target instanceof HTMLTextAreaElement ||
        event.target instanceof HTMLSelectElement ||
        event.metaKey ||
        event.ctrlKey
      )
        return;
      if (
        ["KeyW", "KeyA", "KeyS", "KeyD", "ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight"].includes(
          event.code,
        )
      ) {
        event.preventDefault();
        keys.add(event.code);
        move();
      }
      if (
        !event.repeat &&
        controller.getSnapshot().encounter &&
        (event.code === "Space" || event.code === "KeyJ")
      ) {
        event.preventDefault();
        controller.creature.encounter.input(event.code === "Space" ? { dodge: true } : { strike: true });
      }
    };
    const up = (event: KeyboardEvent) => {
      keys.delete(event.code);
      move();
    };
    const blur = () => {
      keys.clear();
      move();
    };
    window.addEventListener("keydown", down);
    window.addEventListener("keyup", up);
    window.addEventListener("blur", blur);
    return () => {
      window.removeEventListener("keydown", down);
      window.removeEventListener("keyup", up);
      window.removeEventListener("blur", blur);
    };
  }, [controller]);
  const phase = {
    watching: "Watching",
    approach: "Closing the distance",
    telegraph: "Attack coming — dodge!",
    attack: "Committed strike",
    recovery: "Recovery — strike now",
    stagger: "Staggered",
    won: "Creature defeated",
    lost: "You fell",
  }[encounter.phase];
  const directions: { name: string; move: [number, number]; glyph: string }[] = [
    { name: "Move left", move: [-1, 0], glyph: "←" },
    { name: "Move forward", move: [0, -1], glyph: "↑" },
    { name: "Move back", move: [0, 1], glyph: "↓" },
    { name: "Move right", move: [1, 0], glyph: "→" },
  ];
  return (
    <section className="creature-encounter-hud" aria-label="Creature encounter">
      <div className="creature-encounter-heading">
        <h2>Creature encounter</h2>
        <button type="button" onClick={() => controller.creature.encounter.stop()}>
          End encounter
        </button>
      </div>
      <label>
        You <meter min={0} max={100} value={encounter.playerHealth} /> {encounter.playerHealth}
      </label>
      <label>
        Creature <meter min={0} max={96} value={encounter.creatureHealth} /> {encounter.creatureHealth}
      </label>
      <output className={encounter.telegraph ? "creature-attack-warning" : ""}>{phase}</output>
      <p className="hint">
        Move the blue marker with WASD or arrows. Space dodges; J strikes. Escape returns to authoring.
      </p>
      <div className="creature-encounter-controls">
        {directions.map((direction) => (
          <button
            type="button"
            key={direction.name}
            aria-label={direction.name}
            onPointerDown={(event) => {
              event.currentTarget.setPointerCapture(event.pointerId);
              controller.creature.encounter.input({ move: direction.move });
            }}
            onPointerUp={() => controller.creature.encounter.input({ move: [0, 0] })}
            onPointerCancel={() => controller.creature.encounter.input({ move: [0, 0] })}
            onLostPointerCapture={() => {
              if (controller.getSnapshot().encounter) controller.creature.encounter.input({ move: [0, 0] });
            }}
          >
            {direction.glyph}
          </button>
        ))}
        <button
          type="button"
          disabled={!encounter.dodgeReady || encounter.outcome !== "playing"}
          onClick={() => controller.creature.encounter.input({ dodge: true })}
        >
          Dodge
        </button>
        <button
          type="button"
          disabled={!encounter.strikeReady || encounter.outcome !== "playing"}
          onClick={() => controller.creature.encounter.input({ strike: true })}
        >
          Strike
        </button>
      </div>
      {encounter.events.at(-1) && (
        <small>Last action: {encounter.events.at(-1)?.kind.replaceAll("-", " ")}</small>
      )}
    </section>
  );
}
