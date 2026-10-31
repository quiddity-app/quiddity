import { Channel, invoke } from "@tauri-apps/api/core";
import type { Backend } from "./graph";

/** The shell holds the graph on disk; bytes cross as raw IPC bodies. */
export const tauriBackend: Backend = {
  async load() {
    return new Uint8Array(await invoke<ArrayBuffer>("load_graph"));
  },
  async push(updates, typed) {
    // Block IDs are digits and an `@`, so a comma can separate them.
    const headers: Record<string, string> = {};
    if (typed.length > 0) headers["x-typed"] = typed.join(",");
    await invoke("push_updates", updates, { headers });
  },
  async focus(blocks) {
    await invoke("set_focus", { blocks });
  },
  /** What other devices change arrives on a channel, as raw bytes. */
  async subscribe(onUpdates) {
    const channel = new Channel<ArrayBuffer>();
    channel.onmessage = (bytes) => onUpdates(new Uint8Array(bytes));
    await invoke("subscribe_updates", { channel });
  },
};
