import init, { Replica } from "./wasm/quiddity_wasm";
import type {
  BlockHit,
  Connections,
  Device,
  Entity,
  EntityId,
  EntityRef,
  Merged,
  Reference,
  Summary,
  TitleHit,
} from "./model";

/** Where the graph lives: the Tauri shell, or memory for plain-browser dev. */
export interface Backend {
  load(): Promise<Uint8Array>;
  /** `typed` names the blocks the updates were typed in, when that is all
   * they are: the shell holds typing back from devices that can't see it. */
  push(updates: Uint8Array, typed: string[]): Promise<void>;
  /** The blocks being edited, which other devices see. Backends with no
   * other devices don't have it. */
  focus?(blocks: string[]): Promise<void>;
  /** Calls `onUpdates` with what other devices change, for as long as the
   * page lives. Backends with no other devices don't have it. */
  subscribe?(onUpdates: (updates: Uint8Array) => void): Promise<void>;
  /** This device's ID, for letting it in from another device. Only the app
   * is a device that others can reach. */
  deviceId?(): Promise<string>;
  /** Lets another device in, given the ID it shows. Rejects with the reason
   * if the ID is not one. */
  allowDevice?(device: string): Promise<void>;
}

/** Views catch up this long after typing stops; structure changes at once. */
const NOTIFY_MS = 120;
/** Typing is one undo step until it pauses this long. */
const TYPING_MS = 800;
/** The shell hears of edits this long after the last one. */
const PUSH_MS = 300;
const RETRY_MS = 2000;

const sameBytes = (a: Uint8Array, b: Uint8Array) =>
  a.length === b.length && a.every((x, i) => x === b[i]);

/** Today's date as `YYYY-MM-DD`, where the user is. */
export function today(): string {
  const d = new Date();
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

/**
 * The UI's replica of the graph. Reads are answered locally and kept until the
 * next edit; edits apply locally, tell subscribers, and go to the shell as
 * updates a moment later, so nothing the user types waits on IPC.
 */
export class Graph {
  private revision = 0;
  private listeners = new Set<() => void>();
  private cache = new Map<string, unknown>();
  private pushed: Uint8Array;
  /** Blocks typed in since the last push, and whether anything else changed. */
  private typed = new Set<string>();
  private structural = false;
  /** The block being edited, as other devices are told. */
  private focused: string | undefined;
  /** What the shell was last told, to say nothing twice. */
  private reported = "";
  private pushing: Promise<void> = Promise.resolve();
  private pushTimer: ReturnType<typeof setTimeout> | undefined;
  private notifyTimer: ReturnType<typeof setTimeout> | undefined;
  private typingTimer: ReturnType<typeof setTimeout> | undefined;

  constructor(
    private replica: Replica,
    private backend: Backend,
  ) {
    this.pushed = replica.version();
  }

  /** For `useSyncExternalStore`. */
  subscribe = (listener: () => void) => {
    this.listeners.add(listener);
    return () => void this.listeners.delete(listener);
  };

  /** Changes whenever something a view may have read has. */
  getRevision = () => this.revision;

  private notify() {
    clearTimeout(this.notifyTimer);
    this.notifyTimer = undefined;
    this.revision++;
    this.listeners.forEach((l) => l());
  }

  /** Tells subscribers now of typing they haven't heard about yet. Call
   * before a view that was being typed in goes back to showing the graph. */
  settle() {
    if (this.notifyTimer !== undefined) this.notify();
  }

  private read<T>(key: string, read: () => unknown): T {
    if (!this.cache.has(key)) this.cache.set(key, read());
    return this.cache.get(key) as T;
  }

  entity = (id: EntityId) => this.read<Entity>(`e:${id}`, () => this.replica.entity(id));
  entities = () => this.read<EntityRef[]>("entities", () => this.replica.entities());
  summaries = () => this.read<Summary[]>("summaries", () => this.replica.summaries());
  connections = () =>
    this.read<Connections>("connections", () => this.replica.connections());
  favourites = () => this.read<EntityRef[]>("favourites", () => this.replica.favourites());
  journals = () =>
    this.read<EntityRef[]>("journals", () => this.replica.journals(today()));
  references = (id: EntityId) =>
    this.read<Reference[]>(`r:${id}`, () => this.replica.references(id));
  unlinkedReferences = (id: EntityId) =>
    this.read<Reference[]>(`u:${id}`, () => this.replica.unlinkedReferences(id));

  /** Entities whose titles match `query`, best first. Not kept: a query is
   * rarely asked twice, and the answer needn't be stable between edits. */
  searchTitles = (query: string, limit: number) =>
    this.replica.searchTitles(query, limit) as TitleHit[];

  /** Blocks whose text matches `query`, best first. Not kept, as above. */
  searchText = (query: string, limit: number) =>
    this.replica.searchText(query, limit) as BlockHit[];

  /** The devices the graph records, most recently seen first. */
  devices = () => this.read<Device[]>("devices", () => this.replica.devices());

  /** Whether this build is a device others can reach, and so can pair. */
  canPair = () => this.backend.deviceId !== undefined && this.backend.allowDevice !== undefined;

  private id: Promise<string> | undefined;

  /** This device's ID. Asked for once: a promise to `use`. */
  deviceId = () => (this.id ??= this.backend.deviceId!());

  /** Lets another device in. Rejects with a message the user can read. */
  async allowDevice(device: string) {
    try {
      await this.backend.allowDevice!(device);
    } catch (error) {
      throw new Error(error instanceof Error ? error.message : String(error));
    }
  }

  /** A block's text as it is now, which a rendered view may lag behind. */
  blockText = (id: string) => this.replica.blockText(id);

  /** Takes changes made on another device, which the shell passes on.
   * Views are told at once, as for any change that isn't typing. They are not
   * pushed back: the shell has them. */
  receive(updates: Uint8Array) {
    this.replica.import(updates);
    this.cache.clear();
    this.notify();
  }

  /** Records an edit. Typing notifies after a pause, so a view of the block
   * being typed in isn't rebuilt per keystroke; structure notifies at once. */
  private edited<T>(result: T, typing = false): T {
    if (!typing) this.structural = true;
    this.cache.clear();
    this.schedulePush();
    if (typing) {
      this.notifyTimer ??= setTimeout(() => this.notify(), NOTIFY_MS);
    } else {
      this.notify();
    }
    return result;
  }

  spliceText(id: string, pos: number, len: number, insert: string) {
    this.replica.spliceText(id, pos, len, insert);
    this.typed.add(id);
    clearTimeout(this.typingTimer);
    this.typingTimer = setTimeout(() => this.replica.endTyping(), TYPING_MS);
    this.edited(undefined, true);
  }

  /** Undoes the latest step of the user's own edits. False if there is none. */
  undo() {
    return this.edited(this.replica.undo());
  }

  redo() {
    return this.edited(this.replica.redo());
  }

  createBlock(parent: string, index: number | undefined, text: string) {
    return this.edited(this.replica.createBlock(parent, index, text));
  }

  createEntity(name: string) {
    return this.edited(this.replica.createEntity(name));
  }

  /** Renames an entity, keeping the old name as an alias. */
  renameEntity(id: string, name: string) {
    return this.edited(this.replica.renameEntity(id, name));
  }

  /** Deletes entities and everything in them, as one undo step. */
  deleteEntities(ids: string[]) {
    return this.edited(this.replica.deleteEntities(ids));
  }

  /** The entity a link goes to, made first if there is none. `key` is the
   * text between the brackets, as an entity's `links` has it. */
  followLink(key: string) {
    return this.edited(this.replica.followLink(key));
  }

  /** The entity that defines the property `key`, made first if there is none. */
  defineProperty(key: string) {
    return this.edited(this.replica.defineProperty(key));
  }

  /** Settles a block after editing, given its text from before editing began:
   * links it gained that go nowhere get an entity, and one made from a link
   * just corrected is renamed. Returns the ids made. */
  commitBlock(id: string, before: string) {
    return this.edited(this.replica.commitBlock(id, before));
  }

  /** Splits a block at `at`; returns the new block. */
  split(id: string, at: number) {
    return this.edited(this.replica.split(id, at));
  }

  indent(id: string) {
    return this.edited(this.replica.indent(id));
  }

  outdent(id: string) {
    return this.edited(this.replica.outdent(id));
  }

  moveSibling(id: string, delta: number) {
    return this.edited(this.replica.moveSibling(id, delta));
  }

  mergeIntoPrevious(id: string): Merged | undefined {
    return this.edited(this.replica.mergeIntoPrevious(id) as Merged | undefined);
  }

  ensureToday(): EntityRef {
    const before = this.replica.version();
    const day = this.replica.ensureJournal(today()) as EntityRef;
    if (!sameBytes(before, this.replica.version())) this.edited(undefined);
    return day;
  }

  clearUndo() {
    this.replica.clearUndo();
  }

  private schedulePush(delay = PUSH_MS) {
    clearTimeout(this.pushTimer);
    this.pushTimer = setTimeout(() => void this.flush(), delay);
  }

  /** Sends the shell what it hasn't seen. Resolves once it has been told. */
  flush(): Promise<void> {
    clearTimeout(this.pushTimer);
    this.pushing = this.pushing.then(async () => {
      const version = this.replica.version();
      if (sameBytes(version, this.pushed)) return;
      // Only typing can be held back: anything else goes at once.
      const typed = this.structural ? [] : [...this.typed];
      const structural = this.structural;
      this.typed.clear();
      this.structural = false;
      try {
        await this.backend.push(this.replica.exportUpdates(this.pushed), typed);
        this.pushed = version;
      } catch (error) {
        console.error("Couldn't save to the shell; will retry", error);
        typed.forEach((id) => this.typed.add(id));
        this.structural ||= structural;
        this.schedulePush(RETRY_MS);
      }
    });
    return this.pushing;
  }

  /** `id` is the block being edited. The shell is told after it has what was
   * typed before, so moving between blocks ends the typing in the last. */
  focus(id: string) {
    if (this.focused === id) return;
    this.focused = id;
    void this.report();
  }

  /** `id` is no longer being edited, unless another block has been since. */
  blur(id: string) {
    if (this.focused !== id) return;
    this.focused = undefined;
    void this.report();
  }

  /** Tells the shell which block is open, as it is by the time it hears:
   * a move from one block to another is one report, not two. */
  private async report() {
    try {
      await this.flush();
      const blocks = this.focused === undefined ? [] : [this.focused];
      const key = blocks.join(",");
      if (key === this.reported) return;
      this.reported = key;
      await this.backend.focus?.(blocks);
    } catch (error) {
      console.error("Couldn't say which block is open", error);
    }
  }
}

let booted: Promise<Graph> | undefined;

/** Loads the replica from wherever the graph lives. Once per page. */
export function bootGraph(): Promise<Graph> {
  booted ??= (async () => {
    await init();
    const inTauri = "__TAURI_INTERNALS__" in window;
    const backend: Backend = process.env.NEXT_PUBLIC_QUIDDITY_STATIC_GRAPH
      ? (await import("./backend-static")).staticBackend
      : inTauri
        ? (await import("./backend-tauri")).tauriBackend
        : (await import("./backend-memory")).memoryBackend;
    const graph = new Graph(new Replica(await backend.load()), backend);
    // Before today's page is made, so nothing that arrives is missed.
    backend
      .subscribe?.((updates) => graph.receive(updates))
      .catch((error) => console.error("Couldn't listen for other devices", error));
    graph.ensureToday();
    // Making today's page isn't the user's doing, so it isn't theirs to undo.
    graph.clearUndo();
    const save = () => void graph.flush();
    window.addEventListener("pagehide", save);
    document.addEventListener("visibilitychange", () => document.hidden && save());
    return graph;
  })();
  return booted;
}
