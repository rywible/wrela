import { type CharacterDefinition, type CreatureDefinition, idSchema } from "@wrela/model";

import { z } from "zod";
import type { EditBatch } from "./commands";
import { applyCreatureOperation, type CreaturePoint, requireCreature } from "./creature";
import type { CreatureOperation } from "./creature-commands";

const axis = z.number().int().min(0).max(2);
const finite = z.number().finite();
export const creatureSolveRequestSchema = z.strictObject({
  id: idSchema,
  target: idSchema,
  expectedRevision: z.number().int().min(0),
  controls: z
    .array(
      z.discriminatedUnion("kind", [
        z.strictObject({
          kind: z.literal("regionScale"),
          region: idSchema,
          axis,
          minimum: finite.min(0.1).max(10),
          maximum: finite.min(0.1).max(10),
        }),
        z.strictObject({
          kind: z.literal("contactTarget"),
          contact: idSchema,
          axis,
          minimum: finite.min(-10000),
          maximum: finite.max(10000),
        }),
      ]),
    )
    .min(1)
    .max(16),
  objectives: z
    .array(
      z.discriminatedUnion("kind", [
        z.strictObject({
          kind: z.literal("regionExtent"),
          region: idSchema,
          axis,
          value: finite.positive(),
          tolerance: finite.positive(),
          weight: finite.positive().max(1000).default(1),
        }),
        z.strictObject({
          kind: z.literal("contactTarget"),
          contact: idSchema,
          axis,
          value: finite,
          tolerance: finite.positive(),
          weight: finite.positive().max(1000).default(1),
        }),
      ]),
    )
    .min(1)
    .max(32),
  preserve: z.array(idSchema).max(128).default([]),
  budget: z
    .strictObject({
      evaluations: z.number().int().min(4).max(1024).default(128),
      iterations: z.number().int().min(1).max(64).default(16),
    })
    .default({ evaluations: 128, iterations: 16 }),
  actor: z.string().min(1).max(120).optional(),
  intent: z.string().min(1).max(1000).optional(),
});
export type CreatureSolveRequest = z.input<typeof creatureSolveRequestSchema>;
/** Bounded finite-difference coordinate Gauss–Newton over editable source. This
 * solver does not evaluate visual quality or claim to measure runtime foot slip. */
export type CreatureSolveProgress = {
  phase: "initial" | "probe" | "line-search";
  evaluations: number;
  iterations: number;
  bestCost: number | null;
};
/** Each yielded checkpoint precedes one bounded source evaluation. Async drivers
 * return control to the event loop here; the synchronous API consumes the same iterator. */
export function* solveCreatureEditSteps(
  document: CharacterDefinition,
  input: CreatureSolveRequest,
  signal?: AbortSignal,
) {
  const request = creatureSolveRequestSchema.parse(input),
    character = requireCreature(document);
  if (request.target !== character.id) throw Error("Solver target does not match the provided character");
  const controls = request.controls;
  if (
    new Set(
      controls.map((v) =>
        JSON.stringify(v.kind === "regionScale" ? [v.kind, v.region, v.axis] : [v.kind, v.contact, v.axis]),
      ),
    ).size !== controls.length
  )
    throw Error("Solver controls must have unique source coordinates");
  for (const control of controls)
    if (control.minimum > control.maximum) throw Error("Solver control bounds are inverted");
  const initial = controls.map((control) => {
    if (control.kind === "regionScale") {
      if (!character.creature.regions.some((r) => r.id === control.region))
        throw Error(`Unknown region ${control.region}`);
      if (request.preserve.includes(control.region))
        throw Error(`Hard preservation constraint conflicts with solver control ${control.region}`);
      if (control.minimum > 1 || control.maximum < 1)
        throw Error("Region scale bounds must include unchanged source (1)");
      return 1;
    }
    const contact = character.creature.contacts.find((v) => v.id === control.contact);
    if (!contact) throw Error(`Unknown contact ${control.contact}`);
    if (
      character.creature.regions.some(
        (r) => request.preserve.includes(r.id) && r.jointIds.includes(contact.joint),
      )
    )
      throw Error(`Hard preservation constraint protects contact ${contact.id}`);
    const value = contact.target[control.axis];
    if (value < control.minimum || value > control.maximum)
      throw Error("Contact bounds must contain the current value");
    return value;
  });
  const operationsFor = (parameters: number[]): CreatureOperation[] => {
    const scales = new Map<string, CreaturePoint>(),
      contacts = new Map<string, CreatureDefinition["contacts"][number]>();
    controls.forEach((control, i) => {
      if (control.kind === "regionScale") {
        const scale = scales.get(control.region) ?? [1, 1, 1];
        scale[control.axis] = parameters[i];
        scales.set(control.region, scale);
      } else {
        const sourceContact = character.creature.contacts.find((v) => v.id === control.contact);
        if (!sourceContact) throw Error("Solver contact disappeared");
        const value = contacts.get(control.contact) ?? structuredClone(sourceContact);
        value.target[control.axis] = parameters[i];
        contacts.set(control.contact, value);
      }
    });
    return [
      ...[...scales].map(
        ([region, scale]): CreatureOperation => ({
          kind: "creature.proportion",
          target: character.id,
          region,
          scale,
          preserve: request.preserve,
        }),
      ),
      ...[...contacts.values()].map(
        (value): CreatureOperation => ({ kind: "creature.contact", target: character.id, value }),
      ),
    ];
  };
  let evaluations = 0,
    iterations = 0;
  const evaluate = (parameters: number[]) => {
    if (signal?.aborted) throw Error("Creature solve cancelled");
    if (evaluations >= request.budget.evaluations) return undefined;
    evaluations++;
    const candidate = structuredClone(character);
    for (const operation of operationsFor(parameters)) applyCreatureOperation(candidate, operation);
    const residuals = request.objectives.map((objective) => {
      const actual =
        objective.kind === "regionExtent"
          ? candidate.creature.regions.find((r) => r.id === objective.region)?.extent[objective.axis]
          : candidate.creature.contacts.find((c) => c.id === objective.contact)?.target[objective.axis];
      if (actual === undefined) throw Error("Objective references missing source");
      return {
        kind: objective.kind,
        id: objective.kind === "regionExtent" ? objective.region : objective.contact,
        axis: objective.axis,
        actual,
        desired: objective.value,
        residual: actual - objective.value,
        tolerance: objective.tolerance,
        units: "metres",
        satisfied: Math.abs(actual - objective.value) <= objective.tolerance,
      };
    });
    const weighted = residuals.map((value, i) => value.residual * Math.sqrt(request.objectives[i].weight));
    return { residuals, weighted, cost: weighted.reduce((sum, v) => sum + v * v, 0) };
  };
  let parameters = [...initial];
  yield { phase: "initial", evaluations, iterations, bestCost: null } satisfies CreatureSolveProgress;
  const initialEvaluation = evaluate(parameters);
  if (!initialEvaluation) throw Error("Solver has no initial evaluation budget");
  let current = initialEvaluation;
  const unidentifiable = new Set<number>(),
    failures = new Set<string>();
  for (
    ;
    iterations < request.budget.iterations &&
    evaluations < request.budget.evaluations &&
    !current.residuals.every((v) => v.satisfied);
  ) {
    iterations++;
    let improved = false;
    for (let index = 0; index < controls.length && evaluations < request.budget.evaluations; index++) {
      const control = controls[index],
        delta = Math.max(1e-5, Math.abs(parameters[index]) * 1e-4);
      const probe = [...parameters];
      probe[index] = Math.min(control.maximum, parameters[index] + delta);
      if (probe[index] === parameters[index])
        probe[index] = Math.max(control.minimum, parameters[index] - delta);
      if (probe[index] === parameters[index]) {
        unidentifiable.add(index);
        continue;
      }
      let response: ReturnType<typeof evaluate>;
      try {
        yield {
          phase: "probe",
          evaluations,
          iterations,
          bestCost: current.cost,
        } satisfies CreatureSolveProgress;
        response = evaluate(probe);
      } catch (error) {
        failures.add(error instanceof Error ? error.message : String(error));
        continue;
      }
      if (!response) break;
      const derivatives = response.weighted.map(
        (value, i) => (value - current.weighted[i]) / (probe[index] - parameters[index]),
      );
      const norm = derivatives.reduce((sum, value) => sum + value * value, 0);
      if (norm < 1e-16) {
        unidentifiable.add(index);
        continue;
      }
      const step = -derivatives.reduce((sum, value, i) => sum + value * current.weighted[i], 0) / norm;
      for (const fraction of [1, 0.5, 0.25, 0.125]) {
        const next = [...parameters];
        next[index] = Math.max(
          control.minimum,
          Math.min(control.maximum, parameters[index] + step * fraction),
        );
        let result: ReturnType<typeof evaluate>;
        try {
          yield {
            phase: "line-search",
            evaluations,
            iterations,
            bestCost: current.cost,
          } satisfies CreatureSolveProgress;
          result = evaluate(next);
        } catch (error) {
          failures.add(error instanceof Error ? error.message : String(error));
          continue;
        }
        if (!result) break;
        if (result.cost < current.cost - 1e-16) {
          parameters = next;
          current = result;
          improved = true;
          break;
        }
      }
    }
    if (!improved) break;
  }
  if (signal?.aborted) throw Error("Creature solve cancelled");
  const batch: EditBatch = {
    expectedRevision: request.expectedRevision,
    actor: request.actor,
    intent: request.intent ?? "Bounded creature source constraint solve",
    label: "Creature constraint candidate",
    operations: operationsFor(parameters),
  };
  const converged = current.residuals.every((v) => v.satisfied);
  return {
    id: request.id,
    batch,
    status: converged
      ? "converged"
      : evaluations >= request.budget.evaluations
        ? "budget-exhausted"
        : "unsatisfied",
    evaluations,
    iterations,
    parameters: controls.map((control, i) => ({
      ...control,
      value: parameters[i],
      atBound: parameters[i] === control.minimum || parameters[i] === control.maximum,
    })),
    residuals: current.residuals,
    unidentifiable: [...unidentifiable],
    rejectedConstraints: [...failures],
    scope: "Editable source metrics; runtime contact slip and visual quality require scenario evaluation",
  };
}

export type CreatureSolveResult =
  ReturnType<typeof solveCreatureEditSteps> extends Generator<unknown, infer Result, unknown>
    ? Result
    : never;
/** Backward-compatible synchronous entry point; prefer jobs for interactive use. */
export function solveCreatureEdits(
  document: CharacterDefinition,
  input: CreatureSolveRequest,
  signal?: AbortSignal,
) {
  const steps = solveCreatureEditSteps(document, input, signal);
  let step = steps.next();
  while (!step.done) step = steps.next();
  return step.value;
}
