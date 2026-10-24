import { describe, expect, it } from "vitest";
import type { InstalledMod, Mod } from "../stores/modStore";
import {
  catalogIdFromBmiUrl,
  hydrateModsFromCache,
  mergeAuthoritativeCatalog,
  prepareModsForCache,
  syncInstalledCatalogStatus,
  ensureInstalledModCards,
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

  it("clears a stale missing-catalog flag when an authoritative row reappears", () => {
    const result = mergeAuthoritativeCatalog(
      [mod("Present", { _catalogEntry: true, orphaned: true })],
      [mod("Present", { _catalogEntry: true })],
    );
    expect(result.mods[0].orphaned).toBe(false);
  });

  it("synchronizes installed catalog status in both directions without changing unrelated rows", () => {
    const unrelated = mod("Unrelated", { orphaned: true });
    const result = syncInstalledCatalogStatus(
      [
        mod("Reappeared", { orphaned: true }),
        mod("Outside catalog", { orphaned: false }),
        unrelated,
      ],
      [
        { name: "reappeared", path: "C:/Mods/Reappeared", orphaned: false },
        { name: "Outside catalog", path: "C:/Mods/Outside", orphaned: true },
      ],
    );
    expect(result.map((item) => item.orphaned)).toEqual([false, true, true]);
    expect(result[2]).toBe(unrelated);
  });

  it("adds all 27 DB installations to a populated 24-row primary catalog exactly once", () => {
    const primary = Array.from({ length: 24 }, (_, index) =>
      mod(`Primary ${index}`, { _catalogEntry: true }),
    );
    const installed = [
      ...primary.map((item) => ({
        name: item.title,
        path: `C:/Mods/${item.title}`,
        orphaned: false,
      })),
      ...["Deselect All", "JokerDisplay", "Multiplayer"].map((name) => ({
        name,
        path: `C:/Mods/${name}`,
        orphaned: true,
      })),
    ];
    const createPlaceholder = (record: InstalledMod) =>
      mod(record.name, {
        installed: true,
        _installedPath: record.path,
        _catalogEntry: false,
        orphaned: record.orphaned,
      });
    const seeded = ensureInstalledModCards(
      primary,
      installed,
      createPlaceholder,
    );
    expect(seeded).toHaveLength(27);
    expect(new Set(seeded.map((item) => item.title)).size).toBe(27);
    expect(
      seeded.filter((item) => item.orphaned).map((item) => item.title),
    ).toEqual(["Deselect All", "JokerDisplay", "Multiplayer"]);
    const refreshed = mergeAuthoritativeCatalog(seeded, primary).mods;
    const seededAgain = ensureInstalledModCards(
      refreshed,
      installed,
      createPlaceholder,
    );
    expect(seededAgain).toHaveLength(27);
    expect(new Set(seededAgain.map((item) => item.title)).size).toBe(27);
  });

  it("recreates an installed cached catalog row pruned by a background refresh", () => {
    const existing = [
      mod("Present", { _catalogEntry: true, _dirName: "author@present" }),
      mod("JokerDisplay", {
        _catalogEntry: true,
        _dirName: "author@joker",
        orphaned: true,
        installed: true,
      }),
    ];
    const installed: InstalledMod[] = [
      { name: "JokerDisplay", path: "C:/Mods/JokerDisplay", orphaned: true },
    ];
    const incoming = [
      mod("Present", { _catalogEntry: true, _dirName: "author@present" }),
    ];
    const merged = mergeAuthoritativeCatalog(existing, incoming);
    expect(merged.prunedCount).toBe(1);
    const restored = ensureInstalledModCards(merged.mods, installed, (record) =>
      mod(record.name, {
        installed: true,
        _installedPath: record.path,
        _catalogEntry: false,
      }),
    );
    expect(restored).toHaveLength(2);
    expect(
      restored.find((item) => item.title === "JokerDisplay"),
    ).toMatchObject({
      installed: true,
      _catalogEntry: false,
      _installedPath: "C:/Mods/JokerDisplay",
      orphaned: true,
    });
    expect(
      restored.find((item) => item.title === "JokerDisplay")?._dirName,
    ).toBeUndefined();
    expect(mergeAuthoritativeCatalog(restored, incoming).mods).toHaveLength(2);
  });

  it("preserves membership flags when a compact installed summary omits them", () => {
    const existing = mod("Installed", { orphaned: true, installed: true });
    expect(
      syncInstalledCatalogStatus(
        [existing],
        [{ name: "Installed", path: "C:/Mods/Installed" }],
      )[0],
    ).toBe(existing);
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
