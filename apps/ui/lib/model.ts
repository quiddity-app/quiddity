export type EntityId = string;

export interface EntityRef {
  id: EntityId;
  title: string;
}

export interface Block {
  id: string;
  text: string;
  /** The properties written in `text`, in order, for rendering as chips. */
  properties: Property[];
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
  /** `#key(value)` within the text, rather than `key:: value` on a line. */
  inline: boolean;
  /** Where it is written, in UTF-16 units of the block's text. For a line
   * property, the whole line. */
  start: number;
  end: number;
  /** The entity that defines the property, if anyone has. */
  definition: EntityId | null;
  /** A `##key(parameter)` line, whose parameter is its value, as text. */
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
}

/** Every entity, and which link to which: one edge per pair, however many links. */
export interface Connections {
  nodes: EntityRef[];
  edges: { source: EntityId; target: EntityId }[];
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
  /** The matched text in the title, as ascending `[start, end)` runs of UTF-16 units. */
  ranges: [number, number][];
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
