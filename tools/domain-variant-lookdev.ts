import { mkdir } from "node:fs/promises";
import { join, resolve } from "node:path";
import { type AuthoringDomain, AuthoringSession } from "@wrela/authoring";
import { fixtureServer, waitFor, withBrowser } from "./browser";
import { createDomainVariantStudy, DOMAIN_VARIANT_DOMAINS } from "./domain-variant-studies";
import type { createDomainVariantLookdevFixture } from "./fixtures/domain-variant-lookdev";

const argument = (name: string, fallback: string) =>
  process.argv.find((value) => value.startsWith(`--${name}=`))?.slice(name.length + 3) ?? fallback;
const domains = argument("domain", DOMAIN_VARIANT_DOMAINS.join(",")).split(",") as AuthoringDomain[];
if (
  !domains.length ||
  new Set(domains).size !== domains.length ||
  domains.some((domain) => !DOMAIN_VARIANT_DOMAINS.includes(domain))
)
  throw Error(`Choose unique domains from ${DOMAIN_VARIANT_DOMAINS.join(", ")}`);
const size = process.argv.includes("--small") ? [640, 480] : [960, 720];
type Capture = Awaited<ReturnType<ReturnType<typeof createDomainVariantLookdevFixture>["capture"]>>;
const escapeHtml = (value: string) =>
  value.replace(
    /[&<>"']/g,
    (char) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[char] ?? char,
  );
await withBrowser(
  async (view, output, errors) => {
    const server = await fixtureServer(
      `import {createDomainVariantLookdevFixture} from ${JSON.stringify(resolve("tools/fixtures/domain-variant-lookdev.ts"))};window.fixture=createDomainVariantLookdevFixture();window.ready=true;`,
      output,
    );
    const reports: Omit<Capture, "frames" | "project">[] = [];
    const frames: {
      domain: string;
      variant: string;
      id: string;
      path: string;
      metadata: Omit<Capture["frames"][number], "image">;
    }[] = [];
    const save = async () => {
      await Bun.write(
        join(output, "domain-variant-review.json"),
        JSON.stringify(
          {
            domains,
            reports,
            frames,
            errors,
            status: "unreviewed",
            economics: { acceptedVariants: null, manualInterventions: null, agentTokenCost: null },
            scope:
              "Three predeclared source parameter variants per domain. Matched images and timings support manual review; source invariants and successful renders do not award AAA acceptance.",
          },
          null,
          2,
        ),
      );
      await Bun.write(
        join(output, "index.html"),
        `<!doctype html><meta charset="utf-8"><title>Authoring variants</title><style>body{margin:32px;background:#151816;color:#e8ece8;font:16px system-ui}h1{font-size:28px}section{margin:32px 0}article{display:inline-block;vertical-align:top;width:31%;margin:1%}img{width:100%;background:#222}a{color:#aac8a7}p{max-width:80ch}</style><h1>Matched authoring variants</h1><p>Visual acceptance is unreviewed. Each domain retains baseline and three ordinary source alternatives, constraints, source changes and measured request-to-frame latency. Token cost and manual intervention counts are unknown.</p><p><a href="domain-variant-review.json">Review manifest</a></p>${domains
          .map(
            (domain) =>
              `<section><h2>${escapeHtml(domain)}</h2>${frames
                .filter((frame) => frame.domain === domain)
                .map(
                  (frame) =>
                    `<article><a href="${escapeHtml(frame.path)}"><img src="${escapeHtml(frame.path)}"></a><p>${escapeHtml(frame.variant)} · ${escapeHtml(frame.id)}</p></article>`,
                )
                .join("")}</section>`,
          )
          .join("")}`,
      );
    };
    try {
      await view.navigate(String(server.url));
      await waitFor(view, "window.ready||window.failure", 60000);
      const failure = await view.evaluate("window.failure");
      if (failure) throw Error(String(failure));
      for (const domain of domains) {
        const study = createDomainVariantStudy(domain),
          directory = join(output, domain);
        await mkdir(directory, { recursive: true });
        const session = new AuthoringSession(study.lookdev.project),
          proposal = session.proposeDomainVariants(study.request);
        await Bun.write(join(directory, "baseline.json"), JSON.stringify(study.lookdev.project, null, 2));
        await Bun.write(join(directory, "request.json"), JSON.stringify(study.request, null, 2));
        await Bun.write(join(directory, "source-review.json"), JSON.stringify(proposal, null, 2));
        // Save ordinary source products before GPU work, so a renderer failure retains the reviewable proposal.
        for (const variant of proposal.variants) {
          const isolated = new AuthoringSession(study.lookdev.project);
          isolated.apply(variant.candidate.batch);
          await Bun.write(join(directory, `${variant.id}.json`), isolated.export());
        }
        for (let index = -1; index < study.request.variants.length; index++) {
          const result = await view.evaluate<Capture>(`fixture.capture(${JSON.stringify(study)},${index})`);
          const { frames: captured, project: _project, ...report } = result;
          reports.push(report);
          for (const frame of captured) {
            const { image, ...metadata } = frame;
            const path = `${domain}/${result.variant}-${frame.id}.png`;
            await Bun.write(join(output, path), Buffer.from(image.split(",")[1], "base64"));
            frames.push({ domain, variant: result.variant, id: frame.id, path, metadata });
            if (
              !frame.complete ||
              frame.diagnostics.length ||
              /swiftshader|software|llvmpipe/i.test(frame.measurements.adapter)
            )
              throw Error(`Incomplete hardware capture: ${path}`);
          }
          await save();
          if (errors.length) throw Error(errors.join("\n"));
        }
      }
      console.log(
        JSON.stringify({
          output,
          domains,
          variants: reports.length - domains.length,
          images: frames.length,
          visualAcceptance: "unreviewed",
        }),
      );
    } catch (error) {
      errors.push(error instanceof Error ? error.message : String(error));
      throw error;
    } finally {
      await save();
      await view.evaluate("window.fixture?.dispose()").catch(() => {});
      server.stop(true);
    }
  },
  size[0],
  size[1],
);
