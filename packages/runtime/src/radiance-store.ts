import type { CookedRadiance } from "@wrela/compiler";

/** Storage is an automatic acceleration layer. Failure never prevents lighting. */
export interface RadianceProductStore {
  get(source: string): Promise<CookedRadiance | undefined>;
  put(product: CookedRadiance): Promise<boolean>;
  remove(source: string): Promise<void>;
}
const MAX_BYTES = 128 * 1024 * 1024;
const MAX_ENTRY_BYTES = 64 * 1024 * 1024;
const MAX_ENTRIES = 32;
type Entry = { source: string; bytes: number; used: number };

let sharedStore: RadianceProductStore | undefined;
export function browserRadianceStore(): RadianceProductStore | undefined {
  if (typeof indexedDB === "undefined") return;
  if (sharedStore) return sharedStore;
  let pending: Promise<IDBDatabase> | undefined;
  const open = () => {
    if (pending) return pending;
    pending = new Promise<IDBDatabase>((resolve, reject) => {
      let expired = false;
      const request = indexedDB.open("wrela-compiled-lighting", 1);
      const fail = (error: unknown) => {
        expired = true;
        clearTimeout(timer);
        reject(error);
      };
      const timer = setTimeout(() => fail(Error("Lighting storage unavailable")), 1500);
      request.onupgradeneeded = () => {
        if (expired) {
          request.transaction?.abort();
          return;
        }
        request.result.createObjectStore("products", { keyPath: "source" });
        request.result.createObjectStore("entries", { keyPath: "source" });
      };
      request.onerror = () => fail(request.error);
      request.onblocked = () => fail(Error("Lighting storage blocked"));
      request.onsuccess = () => {
        clearTimeout(timer);
        const db = request.result;
        if (expired) {
          db.close();
          return;
        }
        db.onversionchange = () => {
          db.close();
          pending = undefined;
        };
        resolve(db);
      };
    });
    return pending;
  };
  const transaction = async <T>(run: (tx: IDBTransaction, done: (value: T) => void) => void): Promise<T> => {
    const db = await open();
    return new Promise<T>((resolve, reject) => {
      const tx = db.transaction(["products", "entries"], "readwrite");
      let value: T;
      tx.oncomplete = () => resolve(value);
      tx.onerror = tx.onabort = () => reject(tx.error ?? Error("Lighting storage transaction failed"));
      run(tx, (result) => {
        value = result;
      });
    });
  };
  sharedStore = {
    async get(source) {
      return transaction<CookedRadiance | undefined>((tx, done) => {
        const products = tx.objectStore("products"),
          entries = tx.objectStore("entries");
        const request = products.get(source);
        request.onsuccess = () => {
          const product = request.result as CookedRadiance | undefined;
          if (product) entries.put({ source, bytes: product.bytes, used: Date.now() });
          done(product);
        };
      });
    },
    async put(product) {
      if (!Number.isFinite(product.bytes) || product.bytes < 0 || product.bytes > MAX_ENTRY_BYTES)
        return false;
      await transaction<void>((tx, done) => {
        const products = tx.objectStore("products"),
          entries = tx.objectStore("entries");
        const request = entries.getAll();
        request.onsuccess = () => {
          const retained = (request.result as Entry[])
            .filter((e) => e.source !== product.source)
            .sort((a, b) => a.used - b.used);
          let bytes = product.bytes + retained.reduce((sum, e) => sum + e.bytes, 0);
          while (retained.length && (retained.length >= MAX_ENTRIES || bytes > MAX_BYTES)) {
            const oldest = retained.shift();
            if (!oldest) break;
            bytes -= oldest.bytes;
            products.delete(oldest.source);
            entries.delete(oldest.source);
          }
          products.put(product);
          entries.put({ source: product.source, bytes: product.bytes, used: Date.now() });
          done();
        };
      });
      return true;
    },
    async remove(source) {
      await transaction<void>((tx, done) => {
        tx.objectStore("products").delete(source);
        tx.objectStore("entries").delete(source);
        done();
      });
    },
  };
  return sharedStore;
}
