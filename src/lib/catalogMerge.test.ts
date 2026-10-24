import { describe, expect, it } from "vitest";
import type { InstalledMod, Mod } from "../stores/modStore";
import {
  catalogIdFromBmiUrl,
  hydrateModsFromCache,
  mergeAuthoritativeCatalog,
  prepareModsForCache,
  syncInstalledCatalogStatus,
  ensureInstalledModCards,
  deduplicateMods,
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
  it("keeps earliest-match ordering when the ID and title indexes find different rows", () => {
    const rows = deduplicateMods([
      mod("Shared", {
        id: "first",
        repo: "https://github.com/one/mod",
        categories: [0],
      }),
      mod("Other", {
        id: "second",
        repo: "https://github.com/two/mod",
        categories: [1],
      }),
      mod("Shared", {
        id: "second",
        repo: "https://github.com/one/mod",
        categories: [2],
      }),
    ]);
    expect(rows.map((entry) => entry.categories)).toEqual([[0, 2], [1]]);
  });

  it("refreshes candidate indexes after a live alias changes the retained title", () => {
    const rows = deduplicateMods([
      mod("Old title", { id: "author@mod", categories: [0] }),
      mod("New title", {
        id: "author@mod",
        _catalogEntry: true,
        downloadURL: "bmi://author@mod",
        categories: [1],
      }),
      mod("New title", { id: "different-id", categories: [2] }),
      mod("Old title", { id: "new-old-title", categories: [3] }),
    ]);
    expect(rows.map((entry) => entry.title)).toEqual([
      "New title",
      "Old title",
    ]);
    expect(rows.map((entry) => entry.categories)).toEqual([[0, 1, 2], [3]]);
  });

  it("combines aliases and categories without dropping the live installer or installed path", () => {
    const rows = deduplicateMods([
      mod(" Example_Mod ", {
        repo: "https://github.com/Author/Example.git/",
        _catalogEntry: true,
        categories: [0],
        description: "Cached details",
      }),
      mod("Example Mod", {
        repo: "https://github.com/author/example",
        _catalogEntry: true,
        categories: [1],
        _dirName: "author@example",
        downloadURL: "bmi://author@example",
      }),
      mod("example-mod", {
        repo: "",
        publisher: "Installed",
        installed: true,
        _installedPath: "C:/Mods/Example",
      }),
    ]);
    expect(rows).toHaveLength(1);
    expect(rows[0]).toMatchObject({
      title: "Example Mod",
      categories: [0, 1],
      description: "Cached details",
      downloadURL: "bmi://author@example",
      _dirName: "author@example",
      installed: true,
      _installedPath: "C:/Mods/Example",
    });
  });

  it("keeps distinct same-name sources, collection children, and physical installations", () => {
    const rows = [
      mod("Same title", { repo: "https://github.com/one/mod" }),
      mod("Same title", { repo: "https://github.com/two/mod" }),
      mod("Child A", { repo: "https://github.com/one/collection" }),
      mod("Child B", { repo: "https://github.com/one/collection" }),
      mod("Scoped", { repo: "https://github.com/one/collection#first" }),
      mod("Scoped", { repo: "https://github.com/one/collection#second" }),
      mod("Copy", { _installedPath: "C:/Mods/one" }),
      mod("Copy", { _installedPath: "C:/Mods/two" }),
      mod("Offline", { repo: "", publisher: "one" }),
      mod("Offline", { repo: "", publisher: "two" }),
    ];
    expect(deduplicateMods(rows)).toEqual(rows);
  });

  it("migrates duplicate old cache rows and tolerates partial cache metadata", () => {
    const hydrated = hydrateModsFromCache([
      { title: "Example", publisher: "Tester" },
      { title: "EXAMPLE", publisher: "Tester" },
    ]);
    expect(hydrated).toHaveLength(1);
    expect(hydrated[0].downloadURL).toBe("");
    expect(prepareModsForCache([mod("Example"), mod("EXAMPLE")])).toHaveLength(
      1,
    );
  });

  it("reconciles authoritative aliases by source and preserves the installation identity", () => {
    const result = mergeAuthoritativeCatalog(
      [
        mod("Example-Mod", {
          _catalogEntry: true,
          installed: true,
          _installedPath: "C:/Mods/Example",
          _dirName: "author@example",
        }),
      ],
      [
        mod("Example Mod", {
          _catalogEntry: true,
          _dirName: "author@example",
          downloadURL: "bmi://author@example",
          categories: [0],
        }),
        mod("example_mod", {
          _catalogEntry: true,
          _dirName: "author@example",
          downloadURL: "bmi://author@example",
          categories: [1],
        }),
      ],
    );
    expect(result.prunedCount).toBe(0);
    expect(result.mods).toHaveLength(1);
    expect(result.mods[0]).toMatchObject({
      installed: true,
      _installedPath: "C:/Mods/Example",
      categories: [0, 1],
    });
    expect(
      ensureInstalledModCards(
        result.mods,
        [{ name: "Example-Mod", path: "C:/Mods/Example" }],
        (record) => mod(record.name),
      ),
    ).toHaveLength(1);
  });

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
