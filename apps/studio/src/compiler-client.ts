import { BrowserCompiler } from "../../../packages/compiler/src/client";
export class CompilerClient extends BrowserCompiler {
  constructor() {
    super("/compile-worker.js");
  }
}
