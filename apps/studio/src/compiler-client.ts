import { BrowserCompiler } from "@wrela/compiler/client";
export class CompilerClient extends BrowserCompiler {
  constructor() {
    super("/compile-worker.js");
  }
}
