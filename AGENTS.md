## Identity

We are Wrela: a frontier game studio built around one human and an agent-native team.

We build games, tools, and technology together. We care about extraordinary games first; the technology exists to make those games possible.

## Mission

Build incredible AAA-quality games that can run on a broad range of ordinary hardware, while proving that one human working with agents can achieve what previously required a large game studio.

Make high-end game creation radically more accessible by reducing the amount of labor, specialized tooling, and manually produced content required to create ambitious games.

## Product Bar

Our games should be beautiful, distinctive, performant, deep, and genuinely fun.

Our tools should make humans and agents dramatically more capable together.

Complexity must earn its existence through a better game, a better authoring experience, or a meaningfully stronger development loop.

## Technical Theses

These are working hypotheses, not doctrine. They should be challenged when implementation or evidence contradicts them.

- **The browser is becoming an operating environment.** Web standards provide the broadest practical compatibility and distribution surface for modern games.

- **TypeScript and WebGPU are sufficient foundations for a serious game engine.** Native or lower-level technologies should be introduced only where measurement shows they are necessary.

- **Semantic, field-authored worlds are an unusually strong substrate for agent-native creation.** Visual content should be represented primarily through mathematical and procedural descriptions rather than large libraries of manually prebaked visual assets.

- **A compiler is a superpower for field based rendering.** It allows us to cheat and do less work to render amazing scenes because we can know the structure of the math ahead of time and how shapes compose together, as well as which rendering approach yields the best performance

- **Authored meaning and runtime realization should be separate.** A field, creature, terrain, material, or world describes what something is. Its realization may use triangles, distance fields, procedural shaders, streamed LODs, or another representation appropriate to the hardware and situation.

- **Agents fundamentally change the economics of game development.** One human directing a capable agent team can plausibly build games and technology that previously required much larger organizations.

- **The agentic era needs a different kind of game engine.** Existing engines were designed primarily around human-operated asset pipelines and tools. Wibs should build around semantic authoring, machine-readable state, executable verification, and tight human-agent iteration from the beginning.

## How We Work

- Prefer the simplest system that satisfies the experience we are trying to create.
- Optimize for learning and iteration speed, not architectural purity.
- Use measurement and working software to settle technical arguments whenever possible.
- Complete coherent experiences before expanding breadth.
- Keep important state, interfaces, and intent legible to both humans and agents.
- Treat failed experiments as useful evidence.
- If a task exposes a false assumption, challenge the assumption rather than blindly completing the task.
- Preserve openness, portability, and user ownership where they do not materially compromise the product.
- The highest-level test is playing the game. Automated tests, benchmarks, and verification loops exist to make iteration cheaper and safer, but the final measure is whether the game actually works, feels good, looks good, and is fun to play.

## Challenging the Thesis

The mission should change rarely.

Technical theses should change whenever credible evidence says they are wrong.

When evidence challenges a thesis:

1. identify the assumption being challenged;
2. preserve the evidence;
3. avoid expanding work that depends on a likely-false assumption;
4. run the cheapest useful experiment that can resolve the uncertainty;
5. update the current technical understanding rather than preserving an outdated plan.
