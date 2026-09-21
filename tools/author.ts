import { AuthoringSession } from "@wrela/authoring";
import { WorkspaceBridge } from "./bridge";

/** Local, inspectable authoring over the same transactions used by Studio. */
export async function runAuthoring(args: string[]) {
  const [directory, command = "inspect", argument] = args;
  if (!directory)
    throw new Error(
      "Usage: bun run author <workspace> inspect [id] | discover | preview <proposal.json> | apply <proposal.json>",
    );
  const bridge = new WorkspaceBridge(directory);
  await bridge.initialize();
  const current = await bridge.read();
  if (!current) throw new Error("Workspace has no published project. Save a project from Studio first.");
  const session = new AuthoringSession(current.project);
  if (command === "discover")
    return {
      ...session.discover(),
      workspaceKey: current.key,
      transport:
        "A proposal contains {workspaceKey,batch}. Revisions describe this inspected snapshot; workspaceKey protects publication against all other writers.",
    };
  if (command === "inspect") return { workspaceKey: current.key, result: session.inspect(argument) };
  if (command !== "apply" && command !== "preview") throw new Error(`Unknown authoring command: ${command}`);
  if (!argument) throw new Error("A proposal JSON path is required");
  const proposal: unknown = JSON.parse(await Bun.file(argument).text());
  if (!proposal || typeof proposal !== "object" || !("workspaceKey" in proposal) || !("batch" in proposal))
    throw new Error("Proposal must contain workspaceKey and batch");
  if (proposal.workspaceKey !== current.key)
    throw new Error("Workspace changed; inspect it and revise the proposal");
  const result = session.apply(proposal.batch as Parameters<AuthoringSession["apply"]>[0]);
  const changes = result.changed.map((id) => ({
    id,
    before: current.project.documents.find((document) => document.id === id),
    after: session.inspect(id),
  }));
  if (command === "preview") return { published: false, workspaceKey: current.key, ...result, changes };
  const published = await bridge.save(session.getSnapshot().project, current.key);
  return { published: true, workspaceKey: published.key, ...result, changes };
}
if (import.meta.main) {
  try {
    console.log(JSON.stringify(await runAuthoring(process.argv.slice(2)), null, 2));
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 1;
  }
}
