import { mkdir, open, rename, unlink } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import {
  appendCreatureObservation,
  assessCreatureObservation,
  type CreatureNotebook,
  creatureNotebookSchema,
} from "@wrela/authoring";
import { contentKey, idSchema } from "@wrela/model";

/** Persistent review evidence lives beside the project, so annotations cannot
 * invalidate the geometry revision they describe. Writes use a lock and source-key CAS. */
export async function runCreatureNotebook(args: string[]) {
  const [path, command = "inspect", argument] = args;
  if (!path)
    throw Error(
      "Usage: creature-notebook <book.json> init <character> | inspect | observe/assess <request.json>",
    );
  const file = resolve(path);
  if (command === "inspect") {
    const book = creatureNotebookSchema.parse(await Bun.file(file).json());
    return { key: contentKey(book), book };
  }
  await mkdir(dirname(file), { recursive: true });
  const lock = await open(`${file}.lock`, "wx");
  try {
    let book: CreatureNotebook;
    if (command === "init") {
      if (await Bun.file(file).exists()) throw Error("Notebook already exists");
      book = creatureNotebookSchema.parse({
        version: 1,
        target: idSchema.parse(argument),
        observations: [],
        assessments: [],
      });
    } else {
      if (!argument || !["observe", "assess"].includes(command))
        throw Error("Unknown notebook operation or missing request");
      const requestFile = Bun.file(argument);
      if (requestFile.size > 1024 * 1024) throw Error("Notebook request exceeds 1 MiB");
      const request = await requestFile.json(),
        prior = creatureNotebookSchema.parse(await Bun.file(file).json());
      if (request.expectedKey !== contentKey(prior)) throw Error("Notebook changed; inspect and retry");
      book =
        command === "observe"
          ? appendCreatureObservation(prior, request.observation)
          : assessCreatureObservation(prior, request.assessment);
    }
    const temp = `${file}.tmp`;
    await Bun.write(temp, JSON.stringify(book, null, 2));
    await rename(temp, file);
    return { key: contentKey(book), book };
  } finally {
    await lock.close();
    await unlink(`${file}.lock`);
  }
}
if (import.meta.main) console.log(JSON.stringify(await runCreatureNotebook(process.argv.slice(2)), null, 2));
