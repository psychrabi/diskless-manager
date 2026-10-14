import { expect, it } from "vitest";
import { isSuperClient } from "./clientMode";

it("normalizes blank snapshots and pending assignments", () => {
  expect(isSuperClient({ master: "tank/windows", snapshot: "  " })).toBe(true);
  expect(isSuperClient({ master: "  PENDING  " })).toBe(false);
  expect(isSuperClient({ master: "tank/windows", snapshot: "tank/windows@clean" })).toBe(false);
});
