import { RELIEF_VERTEX_FLOATS, VERTEX_FLOATS } from "./packing";

/** Only relief pays for a compact immutable position/normal stream. Ordinary
 * meshes bind their main immutable buffer again, even when slot zero is deformed. */
export function meshVertexBufferLayouts(compactRelief = false): GPUVertexBufferLayout[] {
  return [
    {
      arrayStride: VERTEX_FLOATS * 4,
      attributes: [
        { shaderLocation: 0, offset: 0, format: "float32x3" },
        { shaderLocation: 1, offset: 12, format: "float32x3" },
        { shaderLocation: 2, offset: 24, format: "float32x3" },
        { shaderLocation: 3, offset: 36, format: "float32x4" },
        { shaderLocation: 4, offset: 52, format: "float32x4" },
        { shaderLocation: 7, offset: 68, format: "float32x4" },
        { shaderLocation: 8, offset: 84, format: "float32x2" },
      ],
    },
    {
      arrayStride: (compactRelief ? RELIEF_VERTEX_FLOATS : VERTEX_FLOATS) * 4,
      attributes: [
        { shaderLocation: 5, offset: 0, format: "float32x3" },
        { shaderLocation: 9, offset: 0, format: "float32x3" },
        { shaderLocation: 10, offset: 12, format: "float32x3" },
      ],
    },
    {
      arrayStride: VERTEX_FLOATS * 4,
      attributes: [{ shaderLocation: 6, offset: 0, format: "float32x3" }],
    },
  ];
}
