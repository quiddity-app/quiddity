import { Channel, invoke } from '@tauri-apps/api/core';

import type { Begin, GraphList, Problem } from './recovery';
import type { Backend } from './graph';
import type { SyncDetails } from './model';

/** The shell holds the graph on disk; bytes cross as raw IPC bodies. */
export const tauriBackend: Backend = {
  async load() {
    return new Uint8Array(await invoke<ArrayBuffer>('load_graph'));
  },
  async startupProblem() {
    return await invoke<Problem | null>('startup_problem');
  },
  async recover(how) {
    await invoke('recover', { how });
  },
  async graphs() {
    return await invoke<GraphList>('list_graphs');
  },
  async switchGraph(name) {
    await invoke('switch_graph', { name });
  },
  async createGraph(name, sync) {
    await invoke('create_graph', { name, sync });
  },
  async firstRun() {
    return await invoke<boolean>('first_run');
  },
  async beginGraph(how: Begin) {
    await invoke('start_graph', { how });
  },
  async joining() {
    return await invoke<boolean>('is_joining');
  },
  async push(updates, typed) {
    // Block IDs are digits and an `@`, so a comma can separate them.
    const headers: Record<string, string> = {};
    if (typed.length > 0) {
      headers['x-typed'] = typed.join(',');
    }
    await invoke('push_updates', updates, { headers });
  },
  async focus(blocks) {
    await invoke('set_focus', { blocks });
  },
  async networkChanged() {
    await invoke('network_changed');
  },
  async deviceId() {
    return await invoke<string>('device_id');
  },
  async allowDevice(device) {
    await invoke('allow_device', { device });
  },
  /** How this device stands with the others, and why, arrives as an object. */
  async subscribeStatus(onStatus) {
    const channel = new Channel<SyncDetails>();
    channel.onmessage = onStatus;
    await invoke('subscribe_status', { channel });
  },
  async putBlob(bytes) {
    return await invoke<string>('put_blob', bytes);
  },
  async getBlob(hash) {
    return new Uint8Array(await invoke<ArrayBuffer>('get_blob', { hash }));
  },
  /** What other devices change arrives on a channel, as raw bytes. */
  async subscribe(onUpdates) {
    const channel = new Channel<ArrayBuffer>();
    channel.onmessage = (bytes) => {
      onUpdates(new Uint8Array(bytes));
    };
    await invoke('subscribe_updates', { channel });
  },
};
