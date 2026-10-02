// A small PNG encoder for test mode's capture: 8-bit RGBA, no interlacing, each row filtered with
// "Up" (the difference from the row above), compressed with the platform's zlib
// (CompressionStream's "deflate" is the zlib format PNG wants).

const SIGNATURE = Uint8Array.of(0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a);
const FILTER_UP = 2;

const CRC_TABLE = (() => {
  const table = new Uint32Array(256);
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) {
      c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    }
    table[n] = c >>> 0;
  }
  return table;
})();

/** The CRC-32 that PNG chunks carry (ISO 3309, as in zlib). */
export function crc32(bytes: Uint8Array): number {
  let c = 0xffffffff;
  for (let i = 0; i < bytes.length; i++) {
    c = (CRC_TABLE[(c ^ (bytes[i] ?? 0)) & 0xff] ?? 0) ^ (c >>> 8);
  }
  return (c ^ 0xffffffff) >>> 0;
}

function chunk(type: string, data: Uint8Array): Uint8Array {
  const out = new Uint8Array(12 + data.length);
  const view = new DataView(out.buffer);
  view.setUint32(0, data.length);
  for (let i = 0; i < 4; i++) {
    out[4 + i] = type.charCodeAt(i);
  }
  out.set(data, 8);
  view.setUint32(8 + data.length, crc32(out.subarray(4, 8 + data.length)));
  return out;
}

async function zlib(data: Uint8Array): Promise<Uint8Array> {
  const stream = new Blob([data as Uint8Array<ArrayBuffer>])
    .stream()
    .pipeThrough(new CompressionStream("deflate"));
  return new Uint8Array(await new Response(stream).arrayBuffer());
}

/** Encodes `rgba` (tightly packed rows, top row first) as a PNG. */
export async function encodePng(
  width: number,
  height: number,
  rgba: Uint8Array,
): Promise<Uint8Array> {
  if (!Number.isInteger(width) || !Number.isInteger(height) || width < 1 || height < 1) {
    throw new RangeError(`a PNG can't be ${width}×${height}`);
  }
  const stride = width * 4;
  if (rgba.length !== stride * height) {
    throw new RangeError(`${rgba.length} bytes aren't a ${width}×${height} RGBA image`);
  }
  const filtered = new Uint8Array((stride + 1) * height);
  for (let y = 0; y < height; y++) {
    const out = y * (stride + 1);
    const row = y * stride;
    filtered[out] = FILTER_UP;
    for (let x = 0; x < stride; x++) {
      const above = y === 0 ? 0 : (rgba[row - stride + x] ?? 0);
      filtered[out + 1 + x] = ((rgba[row + x] ?? 0) - above) & 0xff;
    }
  }

  const header = new Uint8Array(13);
  const view = new DataView(header.buffer);
  view.setUint32(0, width);
  view.setUint32(4, height);
  header.set([8, 6, 0, 0, 0], 8); // 8 bits per channel, RGBA, deflate, adaptive filtering, no interlace

  const parts = [
    SIGNATURE,
    chunk("IHDR", header),
    chunk("IDAT", await zlib(filtered)),
    chunk("IEND", new Uint8Array()),
  ];
  const png = new Uint8Array(parts.reduce((n, p) => n + p.length, 0));
  let at = 0;
  for (const part of parts) {
    png.set(part, at);
    at += part.length;
  }
  return png;
}
