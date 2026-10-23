import { describe, expect, it } from "vitest";
import { pageCount, pageItems } from "./browseList";

describe("mod list pagination", () => {
  const mixed = Array.from({ length: 37 }, (_, i) => ({
    kind: i < 13 ? "local" : "catalog",
    id: i,
  }));
  it("caps a mixed installed page across local and catalog sections", () => {
    const page = pageItems(mixed, 2, 10);
    expect(page.map((item) => item.id)).toEqual([
      10, 11, 12, 13, 14, 15, 16, 17, 18, 19,
    ]);
    expect(page.filter((item) => item.kind === "local")).toHaveLength(3);
    expect(page.filter((item) => item.kind === "catalog")).toHaveLength(7);
    expect(pageCount(mixed.length, 10)).toBe(4);
  });
  it("shows every result in all mode regardless of previous page", () => {
    expect(pageItems(mixed, 4, "all")).toEqual(mixed);
    expect(pageCount(mixed.length, "all")).toBe(1);
  });
  it("clamps pages after a filter or uninstall shrinks the result set", () => {
    expect(pageItems(mixed.slice(0, 12), 4, 10).map((item) => item.id)).toEqual(
      [10, 11],
    );
    expect(pageItems([], 4, 10)).toEqual([]);
    expect(pageCount(0, 10)).toBe(1);
  });
});
