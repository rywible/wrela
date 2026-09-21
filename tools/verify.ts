import { join, resolve } from "node:path";
import { fixtureServer, waitFor, withBrowser } from "./browser";
import { verifyStudio } from "./ui-verification";

const webkit = process.argv.includes("--webkit");
const source = `import ${JSON.stringify(resolve("tools/fixtures/render-verification.ts"))};`;
await withBrowser(
  async (view, output, errors) => {
    const server = await fixtureServer(source, output);
    try {
      await view.navigate(server.url.toString());
      await waitFor(view, "window.ready || window.failure");
      const failure = await view.evaluate("window.failure");
      if (failure && webkit && /WebGPU|adapter/.test(String(failure))) {
        const report = {
          status: "unavailable",
          backend: "Bun WKWebView (not the Safari application)",
          reason: String(failure),
          output,
        };
        await Bun.write(join(output, "compatibility.json"), JSON.stringify(report, null, 2));
        console.log(JSON.stringify(report, null, 2));
        process.exitCode = 2;
        return;
      }
      if (failure) throw new Error(`${String(failure)}\n${errors.join("\n")}`);
      const initial = await view.evaluate<{ adapter: string; diagnostics: unknown[] }>(
        "({adapter:fixture.renderer.measurements.adapter,diagnostics:fixture.renderer.diagnostics})",
      );
      if (/swiftshader|llvmpipe|software/i.test(initial.adapter))
        throw new Error(`Software adapter is not a hardware verification: ${initial.adapter}`);
      const parity = await view.evaluate("fixture.waterParity()");
      const instancing = await view.evaluate("fixture.instancing()");
      const waterApproximation = await view.evaluate("fixture.waterApproximation()");
      const budgetRejection = await view.evaluate("fixture.budgetRejection()");
      const materialRebase = await view.evaluate("fixture.materialRebase()");
      const storage = await view.evaluate("fixture.storage()");
      const shaderDiagnostics = await view.evaluate<{ line: number; column: number; message: string }[]>(
        "fixture.shaderDiagnostics()",
      );
      if (!shaderDiagnostics.some((message) => message.line === 2 && message.column > 0))
        throw new Error("Shader diagnostics did not preserve WGSL source location.");
      const modes = [
        "beauty",
        "normals",
        "depth",
        "lod",
        "binding",
        "silhouette",
        "identity",
        "albedo",
        "roughness",
        "metallic",
      ];
      for (const mode of modes) {
        await view.evaluate(
          `(()=>{fixture.scene.mode=${JSON.stringify(mode)};fixture.renderer.render(fixture.scene);return true})()`,
        );
        await Bun.write(join(output, `${mode}.png`), await view.screenshot());
      }
      const pointLighting = await view.evaluate<boolean>("fixture.pointLighting()");
      if (!pointLighting) throw new Error("Authored point lighting did not affect captured pixels.");
      const capture = await view.evaluate<number>("fixture.capture().then(blob=>blob.size)");
      if (capture < 1000) throw new Error(`Capture unexpectedly small: ${capture}`);
      await view.evaluate("fixture.lose()");
      await waitFor(
        view,
        'fixture.renderer.status === "ready" && fixture.renderer.diagnostics.some(d=>d.code==="device-lost")',
      );
      await view.evaluate("fixture.renderer.render(fixture.scene)");
      const final = await view.evaluate<{
        diagnostics: { severity: string; message: string }[];
        errors: string[];
        measurements: unknown;
      }>(
        "({diagnostics:fixture.renderer.diagnostics,errors:fixture.errors,measurements:fixture.renderer.measurements})",
      );
      const failures = [
        ...errors,
        ...final.errors,
        ...final.diagnostics.filter((d) => d.severity === "error").map((d) => d.message),
      ];
      if (failures.length) throw new Error(failures.join("\n"));
      const studio = webkit || process.argv.includes("--gpu-only") ? null : await verifyStudio(view, output);
      if (errors.length) throw new Error(errors.join("\n"));
      const report = {
        sourceManifest: "source-manifest.json",
        environmentManifest: "environment.json",
        studio,
        status: "passed",
        backend: webkit ? "Bun WKWebView WebGPU (not the Safari application)" : "Chromium hardware WebGPU",
        adapter: initial.adapter,
        waterParity: parity,
        waterApproximation,
        instancing,
        budgetRejection,
        materialRebase,
        storage,
        shaderDiagnostics,
        pointLighting,
        modes,
        captureBytes: capture,
        deviceLossRecovery: true,
        ...final,
      };
      await Bun.write(join(output, "verification.json"), JSON.stringify(report, null, 2));
      console.log(
        JSON.stringify(
          {
            status: report.status,
            backend: report.backend,
            adapter: report.adapter,
            waterParity: report.waterParity,
            instancing: report.instancing,
            budgetRejection: report.budgetRejection,
            materialRebase: report.materialRebase,
            uiChecks: studio?.checks.length,
            output,
          },
          null,
          2,
        ),
      );
    } finally {
      server.stop(true);
    }
  },
  1280,
  800,
  webkit ? "webkit" : "chrome",
);
