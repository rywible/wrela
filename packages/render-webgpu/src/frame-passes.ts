/** Pass interfaces own attachment policy and timestamp slots. Submission callbacks
 * receive a live pass only for its lifetime; end() is guaranteed on encode failure. */
export function encodeShadowPass(
  encoder: GPUCommandEncoder,
  depth: GPUTexture,
  query: GPUQuerySet | undefined,
  draw: (pass: GPURenderPassEncoder) => void,
) {
  const pass = encoder.beginRenderPass({
    label: "Shadow pass",
    colorAttachments: [],
    depthStencilAttachment: {
      view: depth.createView(),
      depthClearValue: 1,
      depthLoadOp: "clear",
      depthStoreOp: "store",
    },
    timestampWrites: query
      ? { querySet: query, beginningOfPassWriteIndex: 2, endOfPassWriteIndex: 3 }
      : undefined,
  });
  try {
    draw(pass);
  } finally {
    pass.end();
  }
}
export function encodeDisplayPass(
  encoder: GPUCommandEncoder,
  target: GPUTexture,
  pipeline: GPURenderPipeline,
  global: GPUBindGroup,
  display: GPUBindGroup,
  query?: GPUQuerySet,
) {
  const pass = encoder.beginRenderPass({
    label: "Display transform and anti-aliasing resolve",
    colorAttachments: [
      {
        view: target.createView(),
        loadOp: "clear",
        storeOp: "store",
        clearValue: { r: 0, g: 0, b: 0, a: 1 },
      },
    ],
    timestampWrites: query
      ? { querySet: query, beginningOfPassWriteIndex: 8, endOfPassWriteIndex: 9 }
      : undefined,
  });
  try {
    pass.setPipeline(pipeline);
    pass.setBindGroup(0, global);
    pass.setBindGroup(1, display);
    pass.draw(3);
  } finally {
    pass.end();
  }
}
