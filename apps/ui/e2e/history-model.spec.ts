import { expect, test } from "@playwright/test";
import { PlaceHistory, type Place } from "../lib/history";

const at = (main: string | null): Place => ({ path: main ? "/" : "/journals/", main });

function visited(...mains: (string | null)[]) {
  const history = new PlaceHistory();
  for (const m of mains) history.observe(at(m));
  return history;
}

test("back and forward walk what was visited, and going elsewhere forgets forward", () => {
  const history = visited("a", "b", "c");
  expect(history.back()).toEqual(at("b"));
  expect(history.back()).toEqual(at("a"));
  expect(history.back()).toBeUndefined();
  expect(history.forward()).toEqual(at("b"));
  history.observe(at("d"));
  expect(history.forward()).toBeUndefined();
  expect(history.back()).toEqual(at("b"));
});

// The pane is seen after it renders, which can be after the next key press.
test("a late sighting of the place just left doesn't forget what was forward", () => {
  const history = visited("a", "b", "c");
  expect(history.back()).toEqual(at("b"));
  // The second step is taken before "b" has been seen, so "c" is seen late.
  expect(history.back()).toEqual(at("a"));
  history.observe(at("c"));
  history.observe(at("b"));
  history.observe(at("a"));
  expect(history.forward()).toEqual(at("b"));
  expect(history.forward()).toEqual(at("c"));
});

test("once the place stepped to is seen, the place left is an ordinary visit again", () => {
  const history = visited("a", "b");
  expect(history.back()).toEqual(at("a"));
  history.observe(at("a"));
  history.observe(at("b"));
  expect(history.forward()).toBeUndefined();
  expect(history.back()).toEqual(at("a"));
});
