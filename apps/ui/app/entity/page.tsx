"use client";

import { MainEntity } from "@/components/main-entity";
import { useView } from "@/components/use-view";

export default function EntityPage() {
  const { main } = useView();
  if (!main) {
    return <p className="mx-auto max-w-measure-text px-6 py-8">No entity chosen.</p>;
  }
  return <MainEntity id={main} />;
}
