import type { MaterialLattice } from "@wrela/model";

/** One resident immutable product. No per-frame baking or unbounded material atlas. */
export class MaterialLatticeGpu {
  texture: GPUTexture;
  readonly bounds: GPUBuffer;
  bytes = 32;
  private key = "";
  constructor(private device: GPUDevice) {
    this.texture = this.empty();
    this.bounds = device.createBuffer({ size: 16, usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST });
  }
  private empty() {
    return this.device.createTexture({
      size: [1, 1, 1],
      dimension: "3d",
      format: "rgba32float",
      usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.COPY_DST,
    });
  }
  update(product: MaterialLattice | undefined, allowance: number): number {
    if (
      product &&
      (product.version !== "material-lattice-1" ||
        !Number.isInteger(product.edge) ||
        product.edge < 1 ||
        product.edge > 64 ||
        product.corners.length !== product.edge ** 3 * 8 ||
        product.origin.some((n) => !Number.isInteger(n) || Math.abs(n) > 512))
    )
      throw new Error("Invalid compiled material lattice");
    if (product && product.corners.byteLength + 16 > allowance) product = undefined;
    if ((product?.key ?? "") === this.key) return 0;
    this.key = product?.key ?? "";
    this.texture.destroy();
    this.texture = product
      ? this.device.createTexture({
          label: "Compiled material lattice",
          size: [product.edge, product.edge, product.edge * 2],
          dimension: "3d",
          format: "rgba32float",
          usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.COPY_DST,
        })
      : this.empty();
    this.bytes = product ? product.corners.byteLength + 16 : 32;
    const bounds = new Int32Array(product ? [...product.origin, product.edge] : [0, 0, 0, 0]);
    this.device.queue.writeBuffer(this.bounds, 0, bounds);
    if (product)
      this.device.queue.writeTexture(
        { texture: this.texture },
        product.corners as Float32Array<ArrayBuffer>,
        { bytesPerRow: product.edge * 16, rowsPerImage: product.edge },
        [product.edge, product.edge, product.edge * 2],
      );
    return 16 + (product?.corners.byteLength ?? 0);
  }
  destroy() {
    this.texture.destroy();
    this.bounds.destroy();
  }
}
