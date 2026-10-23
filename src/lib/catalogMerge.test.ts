import { describe, expect, it } from "vitest";
import type { Mod } from "../stores/modStore";
import {
  catalogIdFromBmiUrl,
  hydrateModsFromCache,
  mergeAuthoritativeCatalog,
  prepareModsForCache,
} from "./catalogMerge";

const mod = (title: string, overrides: Partial<Mod> = {}): Mod => ({
  title,
  description: "",
  image: "/images/cover.jpg",
  categories: [],
  colors: { color1: "#000", color2: "#fff" },
  requires_steamodded: true,
  requires_talisman: false,
  publisher: "Tester",
  repo: "https://example.test/mod",
  downloadURL: "",
  installed: false,
  last_updated: 0,
  ...overrides,
});

describe("authoritative catalog reconciliation", () => {
  it("hydrates BMI provenance and removes cached B when live catalog only has A", () => {
    const cachedAId = catalogIdFromBmiUrl("bmi://Author@A");
    const cachedBId = catalogIdFromBmiUrl("bmi://Author@B");
    const cached = [
      mod("A", { id: cachedAId, _dirName: cachedAId, _catalogEntry: true }),
      mod("B", { id: cachedBId, _dirName: cachedBId, _catalogEntry: true }),
    ];
    const live = [
      mod("A", {
        id: "Author@A",
        _dirName: "Author@A",
        _catalogEntry: true,
        downloadURL: "bmi://Author@A",
      }),
    ];

    const result = mergeAuthoritativeCatalog(cached, live);

    expect(result.mods.map((entry) => entry.title)).toEqual(["A"]);
    expect(result.mods[0]).toMatchObject({
      _dirName: "Author@A",
      downloadURL: "bmi://Author@A",
    });
    expect(result.prunedCount).toBe(1);
  });

  it("prunes legacy catalog rows without inventing an id from folderName", () => {
    const cached = mod("Removed", {
      folderName: "ambiguous-local-folder",
      _catalogEntry: true,
    });

    expect(catalogIdFromBmiUrl("https://example.test/mod.zip")).toBeUndefined();
    expect(catalogIdFromBmiUrl("bmi://../escape")).toBeUndefined();
    expect(mergeAuthoritativeCatalog([cached], [])).toEqual({
      mods: [],
      prunedCount: 1,
    });
  });

  it("preserves non-catalog local placeholders", () => {
    const local = mod("Local only", { installed: true });
    expect(mergeAuthoritativeCatalog([local], [])).toEqual({
      mods: [local],
      prunedCount: 0,
    });
  });

  it("preserves catalog provenance through localStorage save and reload", () => {
    const catalog = mod("Catalog", {
      _catalogEntry: true,
      _dirName: "Author@Catalog",
      downloadURL: "bmi://Author@Catalog",
    });
    const local = mod("Local only", {
      publisher: "Installed",
      installed: true,
      _installedPath: "C:\\Balatro\\Mods\\LocalOnly",
    });

    const serialized = JSON.stringify(prepareModsForCache([catalog, local]));
    const hydrated = hydrateModsFromCache(JSON.parse(serialized));

    expect(hydrated[0]).toMatchObject({
      title: "Catalog",
      _catalogEntry: true,
      _dirName: "Author@Catalog",
      downloadURL: "",
    });
    expect(hydrated[1]).toMatchObject({
      title: "Local only",
      _catalogEntry: false,
      _installedPath: "C:\\Balatro\\Mods\\LocalOnly",
    });
  });

  it("migrates an unmarked legacy catalog row without reclassifying local placeholders", () => {
    const hydrated = hydrateModsFromCache([
      {
        ...mod("Legacy catalog"),
        _catalogEntry: undefined,
        _dirName: undefined,
      },
      {
        ...mod("Legacy local"),
        publisher: "Installed",
        repo: "",
        _installedPath: "C:\\Balatro\\Mods\\LegacyLocal",
        _catalogEntry: undefined,
        _dirName: undefined,
      },
    ]);

    expect(hydrated.map((entry) => entry._catalogEntry)).toEqual([true, false]);
    expect(mergeAuthoritativeCatalog(hydrated, [])).toMatchObject({
      mods: [{ title: "Legacy local" }],
      prunedCount: 1,
    });
  });
});
