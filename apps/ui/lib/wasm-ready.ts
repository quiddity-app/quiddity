import init from './wasm/quiddity_wasm';

let ready: Promise<unknown> | undefined;

/**
 * Starts compiling the WASM, once, and resolves when it is ready. Anything
 * that makes a `Replica` has to wait for it; the rest of starting up (choosing
 * the backend, fetching the snapshot) needn't, so it can happen meanwhile.
 */
export function wasmReady(): Promise<unknown> {
  return (ready ??= init());
}
