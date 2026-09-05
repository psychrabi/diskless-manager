import { describe, it, expect } from "vitest";
import { formatDiskBytes } from "./formatDiskBytes";

describe("disk total formatting", () => {
  it.each([[undefined, "-"], [null, "-"], [NaN, "-"], [-1, "-"], [0, "0 B"], [1024, "1.0 KB"], [1610612736, "1.5 GB"]])("formats %s as %s", (input, expected) => {
    expect(formatDiskBytes(input)).toBe(expected);
  });
});
