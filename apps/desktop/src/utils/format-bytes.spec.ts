import { describe, expect, it } from "vitest";

import { formatBytes } from "@/utils/format-bytes";

describe("formatBytes", () => {
  it("formats common dashboard capacities", () => {
    expect(formatBytes(40.43 * 1024 ** 3)).toBe("40.43 GB");
    expect(formatBytes(454 * 1024 ** 2, 0)).toBe("454 MB");
  });

  it("rejects invalid capacities", () => {
    expect(() => formatBytes(-1)).toThrow(RangeError);
    expect(() => formatBytes(Number.NaN)).toThrow(RangeError);
  });
});
