import { WaterTemporalResolve } from "@wrela/render-webgpu/water-temporal";

/** Exercises the actual shipped shaders: accepted history, disocclusion, identity,
 * foreground exclusion, reset, odd dimensions and detail-preserving correction. */
export async function verifyWaterHistory(device: GPUDevice) {
  const width = 7,
    height = 5,
    rowBytes = 256;
  const color = device.createTexture({
    size: [width, height],
    format: "rgba16float",
    usage:
      GPUTextureUsage.TEXTURE_BINDING |
      GPUTextureUsage.RENDER_ATTACHMENT |
      GPUTextureUsage.COPY_DST |
      GPUTextureUsage.COPY_SRC,
  });
  const motion = device.createTexture({
    size: [width, height],
    format: "rgba32float",
    usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.COPY_DST,
  });
  const depth = device.createTexture({
    size: [width, height],
    format: "depth32float",
    usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.RENDER_ATTACHMENT,
  });
  device.pushErrorScope("validation");
  const history = new WaterTemporalResolve(device, width, height);
  const creationError = await device.popErrorScope();
  if (creationError) throw Error(creationError.message);
  const readback = device.createBuffer({
    size: rowBytes * height,
    usage: GPUBufferUsage.COPY_DST | GPUBufferUsage.MAP_READ,
  });
  const half = (v: number) => {
    const exp = (v >> 10) & 31,
      fraction = v & 1023;
    return (v & 32768 ? -1 : 1) * (exp === 0 ? fraction * 2 ** -24 : (1 + fraction / 1024) * 2 ** (exp - 15));
  };
  const encodeHalf = (v: number) => {
    const exponent = Math.floor(Math.log2(v));
    return ((exponent + 15) << 10) + Math.round((v / 2 ** exponent - 1) * 1024);
  };
  const results: { name: string; correction: number; detailError: number }[] = [];
  try {
    for (const name of [
      "accepted",
      "identity-rejected",
      "depth-rejected",
      "opaque-excluded",
      "reset",
      "frozen",
    ]) {
      history.reset();
      for (let frame = 0; frame < 2; frame++) {
        device.pushErrorScope("validation");
        const shift = frame === 1 && name !== "frozen" ? -0.03125 : 0;
        const colors = new Uint16Array(width * height * 4),
          vectors = new Float32Array(width * height * 4);
        for (let y = 0; y < height; y++)
          for (let x = 0; x < width; x++) {
            const at = (y * width + x) * 4,
              value = (x % 2 === 0 ? 0.375 : 0.625) + shift;
            colors.set([encodeHalf(value), encodeHalf(value), encodeHalf(value), 0x3c00], at);
            const identity =
              frame === 1 && name === "identity-rejected"
                ? -43
                : frame === 1 && name === "opaque-excluded"
                  ? 42
                  : -42;
            vectors.set(
              [
                (x + 0.5) / width,
                (y + 0.5) / height,
                frame === 1 && name === "depth-rejected" ? 10 : 125 / (2500 - 0.95 * 2499.95),
                identity,
              ],
              at,
            );
          }
        device.queue.writeTexture({ texture: color }, colors, { bytesPerRow: width * 8 }, [width, height]);
        device.queue.writeTexture({ texture: motion }, vectors, { bytesPerRow: width * 16 }, [width, height]);
        if (frame === 1 && name === "reset") history.reset();
        const encoder = device.createCommandEncoder();
        const clear = encoder.beginRenderPass({
          colorAttachments: [],
          depthStencilAttachment: {
            view: depth.createView(),
            depthLoadOp: "clear",
            depthStoreOp: "store",
            depthClearValue: 0.95,
          },
        });
        clear.end();
        history.encode(encoder, color, motion, depth);
        encoder.copyTextureToBuffer({ texture: color }, { buffer: readback, bytesPerRow: rowBytes }, [
          width,
          height,
        ]);
        device.queue.submit([encoder.finish()]);
        const validation = await device.popErrorScope();
        if (validation) throw Error(validation.message);
        await readback.mapAsync(GPUMapMode.READ);
        if (frame === 1) {
          const data = new Uint16Array(readback.getMappedRange());
          const a = half(data[0]),
            b = half(data[4]),
            correction = a - (0.375 + shift),
            detailError = Math.abs(b - a - 0.25);
          if (
            !Number.isFinite(correction) ||
            detailError > 0.003 ||
            (name === "accepted" ? correction < 0.002 || correction > 0.03 : Math.abs(correction) > 0.0005)
          )
            throw Error(`Water history ${name}: correction ${correction}, detail error ${detailError}`);
          results.push({ name, correction, detailError });
        }
        readback.unmap();
      }
    }
    return { results, byteLength: history.byteLength };
  } finally {
    history.destroy();
    color.destroy();
    motion.destroy();
    depth.destroy();
    readback.destroy();
  }
}
