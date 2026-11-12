export type EntityId = string;

/** Where the app opens: the graph's `home` setting, with its entity found. */
export type Start =
  | { kind: "journals" }
  | { kind: "all" }
  | { kind: "graph" }
  | { kind: "entity"; id: EntityId };

export interface EntityRef {
  id: EntityId;
  title: string;
}

/** How this device stands with the others. */
export type SyncState =
  /** No other device is listed, and none is connected. */
  | "alone"
  /** Others are listed and none is connected. */
  | "offline"
  /** Connected, but something is missing on one side, or typing is held back. */
  | "behind"
  /** Connected and up to date. */
  | "synced"
  /** Connected and up to date, and another device has a block open. */
  | "present";

/** A device the graph records: one that holds it, or has been let in to it. */
export interface Device {
  /** Its endpoint ID, which another device needs to let it in. */
  id: string;
  /** When it last saved, as an RFC 3339 timestamp in UTC. */
  seen: string;
}

export interface Block {
  id: string;
  text: string;
  /** The properties written in `text`, in order, for rendering as chips. */
  properties: Property[];
  /** The file this block is, if its first line is a `##blob` line that can be
   * read. The rest of `text` is its caption. */
  blob: BlobInfo | null;
  /** Whether this is its entity's properties block, drawn and edited
   * differently from the blocks around it. */
  propertiesBlock: boolean;
  children: Block[];
}

/** A property's value, as written. */
export type PropertyValue =
  | { kind: "text"; text: string }
  /** `key` is how the link is written between its brackets: a key of `links`. */
  | { kind: "link"; name: string; key: string }
  | { kind: "ref"; id: string }
  | { kind: "list"; items: PropertyValue[] };

/** One property written in a block's text. */
export interface Property {
  key: string;
  value: PropertyValue | null;
  /** A tag, `#key`, within the text, rather than `key:: value` on a line. */
  inline: boolean;
  /** Where it is written, in UTF-16 units of the block's text. For a line
   * property, the whole line. */
  start: number;
  end: number;
  /** The entity that defines the property, if anyone has. */
  definition: EntityId | null;
  /** A `##key(parameter)` or `##key:: value` line, whose parameter or value is its value. */
  meta: boolean;
  /** What doesn't fit the property's schema here, in words. */
  flags: string[];
}

/** A value an entity has from another, through `##inherits` or `##transitive`. */
export interface Inherited {
  key: string;
  value: PropertyValue | null;
  /** The entity it is written on. */
  from: EntityRef;
}

/** An entity in an inverse tree, with those related to it in turn. */
export interface InverseNode {
  entity: EntityRef;
  children: InverseNode[];
  /** Children not shown because the tree reached its limit. */
  hidden: number;
}

/** The entities related to one the other way round, under an `##inverse` label. */
export interface Inverse {
  label: string;
  tree: InverseNode[];
}

/** Where a `[[link]]` goes. */
export type LinkTarget =
  | { kind: "entity"; id: EntityId }
  | { kind: "ambiguous"; ids: EntityId[] }
  | { kind: "missing" };

/** A link in a block: what is shown of it, and where it goes. */
export type ResolvedLink = LinkTarget & {
  /** What it is called, which is all that is shown of it. */
  name: string;
  /** The `#key(value)` qualifiers that narrow it, as written, or empty.
   * Shown on hover. */
  qualifiers: string;
};

/** One row of the list of every entity. */
export interface Summary extends EntityRef {
  /** Enough of its tags to tell it from others with the same title. */
  tags: string[];
  /** Words in its blocks, not counting its title or property lines. */
  words: number;
  /** Blocks in other entities that link to it. */
  incoming: number;
  /** Seconds since the Unix epoch. Null if it wasn't recorded, or the history
   * that says has been compacted away. */
  created: number | null;
  /** When a block of it was last added, moved or edited, as `created`. */
  modified: number | null;
  /** Whether it is a journal page. */
  journal: boolean;
}

/** Every entity, and which link to which: one edge per pair, however many links. */
export interface Connections {
  nodes: EntityRef[];
  /** `weak` ones are made by a property, between each end of it and the
   * entity that defines it, and are drawn lighter. */
  edges: { source: EntityId; target: EntityId; weak: boolean }[];
}

/** An entity that has a property, and the values it has it with. */
export interface Use {
  entity: EntityRef;
  values: string[];
}

export interface Entity extends EntityRef {
  /** False for journals and property entities: their date or name is what
   * identifies them. */
  renamable: boolean;
  blocks: Block[];
  /** Keyed by the text between a link's brackets, inherited values' included. */
  links: Record<string, ResolvedLink>;
  inherited: Inherited[];
  inverses: Inverse[];
  /** What doesn't fit a schema and has no chip to show on, in words. */
  flags: string[];
}

/** A file kept in the graph: what its block says about it. The bytes are not
 * in the graph; the shell has them, by `hash`. */
export interface BlobInfo {
  /** The BLAKE3 hash of the bytes, in hex. */
  hash: string;
  mime: string;
  /** In bytes. */
  size: number;
  name: string | null;
}

/** What typing Backspace at the start of a block did: joined it to another. */
export interface Merged {
  /** The block that took the text. */
  into: string;
  /** Where the joined text begins in it, in UTF-16 units: the caret. */
  at: number;
}

/** The blocks of one entity that mention another. */
export interface Reference {
  source: EntityRef;
  /** Each with its children, for context. */
  blocks: Block[];
  links: Record<string, ResolvedLink>;
}

/** An entity whose title matched a search. */
export interface TitleHit {
  entity: EntityRef;
  score: number;
  /** The matched text, as ascending `[start, end)` runs of UTF-16 units: in
   * `alias` if there is one, else in the title. */
  ranges: [number, number][];
  /** The alias that matched better than the title did, if one did. A link
   * to it is still written with the title. */
  alias: string | null;
  /** The entity's properties, each once, in the order written. */
  properties: { key: string; value: string | null }[];
  /** The `#key(value)` qualifiers that pick this entity out from others of
   * the same title, as written after the name. Empty if it needs none. */
  qualifiers: string;
  /** What to write between a link's brackets to reach this entity. */
  link: string;
}

/** A block whose text matched a search. */
export interface BlockHit {
  /** The entity the block is on. */
  entity: EntityRef;
  block: string;
  text: string;
  score: number;
  /** The words that matched, as ascending `[start, end)` runs of `text` in UTF-16 units. */
  ranges: [number, number][];
}
