// The CPU state hash: 64-bit FNV-1a over every byte a program submits, in order (AC7). The
// TypeScript mirror of wrela-abi's `StateHash` (runtime/abi/src/hash.rs).
//
// The 64-bit state is kept as two u32 halves rather than a BigInt, so hashing costs a few integer
// operations per byte. Multiplying by the prime 2^40 + 0x1b3 modulo 2^64 is `h * 0x1b3` plus the
// low word shifted into the high word by 8 (40 - 32).

/** FNV-1a's 64-bit offset basis: the hash of no bytes. */
export const OFFSET_BASIS = 0xcbf2_9ce4_8422_2325n;

/** FNV-1a's 64-bit prime. */
export const PRIME = 0x0000_0100_0000_01b3n;

const PRIME_LOW = 0x1b3;
const TWO_32 = 0x1_0000_0000;

/** A running FNV-1a hash. Hashing bytes in pieces gives the same result as hashing them at once. */
export class StateHash {
  private hi = Number(OFFSET_BASIS >> 32n);
  private lo = Number(OFFSET_BASIS & 0xffff_ffffn);

  update(bytes: Uint8Array): void {
    let hi = this.hi;
    let lo = this.lo;
    for (let i = 0; i < bytes.length; i++) {
      lo = (lo ^ (bytes[i] ?? 0)) >>> 0;
      const low = lo * PRIME_LOW; // below 2^41, so exact in a double
      hi = (Math.imul(hi, PRIME_LOW) + Math.floor(low / TWO_32) + (lo << 8)) >>> 0;
      lo = low >>> 0;
    }
    this.hi = hi;
    this.lo = lo;
  }

  value(): bigint {
    return (BigInt(this.hi) << 32n) | BigInt(this.lo);
  }

  /** The hash as hosts report it: 16 lowercase hex digits. */
  hex(): string {
    return this.hi.toString(16).padStart(8, "0") + this.lo.toString(16).padStart(8, "0");
  }

  toString(): string {
    return this.hex();
  }
}
