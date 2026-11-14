"use client";

import { MainEntity } from "./main-entity";
import { useView } from "./use-view";

/** The main entity, wherever its address came from: an ID or a page's name. */
export function EntityPage() {
  const { main } = useView();
  if (!main) {
    return <p className="mx-auto max-w-measure-text px-6 py-8">No entity chosen.</p>;
  }
  return <MainEntity id={main} />;
}
