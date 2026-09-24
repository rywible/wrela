import { join } from "node:path";
import { createCreatureFixture, referenceProject } from "@wrela/examples";
import { contentKey, type Project, parseProject } from "@wrela/model";
import { type BrowserView, waitFor, withBrowser } from "./browser";
import { buildStudio } from "./build";

function ensure(condition: unknown, message: string): asserts condition {
  if (!condition) throw Error(message);
}
async function reveal(view: BrowserView, expression: string) {
  await view.evaluate(
    `(()=>{const e=${expression};if(!e)throw Error('Missing verification control');for(let p=e.parentElement;p;p=p.parentElement)if(p.tagName==='DETAILS')p.open=true;e.setAttribute('data-verification-control','active');e.scrollIntoView({block:'center'});})()`,
  );
}
async function clearMarker(view: BrowserView) {
  await view.evaluate(
    "document.querySelectorAll('[data-verification-control]').forEach(e=>e.removeAttribute('data-verification-control'))",
  );
}
async function button(view: BrowserView, name: string) {
  await clearMarker(view);
  await reveal(
    view,
    `[...document.querySelectorAll('button')].find(e=>e.textContent.trim()===${JSON.stringify(name)})`,
  );
  await view.click('[data-verification-control="active"]');
}
async function input(view: BrowserView, name: string, value: string, scope = ".inspector-scroll") {
  await clearMarker(view);
  await reveal(
    view,
    `(()=>{const scope=document.querySelector(${JSON.stringify(scope)});return [...scope.querySelectorAll('input')].find(e=>e.getAttribute('aria-label')===${JSON.stringify(name)})??[...scope.querySelectorAll('label')].find(e=>e.textContent.trim()===${JSON.stringify(name)})?.querySelector('input');})()`,
  );
  await view.click('[data-verification-control="active"]');
  await view.evaluate("document.querySelector('[data-verification-control=active]').select()");
  await view.type(value);
  await view.press("Tab");
}
async function ready(view: BrowserView, subject?: string) {
  await view.evaluate(
    `wrela.preview.prepare(${JSON.stringify({ ...(subject ? { subject, stage: "studio", stageId: "neutral-stage" } : {}), timeoutMs: 30000 })})`,
  );
  ensure(
    await view.evaluate("wrela.preview.inspect().status==='current'"),
    "Mounted Studio preview did not become current",
  );
}

/** Tests the installed React Studio and trusted input handlers. Scene framebuffer is bounded; source remains real. */
export async function verifyDomainStudio() {
  const project = referenceProject();
  const creature = createCreatureFixture("ash-warden");
  project.documents.push(...creature.project.documents);
  for (const document of project.documents)
    if (document.kind === "world") {
      document.populations = [];
      document.instances = [];
    }
  let saved = parseProject(project);
  return withBrowser(
    async (view, output, errors) => {
      const directory = await buildStudio(join(output, "studio-build"));
      const checks: { category: string; document: string; property: string; value: unknown }[] = [];
      const report = {
        checks,
        visualApproval: "not-reviewed",
        browserErrors: errors,
        adapter: "",
        performanceScrub: false,
        bounceLighting: false,
      };
      const persist = () =>
        Bun.write(join(output, "domain-studio-verification.json"), JSON.stringify(report, null, 2));
      const server = Bun.serve({
        hostname: "127.0.0.1",
        port: 0,
        async fetch(request) {
          const path = new URL(request.url).pathname;
          if (path === "/favicon.ico") return new Response(null, { status: 204 });
          if (path === "/bridge/session") return Response.json({ available: true });
          if (path === "/bridge/project") {
            if (request.method === "PUT") {
              const body = (await request.json()) as { project: Project; expectedKey: string };
              if (body.expectedKey !== contentKey(saved))
                return Response.json({ error: "Fixture save conflict" }, { status: 409 });
              saved = parseProject(body.project);
            }
            return Response.json({ project: saved, key: contentKey(saved) });
          }
          const file = Bun.file(join(directory, path === "/" ? "index.html" : path));
          if (!(await file.exists())) return new Response("Not found", { status: 404 });
          if (path === "/" || path === "/index.html")
            return new Response(
              (await file.text()).replace(
                "</head>",
                "<style>.viewport{width:320px!important;height:180px!important;min-height:180px!important;max-height:180px!important;align-self:center;justify-self:center}.viewport>canvas{width:320px!important;height:180px!important}</style></head>",
              ),
              { headers: { "Content-Type": "text/html" } },
            );
          return new Response(file);
        },
      });
      try {
        await view.navigate(server.url.toString());
        await waitFor(
          view,
          'window.wrela && ["current","failed"].includes(wrela.preview.inspect().status)',
          60000,
        );
        const cases = [
          {
            category: "materials",
            id: "stone",
            enable: "Enable surface authoring",
            label: "wetness",
            value: "0.7",
            property: "appearance.wetness",
          },
          {
            category: "material-relief",
            id: "stone",
            enable: "relief",
            label: "Relief variation seed",
            value: "37",
            property: "appearance.relief.seed",
          },
          {
            category: "assemblies",
            id: "river-stone",
            enable: "Create assembly",
            label: "Snap grid",
            value: "0.2",
            property: "assembly.grid",
          },
          {
            category: "vegetation",
            id: "alpine-pine",
            enable: "Enable botanical growth",
            label: "Maturity",
            value: "0.4",
            property: "botanical.age",
          },
          {
            category: "geology",
            id: "valley-terrain",
            enable: "Enable geology authoring",
            label: "Erosion strength",
            value: "0.3",
            property: "geology.erosion.strength",
          },
          {
            category: "world",
            id: "winter-valley",
            enable: "Add path",
            label: "width",
            value: "6",
            property: "composition.paths[0].width",
          },
          {
            category: "environment",
            id: "winter-sky",
            enable: "Create environment sequence",
            label: "Duration (seconds)",
            value: "90",
            property: "sequence.duration",
          },
          {
            category: "lighting",
            id: "daylight",
            enable: "Add lighting zone",
            label: "Zone radius (m)",
            value: "8",
            property: "zones[0].radius",
          },
          {
            category: "water",
            id: "coastal-waves",
            enable: "Add water current",
            label: "X",
            value: "2",
            property: "flow.velocity[0]",
            scope: 'section[aria-label="Environment authoring"]',
          },
          {
            category: "stage",
            id: "neutral-stage",
            enable: "Add exposure and light tint",
            label: "Exposure compensation (stops)",
            value: "1",
            property: "grade.exposureCompensation",
          },
          {
            category: "performance",
            id: "polar-bunny",
            enable: "Add event at cursor",
            label: "Duration (seconds)",
            value: "3",
            property: "motions[0].duration",
            scope: ".performance-authoring",
          },
        ];
        for (const item of cases) {
          console.log(`Domain Studio: checking ${item.category}`);
          await ready(view, item.id);
          if (item.enable === "relief") {
            await clearMarker(view);
            await reveal(
              view,
              "[...document.querySelectorAll('label')].find(e=>e.textContent.trim().startsWith('Relief structure'))?.querySelector('select')",
            );
            // Native popup selection is not exposed by Bun.WebView. Exercise the
            // mounted React change handler; subsequent edits/buttons use native input.
            await view.evaluate(
              "(()=>{const control=document.querySelector('[data-verification-control=active]');Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype,'value').set.call(control,'stone');control.dispatchEvent(new Event('change',{bubbles:true}));})()",
            );
            await waitFor(view, 'wrela.inspect("stone").appearance?.relief?.kind==="stone"', 5000);
          } else await button(view, item.enable);
          await ready(view);
          const before = await view.evaluate(`wrela.inspect(${JSON.stringify(item.id)}).${item.property}`);
          await input(view, item.label, item.value, item.scope);
          await ready(view);
          const after = await view.evaluate(`wrela.inspect(${JSON.stringify(item.id)}).${item.property}`);
          ensure(
            after === Number(item.value),
            `${item.category}: edit did not reach authored source (${String(after)})`,
          );
          await view.click('button[aria-label="Undo"]');
          await ready(view);
          ensure(
            await view.evaluate(
              `wrela.inspect(${JSON.stringify(item.id)}).${item.property}===${JSON.stringify(before)}`,
            ),
            `${item.category}: undo failed`,
          );
          await view.click('button[aria-label="Redo"]');
          await ready(view);
          ensure(
            await view.evaluate(
              `wrela.inspect(${JSON.stringify(item.id)}).${item.property}===${Number(item.value)}`,
            ),
            `${item.category}: redo failed`,
          );
          if (item.category === "performance") {
            const revision = await view.evaluate<number>("wrela.inspect().revision");
            await view.scrollTo('input[aria-label="Performance time"]');
            await view.click('input[aria-label="Performance time"]');
            await view.press("Home");
            await view.press("ArrowRight");
            await waitFor(
              view,
              `Math.abs(wrela.preview.inspect().time-Number(document.querySelector('input[aria-label="Performance time"]').value))<0.02 && wrela.preview.inspect().status==="current"`,
            );
            ensure(
              await view.evaluate(`wrela.inspect().revision===${revision}`),
              "Scrubbing mutated authored source",
            );
            ensure(
              await view.evaluate("wrela.preview.inspect().time>0"),
              "Scrubbing failed to advance selected clip",
            );
            report.performanceScrub = true;
          }
          await Bun.write(join(output, `domain-${item.category}.png`), await view.screenshot());
          checks.push({ category: item.category, document: item.id, property: item.property, value: after });
          await persist();
        }
        console.log("Domain Studio: checking creatures");
        await ready(view, "ash-warden");
        await button(view, "Add landmark");
        await ready(view);
        const landmarkId = await view.evaluate<string>(
          'wrela.inspect("ash-warden").creature.landmarks.at(-1).id',
        );
        await clearMarker(view);
        await reveal(
          view,
          `[...document.querySelectorAll('.creature-coherence-panel details')].find(e=>e.querySelector('summary')?.textContent===${JSON.stringify(landmarkId)})?.querySelector('input[aria-label="Landmark position (m) x"]')`,
        );
        await view.click('[data-verification-control="active"]');
        await view.evaluate("document.querySelector('[data-verification-control=active]').select()");
        await view.type("0.25");
        await view.press("Tab");
        await ready(view);
        ensure(
          await view.evaluate('wrela.inspect("ash-warden").creature.landmarks.at(-1).position[0]===0.25'),
          "Creature landmark edit did not reach source",
        );
        await view.click('button[aria-label="Undo"]');
        await ready(view);
        ensure(
          await view.evaluate('wrela.inspect("ash-warden").creature.landmarks.at(-1).position[0]===0'),
          "Creature landmark undo failed",
        );
        await view.click('button[aria-label="Redo"]');
        await ready(view);
        checks.push({
          category: "creatures",
          document: "ash-warden",
          property: "creature.landmarks.at(-1).position[0]",
          value: 0.25,
        });
        await Bun.write(join(output, "domain-creatures.png"), await view.screenshot());
        console.log("Domain Studio: checking bounce lighting");
        await ready(view, "stone");
        const lightingRevision = await view.evaluate<number>("wrela.inspect().revision");
        await button(view, "Bounce lighting");
        await waitFor(
          view,
          '[...document.querySelectorAll("output")].some(e=>e.textContent.includes("Ready · one diffuse bounce"))',
          60000,
        );
        ensure(
          await view.evaluate(`wrela.inspect().revision===${lightingRevision}`),
          "Lighting preview mutated authored source",
        );
        await Bun.write(join(output, "domain-bounce-lighting.png"), await view.screenshot());
        await button(view, "Bounce lighting");
        await waitFor(
          view,
          '[...document.querySelectorAll("button")].find(e=>e.textContent.trim()==="Bounce lighting")?.getAttribute("aria-pressed")==="false"',
        );
        report.bounceLighting = true;
        await view.evaluate("wrela.save()");
        const serialized = await view.evaluate<string>("wrela.project()");
        await view.evaluate("wrela.reopen()");
        await ready(view);
        ensure(
          await view.evaluate(`wrela.project()===${JSON.stringify(serialized)}`),
          "Saved/reopened source differs",
        );
        ensure(checks.length === 12, "Not every requested category was exercised");
        parseProject(JSON.parse(serialized));
        report.adapter = await view.evaluate<string>("wrela.measure().frame.adapter");
        ensure(
          !/swiftshader|llvmpipe|software/i.test(report.adapter),
          "Software rendering is not hardware evidence",
        );
        await persist();
        ensure(!errors.length, errors.join("\n"));
        console.log(
          JSON.stringify({ output, categories: checks.map((check) => check.category), saveReopen: true }),
        );
      } catch (error) {
        await Bun.write(join(output, "domain-failure.png"), await view.screenshot()).catch(() => {});
        await Bun.write(
          join(output, "domain-failure.json"),
          JSON.stringify(
            {
              error: String(error),
              state: await view.evaluate("window.wrela?.preview.inspect()").catch(() => null),
              alerts: await view
                .evaluate("[...document.querySelectorAll('[role=alert]')].map(e=>e.textContent)")
                .catch(() => []),
            },
            null,
            2,
          ),
        );
        console.log(`Domain Studio evidence: ${output}`);
        throw error;
      } finally {
        server.stop(true);
      }
    },
    1440,
    960,
  );
}
if (import.meta.main) await verifyDomainStudio();
