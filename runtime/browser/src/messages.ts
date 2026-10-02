// What the page's main thread and the render worker say to each other.

/** Basic input, forwarded as it happens. Positions are in physical pixels from the canvas's top left. */
export type InputEvent =
  | {
      readonly kind: "pointer";
      readonly action: "move" | "down" | "up";
      readonly x: number;
      readonly y: number;
      readonly buttons: number;
    }
  | { readonly kind: "key"; readonly action: "down" | "up"; readonly code: string }
  | { readonly kind: "focus"; readonly focused: boolean };

export type ToWorker =
  /** Sent once: the canvas to render to, the page's URL (the manifest is beside it), and the mode. */
  | {
      readonly type: "start";
      readonly canvas: OffscreenCanvas;
      readonly base: string;
      readonly test: boolean;
      readonly width: number;
      readonly height: number;
    }
  /** The canvas's new size in physical pixels. */
  | { readonly type: "resize"; readonly width: number; readonly height: number }
  | { readonly type: "input"; readonly input: InputEvent };

export type FromWorker =
  | { readonly type: "status"; readonly text: string }
  | { readonly type: "error"; readonly text: string }
  /** The test run is over, and its results are saved (or it failed). */
  | { readonly type: "done"; readonly ok: boolean; readonly text: string };
