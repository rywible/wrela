/** Encode away from the main thread's canvas idle deadline. Each request owns its short-lived
 * worker; fallback supports browsers/CSPs without worker-backed OffscreenCanvas. */
const encoderSource = `self.onmessage=async({data})=>{try{
  const canvas=new OffscreenCanvas(data.width,data.height),context=canvas.getContext('2d');
  if(!context)throw Error('Capture encoding context unavailable');
  context.putImageData(new ImageData(new Uint8ClampedArray(data.pixels),data.width,data.height),0,0);
  self.postMessage({blob:await canvas.convertToBlob({type:'image/png'})});
}catch(error){self.postMessage({error:String(error)})}};`;
export async function encodeCapturePixels(
  pixels: Uint8ClampedArray<ArrayBuffer>,
  width: number,
  height: number,
): Promise<Blob> {
  if (
    typeof Worker !== "undefined" &&
    typeof OffscreenCanvas !== "undefined" &&
    typeof OffscreenCanvas.prototype.convertToBlob === "function"
  ) {
    const url = URL.createObjectURL(new Blob([encoderSource], { type: "text/javascript" }));
    let worker: Worker | undefined;
    try {
      worker = new Worker(url);
    } catch {
      /* CSP or browser policy may prohibit blob workers. */
    }
    if (worker) {
      const active = worker;
      let timeout: ReturnType<typeof setTimeout> | undefined;
      try {
        return await new Promise<Blob>((resolve, reject) => {
          timeout = setTimeout(() => reject(Error("Capture encoder did not respond")), 10000);
          active.onmessage = ({ data }) =>
            data.blob instanceof Blob
              ? resolve(data.blob)
              : reject(Error(data.error ?? "Capture encoding failed"));
          active.onerror = (event) => {
            event.preventDefault();
            reject(Error(event.message || "Capture encoding worker failed"));
          };
          const copy = pixels.slice().buffer;
          active.postMessage({ pixels: copy, width, height }, [copy]);
        });
      } catch {
        // Fall through to the browser canvas path when worker encoding is unavailable.
      } finally {
        clearTimeout(timeout);
        active.terminate();
        URL.revokeObjectURL(url);
      }
    }
    if (!worker) URL.revokeObjectURL(url);
  }
  const data = new ImageData(pixels, width, height);
  if (
    typeof OffscreenCanvas !== "undefined" &&
    typeof OffscreenCanvas.prototype.convertToBlob === "function"
  ) {
    const canvas = new OffscreenCanvas(width, height),
      context = canvas.getContext("2d");
    if (!context) throw Error("Could not create a capture encoding surface");
    context.putImageData(data, 0, 0);
    return canvas.convertToBlob({ type: "image/png" });
  }
  const canvas = document.createElement("canvas");
  canvas.width = width;
  canvas.height = height;
  const context = canvas.getContext("2d");
  if (!context) throw Error("Could not create a capture encoding surface");
  context.putImageData(data, 0, 0);
  return new Promise((resolve, reject) =>
    canvas.toBlob(
      (blob) => (blob ? resolve(blob) : reject(Error("Could not encode viewport PNG"))),
      "image/png",
    ),
  );
}
