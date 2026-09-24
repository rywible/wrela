import { mkdir, realpath } from "node:fs/promises";
import { join, resolve } from "node:path";
import { createInterface } from "node:readline";
import {
  authoringJobSchema,
  authoringStudySchema,
  heroStudySchema,
  iterationRequestSchema,
} from "@wrela/authoring";
import { z } from "zod";
import { runAuthoring } from "./author";
import { withAuthoringReviewSession } from "./authoring-review-session";
import { toolkitRequestSchema } from "./authoring-toolkit-store";

const invocationSchema = z.strictObject({
  workspace: z.string().min(1),
  work: z.string().min(1),
  action: z.enum(["context", "study", "job", "hero", "iterate", "toolkit", "finish"]),
  input: z.record(z.string(), z.unknown()),
});
export type AgentInvocation = z.input<typeof invocationSchema>;
const dispatchSchema = z.strictObject({
  version: z.literal(1),
  workspace: z.string(),
  work: z.string(),
  key: z.string(),
  brief: z.string(),
  images: z.array(z.string()),
  candidates: z.array(z.strictObject({ proposal: z.string(), passed: z.boolean() })),
});
/** Dispatch is ordinary retained data. Publication still verifies current source and review identities. */
export async function finishDispatchedJob(path: string, proposal: string, benchmarkRequest?: string) {
  const file = Bun.file(path);
  if (file.size > 1024 * 1024) throw Error("Dispatch exceeds 1 MiB");
  const dispatch = dispatchSchema.parse(await file.json());
  if (!dispatch.candidates.some((c) => c.proposal === proposal && c.passed))
    throw Error("Select a passing reviewed dispatch candidate");
  return invokeAuthoringAgent({
    workspace: dispatch.workspace,
    work: dispatch.work,
    action: "finish",
    input: { expectedKey: dispatch.key, proposal, ...(benchmarkRequest ? { benchmarkRequest } : {}) },
  });
}
/** The same invocation backs CLI hosts and a native MCP tool with inline image content. */
export async function invokeAuthoringAgent(raw: AgentInvocation) {
  const input = invocationSchema.parse(raw),
    workspace = resolve(input.workspace);
  const dir = join(workspace, ".wrela", "agent-requests");
  await mkdir(dir, { recursive: true });
  let argument: string;
  if (input.action === "context") {
    if (typeof input.input.target !== "string") throw Error("Context requires a target ID");
    argument = input.input.target;
  } else {
    if (input.action === "study") authoringStudySchema.parse(input.input);
    if (input.action === "job") authoringJobSchema.parse(input.input);
    if (input.action === "hero") heroStudySchema.parse(input.input);
    if (input.action === "iterate") iterationRequestSchema.parse(input.input);
    if (input.action === "toolkit") toolkitRequestSchema.parse(input.input);
    argument = join(dir, `${crypto.randomUUID()}.json`);
    await Bun.write(argument, JSON.stringify(input.input));
  }
  const response = (await runAuthoring([workspace, "work", input.action, input.work, argument])) as {
    work: { gallery?: string; [key: string]: unknown };
  };
  if (input.action === "job" || input.action === "hero") {
    const job = response.work as unknown as {
      key: string;
      quality: { direction: string };
      candidates: { proposal: string; passed: boolean }[];
      gallery: string;
    };
    const dispatch = dispatchSchema.parse({
      version: 1,
      workspace,
      work: input.work,
      key: job.key,
      brief: job.quality.direction,
      images: [job.gallery],
      candidates: job.candidates.map(({ proposal, passed }) => ({ proposal, passed })),
    });
    const path = join(dir, `dispatch-${crypto.randomUUID()}.json`);
    await Bun.write(path, JSON.stringify(dispatch, null, 2));
    response.work.dispatch = path;
  }
  const previews =
    input.action === "toolkit" && Array.isArray(response.work.entries)
      ? response.work.entries.flatMap((e: { previews?: string[] }) => e.previews ?? []).slice(0, 4)
      : [];
  return { result: response.work, images: response.work.gallery ? [response.work.gallery] : previews };
}
const toolEnvelope = {
  type: "object",
  properties: { workspace: { type: "string" }, work: { type: "string" } },
  required: ["workspace", "work"],
  additionalProperties: false,
};
export const authoringAgentTools = [
  ...[
    [
      "hero",
      heroStudySchema,
      "Construct from-scratch hero alternatives from semantic components: hollow shells, rings, rods, beams and plates. Returns matched beauty/clay/grazing images; choose or iterate before publishing.",
    ],
    [
      "iterate",
      iterationRequestSchema,
      "Branch an existing proposal, make a targeted repair or combine materials, and verify pinned decisions. Supports diagnostic and motion views; returns images inline.",
    ],
    [
      "toolkit",
      toolkitRequestSchema,
      "Search versioned reusable capabilities, record gaps and engineering cost, retain creation recipes, and qualify cross-family transfer using source-bound reviews.",
    ],
  ].map(([name, schema, description]) => ({
    name: `wrela_${name}`,
    description,
    inputSchema: {
      ...toolEnvelope,
      properties: { ...toolEnvelope.properties, request: z.toJSONSchema(schema as z.ZodType) },
      required: [...toolEnvelope.required, "request"],
    },
  })),
  {
    name: "wrela_job",
    description:
      "Resolve the task, adapt a curated domain target, generate bounded alternatives, verify source and return baseline/candidate images plus a ready finish request. One call; no discovery required. Conditions-based recommendation is not art acceptance.",
    inputSchema: {
      ...toolEnvelope,
      properties: { ...toolEnvelope.properties, request: z.toJSONSchema(authoringJobSchema) },
      required: [...toolEnvelope.required, "request"],
    },
  },
  {
    name: "wrela_context",
    description:
      "Retrieve task-ready domain controls, units, bounds, examples, quality criteria and current work key.",
    inputSchema: {
      ...toolEnvelope,
      properties: { ...toolEnvelope.properties, target: { type: "string" } },
      required: [...toolEnvelope.required, "target"],
    },
  },
  {
    name: "wrela_study",
    description:
      "Generate and review up to four semantic alternatives in one call. Returns a comparison image inline and passing proposal IDs. Does not publish or award artistic acceptance.",
    inputSchema: {
      ...toolEnvelope,
      properties: { ...toolEnvelope.properties, request: z.toJSONSchema(authoringStudySchema) },
      required: [...toolEnvelope.required, "request"],
    },
  },
  {
    name: "wrela_finish",
    description:
      "Publish a selected reviewed proposal and export portable evidence. Optional benchmarkRequest completes atomic benchmark submission.",
    inputSchema: {
      ...toolEnvelope,
      properties: {
        ...toolEnvelope.properties,
        request: {
          type: "object",
          properties: {
            expectedKey: { type: "string" },
            proposal: { type: "string" },
            benchmarkRequest: { type: "string" },
            requireArtReview: { type: "boolean" },
          },
          required: ["expectedKey", "proposal"],
          additionalProperties: false,
        },
      },
      required: [...toolEnvelope.required, "request"],
    },
  },
];
export async function callAuthoringAgentTool(
  name: string,
  args: { workspace: string; work: string; target?: string; request?: Record<string, unknown> },
) {
  if (!authoringAgentTools.some((t) => t.name === name)) throw Error("Unknown authoring tool");
  const response = await invokeAuthoringAgent({
    workspace: args.workspace,
    work: args.work,
    action: name.slice(6) as AgentInvocation["action"],
    input: name === "wrela_context" ? { target: args.target } : (args.request ?? {}),
  });
  const content: ({ type: "text"; text: string } | { type: "image"; data: string; mimeType: string })[] = [
    { type: "text", text: JSON.stringify(response.result) },
  ];
  const root = response.images.length
    ? await realpath(join(resolve(args.workspace), ".wrela", "work", "evidence"))
    : "";
  for (const path of response.images) {
    if (!(await realpath(path)).startsWith(`${root}/`)) throw Error("Tool image escaped retained evidence");
    const file = Bun.file(path);
    if (file.size > 8 * 1024 * 1024) throw Error("Inline review image exceeds 8 MiB");
    content.push({
      type: "image",
      mimeType: "image/png",
      data: Buffer.from(await file.arrayBuffer()).toString("base64"),
    });
  }
  return { content };
}
export async function serveAuthoringAgent() {
  const lines = createInterface({ input: process.stdin });
  let lifetime: Promise<void> | undefined,
    ready: Promise<void> | undefined,
    release: (() => void) | undefined;
  const ensureWarm = async () => {
    if (!ready) {
      let ok!: () => void, fail!: (error: unknown) => void;
      ready = new Promise<void>((resolve, reject) => {
        ok = resolve;
        fail = reject;
      });
      const stopped = new Promise<void>((resolve) => {
        release = resolve;
      });
      lifetime = withAuthoringReviewSession(async () => {
        ok();
        await stopped;
      }).catch((error) => {
        fail(error);
      });
    }
    await ready;
  };
  try {
    for await (const line of lines) {
      let message: {
        id?: string | number;
        method: string;
        params?: { name: string; arguments: Parameters<typeof callAuthoringAgentTool>[1] };
      };
      try {
        message = JSON.parse(line);
      } catch {
        process.stdout.write(
          JSON.stringify({ jsonrpc: "2.0", id: null, error: { code: -32700, message: "Invalid JSON" } }) +
            "\n",
        );
        continue;
      }
      if (message.id === undefined) continue;
      try {
        let result: unknown;
        if (message.method === "initialize")
          result = {
            protocolVersion: "2024-11-05",
            capabilities: { tools: {} },
            serverInfo: { name: "wrela-authoring", version: "1.0.0" },
          };
        else if (message.method === "ping") result = {};
        else if (message.method === "tools/list") result = { tools: authoringAgentTools };
        else if (message.method === "tools/call" && message.params) {
          try {
            if (["wrela_job", "wrela_study", "wrela_hero", "wrela_iterate"].includes(message.params.name))
              await ensureWarm();
            result = await callAuthoringAgentTool(message.params.name, message.params.arguments);
          } catch (error) {
            result = { isError: true, content: [{ type: "text", text: String(error) }] };
          }
        } else throw Error("Unsupported method");
        process.stdout.write(JSON.stringify({ jsonrpc: "2.0", id: message.id, result }) + "\n");
      } catch (error) {
        process.stdout.write(
          JSON.stringify({
            jsonrpc: "2.0",
            id: message.id,
            error: { code: -32601, message: String(error) },
          }) + "\n",
        );
      }
    }
  } finally {
    release?.();
    await lifetime;
  }
}
if (import.meta.main) {
  if (process.argv[2] === "--mcp") await serveAuthoringAgent();
  else
    try {
      console.log(
        JSON.stringify(
          process.argv[2] === "--finish-job"
            ? await finishDispatchedJob(process.argv[3], process.argv[4], process.argv[5])
            : await invokeAuthoringAgent(await Bun.file(process.argv[2]).json()),
        ),
      );
    } catch (error) {
      console.error(String(error));
      process.exitCode = 1;
    }
}
