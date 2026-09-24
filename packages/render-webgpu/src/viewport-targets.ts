/** Owns the viewport attachments as one lifetime. Temporal/water histories belong
 * to their respective passes and are invalidated by the renderer when this changes. */
export class ViewportTargets {
  motion?: GPUTexture;
  depth?: GPUTexture;
  sceneColor?: GPUTexture;
  sceneMultisample?: GPUTexture;
  resize(device: GPUDevice, width: number, height: number, samples: number) {
    this.destroy();
    try {
      this.motion = device.createTexture({
        label: "Previous surface position and identity",
        size: [width, height],
        sampleCount: samples,
        format: samples > 1 ? "rgba16float" : "rgba32float",
        usage: GPUTextureUsage.RENDER_ATTACHMENT | GPUTextureUsage.TEXTURE_BINDING,
      });
      this.depth = device.createTexture({
        label: "Viewport depth",
        size: [width, height],
        sampleCount: samples,
        format: "depth32float",
        usage: GPUTextureUsage.RENDER_ATTACHMENT | GPUTextureUsage.TEXTURE_BINDING,
      });
      this.sceneColor = device.createTexture({
        label: "Scene linear HDR",
        size: [width, height],
        format: "rgba16float",
        usage:
          GPUTextureUsage.RENDER_ATTACHMENT |
          GPUTextureUsage.TEXTURE_BINDING |
          GPUTextureUsage.COPY_SRC |
          GPUTextureUsage.COPY_DST,
      });
      if (samples > 1)
        this.sceneMultisample = device.createTexture({
          label: "Scene geometric anti-aliasing samples",
          size: [width, height],
          sampleCount: samples,
          format: "rgba16float",
          usage: GPUTextureUsage.RENDER_ATTACHMENT,
        });
    } catch (error) {
      this.destroy();
      throw error;
    }
  }
  destroy() {
    this.motion?.destroy();
    this.depth?.destroy();
    this.sceneColor?.destroy();
    this.sceneMultisample?.destroy();
    this.motion = this.depth = this.sceneColor = this.sceneMultisample = undefined;
  }
}
