import { Replica } from "./wasm/quiddity_wasm";
import type { Backend } from "./graph";

interface Seed {
  text: string;
  children: Seed[];
}

const b = (text: string, ...children: Seed[]): Seed => ({ text, children });

const pad = (n: number) => String(n).padStart(2, "0");

function days(count: number): [string, Seed[]][] {
  return Array.from({ length: count }, (_, i) => {
    const d = new Date();
    d.setDate(d.getDate() - i);
    const date = `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
    return [
      date,
      [
        b(`type:: journal\ndate:: ${date}`),
        b(i % 3 === 0 ? "Worked on [[Quiddity]]." : "A quiet day."),
        ...(i % 5 === 0 ? [b("Read about loro again")] : []),
      ],
    ];
  });
}

const pages: [string, Seed[]][] = [
  [
    "Quiddity",
    [
      b("favourite:: true\ntype:: project"),
      b(
        "Local-first, multiplayer second brain.",
        b("Built on [[Loro]] and [[iroh]]."),
        b("Shift-click a link to open it at the side."),
      ),
      b("Open questions", b("Eviction of unsynced edits.")),
    ],
  ],
  [
    "Loro",
    [
      b("favourite:: true\nkind of:: CRDT"),
      b("A CRDT library. Documents are movable trees of text."),
    ],
  ],
  // A small taxonomy, for the meta-properties.
  [
    "kind of",
    [
      b("type:: property\n##transitive\n##inherits\n##inverse(kinds)"),
      b("What something is a kind of. It has that thing's properties too."),
    ],
  ],
  ["CRDT", [b("kind of:: data structure\nmerges:: without coordination")]],
  ["data structure", [b("#structure")]],
  ["iroh", [b("QUIC with hole punching, addressed by public key.")]],
  [
    "Reading list",
    [b("Designing Data-Intensive Applications"), b("Notes on loro and other CRDTs")],
  ],
];

function fill(replica: Replica, parent: string, seeds: Seed[]) {
  for (const seed of seeds) {
    fill(replica, replica.createBlock(parent, undefined, seed.text), seed.children);
  }
}

/** A new entity starts with an empty block, which the first seed takes over. */
function entity(replica: Replica, name: string, [first, ...rest]: Seed[]) {
  const id = replica.createEntity(name);
  const empty = replica.entity(id).blocks[0].id;
  replica.spliceText(empty, 0, 0, first?.text ?? "");
  fill(replica, empty, first?.children ?? []);
  fill(replica, id, rest);
}

/** A made-up graph, for working on the UI in a plain browser. */
function seeded(): Replica {
  const replica = Replica.empty();
  for (const [name, blocks] of [...pages, ...days(30)]) entity(replica, name, blocks);
  return replica;
}

export const memoryBackend: Backend = {
  async load() {
    return seeded().snapshot();
  },
  // Nothing outlives the page: this is for looking at the UI.
  async push() {},
};
