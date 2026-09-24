import { expect, test } from "bun:test";
import type { MeshData } from "@wrela/model";

import { packCreatureDeformation } from "./creature-deformation";
import { WebGPURenderer } from "./index";
import { packReliefSourceVertices, packVertices } from "./packing";
import { meshVertexBufferLayouts } from "./vertex-layout";

const mesh: MeshData = {
  positions: new Float32Array([1, 2, 2.75, 4, 5, 5.5]),
  normals: new Float32Array([0, 0.6, 0.8, 0.6, 0, 0.8]),
  reliefCoordinates: new Float32Array([1, 2, 3, 4, 5, 6]),
  reliefNormals: new Float32Array([0, 0, 1, 0, 0, 1]),
  indices: new Uint32Array([0, 1, 0]),
  bounds: { min: [1, 2, 2.75], max: [4, 5, 6] },
};

/** Decode the actual shader attribute addresses, including nonzero vertex indices. */
function attribute(streams: Float32Array[], compact: boolean, location: number, vertex: number): number[] {
  for (const [slot, layout] of meshVertexBufferLayouts(compact).entries()) {
    const input = [...layout.attributes].find(({ shaderLocation }) => shaderLocation === location);
    if (input) {
      const offset = (layout.arrayStride * vertex + input.offset) / 4;
      return [...streams[slot].slice(offset, offset + Number(input.format.at(-1)))];
    }
  }
  throw Error("Unknown vertex attribute");
}

test("compact relief contains only exact source positions and normals", () => {
  const packed = packReliefSourceVertices(mesh) as Float32Array;
  expect(packed.byteLength).toBe(2 * 24);
  expect([...packed]).toEqual([1, 2, 3, 0, 0, 1, 4, 5, 6, 0, 0, 1]);
  expect(
    packReliefSourceVertices({ ...mesh, reliefCoordinates: undefined, reliefNormals: undefined }),
  ).toBeUndefined();
  for (const malformed of [
    { ...mesh, reliefCoordinates: undefined },
    { ...mesh, reliefNormals: undefined },
    { ...mesh, reliefCoordinates: new Float32Array(3) },
    { ...mesh, reliefNormals: new Float32Array(3) },
  ])
    expect(() => packReliefSourceVertices(malformed)).toThrow("Malformed relief source");
});

test("both vertex layouts preserve immutable material coordinates through live correctives and motion history", () => {
  const material = {
    color: [1, 1, 1] as [number, number, number],
    secondary: [1, 1, 1] as [number, number, number],
    roughness: 1,
    metallic: 0,
    pattern: 0,
    scale: 1,
    normalStrength: 0,
  };
  const current = packCreatureDeformation({
    id: "subject",
    source: "subject",
    mesh,
    material,
    matrix: new Float32Array(16),
    deformation: {
      revision: "pose",
      vertexIndices: new Uint32Array([1]),
      positionDeltas: new Float32Array([0, 0, 0.25]),
      maxDisplacement: 0.25,
    },
  });
  const rest = packVertices(mesh),
    previous = rest.slice();
  previous[23] = 3.5;
  for (const compact of [false, true]) {
    const streams = [current, compact ? (packReliefSourceVertices(mesh) as Float32Array) : rest, previous];
    expect(attribute(streams, compact, 0, 1)).toEqual([4, 5, 5.75]);
    expect(attribute(streams, compact, 6, 1)).toEqual([3.5, 5, 5.5]);
    for (const location of [5, 9])
      expect(attribute(streams, compact, location, 1)).toEqual(compact ? [4, 5, 6] : [4, 5, 5.5]);
    expect(attribute(streams, compact, 10, 1)).toEqual(compact ? [0, 0, 1] : [...mesh.normals.slice(3)]);
  }
  expect([...mesh.positions.slice(3)]).toEqual([4, 5, 5.5]);
});

test("relief pipelines are created on demand and retain shader, render targets and depth state", async () => {
  const descriptors: GPURenderPipelineDescriptor[] = [];
  const Constructor = WebGPURenderer as unknown as new (
    canvas: HTMLCanvasElement,
    options: object,
  ) => {
    device: unknown;
    createMeshPipeline(descriptor: GPURenderPipelineDescriptor): Promise<GPURenderPipeline>;
    meshPipeline(pipeline: GPURenderPipeline, mesh?: { sourceVertex?: unknown }): GPURenderPipeline;
  };
  const renderer = new Constructor({} as HTMLCanvasElement, {});
  renderer.device = {
    createRenderPipeline: (descriptor: GPURenderPipelineDescriptor) => {
      descriptors.push(descriptor);
      return descriptor;
    },
    createRenderPipelineAsync: async (descriptor: GPURenderPipelineDescriptor) => {
      descriptors.push(descriptor);
      return descriptor;
    },
  };
  const module = {} as GPUShaderModule;
  const descriptor: GPURenderPipelineDescriptor = {
    label: "Shadow",
    layout: {} as GPUPipelineLayout,
    vertex: { module, entryPoint: "shadowMain" },
    fragment: { module, entryPoint: "shadowFragment", targets: [] },
    primitive: { topology: "triangle-list", cullMode: "none" },
    depthStencil: { format: "depth32float", depthWriteEnabled: true, depthCompare: "less", depthBias: 2 },
  };
  const ordinary = await renderer.createMeshPipeline(descriptor);
  expect(descriptors).toHaveLength(1);
  expect(renderer.meshPipeline(ordinary)).toBe(ordinary);
  expect(descriptors).toHaveLength(1);
  expect(renderer.meshPipeline(ordinary, { sourceVertex: {} })).toBe(
    descriptors[1] as unknown as GPURenderPipeline,
  );
  expect(descriptors).toHaveLength(2);
  renderer.meshPipeline(ordinary, { sourceVertex: {} });
  expect(descriptors).toHaveLength(2);
  expect(descriptors[0]).toEqual({
    ...descriptors[1],
    label: descriptor.label,
    vertex: descriptors[0].vertex,
  });
  const normalBuffers = [...(descriptors[0].vertex.buffers ?? [])];
  const reliefBuffers = [...(descriptors[1].vertex.buffers ?? [])];
  expect(normalBuffers[1]?.arrayStride).toBe(92);
  expect(reliefBuffers[1]?.arrayStride).toBe(24);
  expect(normalBuffers[1]).toEqual({ ...reliefBuffers[1], arrayStride: 92 } as GPUVertexBufferLayout);
  expect(reliefBuffers[0]).toEqual(normalBuffers[0]);
  expect(reliefBuffers[2]).toEqual(normalBuffers[2]);
});
