// Writes grazer seed 1's parameters (the `Grazer` uniform) for the native benchmark, so the
// browser and native runs evaluate the same bits. Usage: node params.mjs > params-seed1.f32
import { makeGrazer } from '../grazer.js';

process.stdout.write(Buffer.from(makeGrazer(1).params.buffer));
