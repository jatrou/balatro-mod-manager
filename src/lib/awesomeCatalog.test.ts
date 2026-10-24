import { beforeEach, describe, expect, it, vi } from "vitest";
import { getAwesomeInstallSource } from "./awesomeInstall";
import snapshot from "./data/awesome-balatro.json";
import { isKnownMissingSource } from "./repositoryMetadata";
import type { Mod } from "../stores/modStore";
import {
  bundledAwesomeCatalog,
  canonicalGitHubRepository,
  findCatalogMatch,
  filterUnavailableAwesomeEntries,
  loadAwesomeCatalog,
  parseAwesomeBalatroMarkdown,
  awesomeEntryForLoader,
  awesomeEntryLoaders,
  deduplicateAwesomeEntries,
} from "./awesomeCatalog";

const mod = (overrides: Partial<Mod> = {}): Mod => ({
  title: "Redux-Arcanum",
  description: "",
  image: "",
  categories: [],
  colors: { color1: "#000", color2: "#fff" },
  requires_steamodded: true,
  requires_talisman: false,
  publisher: "test",
  repo: "https://github.com/jumbocarrot0/Redux-Arcanum",
  downloadURL: "bmi://redux-arcanum",
  installed: false,
  last_updated: 0,
  ...overrides,
});

describe("Awesome Balatro catalog", () => {
  beforeEach(() => localStorage.clear());

  it("ships the verified upstream snapshot after removing unavailable sources", () => {
    const mods = bundledAwesomeCatalog.entries.filter(
      (entry) => entry.kind === "mod",
    );
    const loaders = bundledAwesomeCatalog.entries.filter(
      (entry) => entry.kind === "loader",
    );
    expect(bundledAwesomeCatalog.source).toMatchObject({
      revision: "1a0335a5834f3148cd24076983d65e6e2bd195c0",
      license: "CC0-1.0",
      entryCount: 306,
    });
    expect(mods).toHaveLength(302);
    expect(loaders).toHaveLength(4);
    expect(
      new Set(bundledAwesomeCatalog.entries.map((entry) => entry.id)).size,
    ).toBe(bundledAwesomeCatalog.entries.length);
    expect(
      bundledAwesomeCatalog.entries.some((entry) =>
        isKnownMissingSource(entry.sourceUrl),
      ),
    ).toBe(false);
  });

  it("parses malformed links, badges, secondary links, and grouped submods", () => {
    const markdown = `
## Mods (Require [**Steamodded**](https://github.com/Steamopollys/Steamodded))
### Consumable cards
- [Redux-Arcanum] (https://github.com/jumbocarrot0/Redux-Arcanum) | [discord](https://discord.test/thread) - tweaks of Codex-Arcanum. by [jumbocarrot0](https://github.com/jumbocarrot0/) <img src="https://img.shields.io/badge/occasional%20crashes-orange">
### Decks
- [Gimmicky decks pack](https://discord.test/pack) - A collection. by \`@Kusoro\`
  - Joker Deck - Start with two random Jokers.
`;
    const catalog = parseAwesomeBalatroMarkdown(markdown, {
      revision: "test-revision",
      fetchedAt: "2026-10-23T00:00:00.000Z",
    });

    expect(catalog.entries).toHaveLength(3);
    expect(catalog.entries[0]).toMatchObject({
      title: "Redux-Arcanum",
      loader: "Steamodded",
      categories: ["Consumable cards"],
      author: "jumbocarrot0",
    });
    expect(catalog.entries[0].links).toHaveLength(2);
    expect(catalog.entries[0].badges[0].label).toContain("occasional crashes");
    expect(catalog.entries[2]).toMatchObject({
      title: "Joker Deck",
      parentTitle: "Gimmicky decks pack",
      sourceUrl: "https://discord.test/pack",
    });
  });

  it("deduplicates identical title, URL, and loader while preserving categories", () => {
    const markdown = `
## Mods (Require [**Lovely**](https://example.test))
### GUI
- [Example](https://github.com/example/example) - One.
### Utils
- [Example](https://github.com/example/example) - One.
`;
    const { entries } = parseAwesomeBalatroMarkdown(markdown);
    expect(entries).toHaveLength(1);
    expect(entries[0].categories).toEqual(["GUI", "Utils"]);
    expect(entries[0].sourceLines).toEqual([4, 6]);
  });

  it("shows one row per loader variant while preserving each source and installation route", () => {
    const catalog = parseAwesomeBalatroMarkdown(`
## Mods (Require [**Balamod**](https://example.test))
### GUI
- [Deselect All](https://github.com/Zei33/deselect-all-balamod) - Legacy. by @Zei33
## Mods (Require [**Steamodded**](https://example.test))
### Utils
- [Deselect_All](https://github.com/Zei33/deselect-all-steamodded.git/) - Modern. by @Zei33
`);
    expect(catalog.entries).toHaveLength(1);
    const entry = catalog.entries[0];
    expect(entry.loader).toBe("Steamodded");
    expect(entry.categories).toEqual(["Utils", "GUI"]);
    expect(awesomeEntryLoaders(entry)).toEqual(["Steamodded", "Balamod"]);
    expect(getAwesomeInstallSource(entry)).toMatchObject({
      kind: "download",
      url: "https://github.com/Zei33/deselect-all-steamodded.git/",
    });
    const legacy = awesomeEntryForLoader(entry, "Balamod");
    expect(legacy.sourceUrl).toBe(
      "https://github.com/Zei33/deselect-all-balamod",
    );
    expect(legacy.id).toBe(entry.id);
    expect(getAwesomeInstallSource(legacy)).toMatchObject({
      kind: "unsupported",
    });
    expect(deduplicateAwesomeEntries(catalog.entries)).toEqual(catalog.entries);
  });

  it("keeps same-name different authors, source subpaths, and collection children distinct", () => {
    const catalog = parseAwesomeBalatroMarkdown(`
## Mods (Require [**Steamodded**](https://example.test))
- [Texture Pack](https://discord.test/first) - Textures. by @One
- [Texture Pack](https://discord.test/second) - Textures. by @Two
- [Collection](https://github.com/one/mods) - Collection. by @One
  - First Child - First.
  - Second Child - Second.
- [Scoped](https://github.com/one/mods#first) - First. by @One
- [Scoped](https://github.com/one/mods#second) - Second. by @One
`);
    expect(catalog.entries).toHaveLength(7);
  });

  it("combines the verified Parrot Joker alias and keeps unrelated same-title mods", () => {
    const parrots = bundledAwesomeCatalog.entries.filter(
      (entry) => entry.title === "Parrot Joker",
    );
    expect(parrots).toHaveLength(1);
    expect(awesomeEntryLoaders(parrots[0])).toEqual(["Lovely", "Balamod"]);
    expect(awesomeEntryForLoader(parrots[0], "Balamod").sourceUrl).toBe(
      "https://github.com/JopStro/BalatroMods",
    );
    expect(
      bundledAwesomeCatalog.entries.filter(
        (entry) => entry.title === "Gay Mod",
      ),
    ).toHaveLength(2);
    expect(
      bundledAwesomeCatalog.entries.filter(
        (entry) => entry.title === "Texture Pack",
      ),
    ).toHaveLength(2);
  });

  it("deduplicates legacy cached variants before an offline fallback and rewrites the cache", async () => {
    localStorage.setItem(
      "awesome-balatro-catalog-v1",
      JSON.stringify(snapshot),
    );
    const result = await loadAwesomeCatalog({
      refresh: true,
      fetchImpl: vi.fn().mockRejectedValue(new Error("offline")),
    });
    expect(result.origin).toBe("cache");
    expect(
      result.catalog.entries.filter((entry) => entry.title === "Deselect All"),
    ).toHaveLength(1);
    const rewritten = JSON.parse(
      localStorage.getItem("awesome-balatro-catalog-v1")!,
    );
    expect(rewritten.entries).toHaveLength(
      bundledAwesomeCatalog.entries.length,
    );
    expect(rewritten.source.entryCount).toBe(rewritten.entries.length);
  });

  it("matches only one exact title and canonical GitHub repository", () => {
    const entry = parseAwesomeBalatroMarkdown(`
## Mods (Require [**Steamodded**](https://example.test))
### Cards
- [Redux Arcanum](https://github.com/jumbocarrot0/Redux-Arcanum) - Test.
`).entries[0];
    expect(canonicalGitHubRepository(entry.sourceUrl)).toBe(
      "jumbocarrot0/redux-arcanum",
    );
    expect(
      findCatalogMatch(entry, [mod({ title: "Redux_Arcanum" })]),
    ).not.toBeNull();
    expect(
      findCatalogMatch(entry, [mod(), mod({ id: "duplicate" })]),
    ).toBeNull();
    expect(
      findCatalogMatch(entry, [mod({ repo: "https://github.com/other/repo" })]),
    ).toBeNull();
  });

  it("rejects stale, orphaned, subpath, and unsupported-loader matches", () => {
    const rootEntry = parseAwesomeBalatroMarkdown(`
## Mods (Require [**Steamodded**](https://example.test))
### Cards
- [Redux-Arcanum](https://github.com/jumbocarrot0/Redux-Arcanum) - Test.
`).entries[0];
    expect(findCatalogMatch(rootEntry, [mod({ downloadURL: "" })])).toBeNull();
    expect(findCatalogMatch(rootEntry, [mod({ orphaned: true })])).toBeNull();

    const fileEntry = {
      ...rootEntry,
      sourceUrl:
        "https://github.com/jumbocarrot0/Redux-Arcanum/blob/main/mod.lua",
    };
    expect(findCatalogMatch(fileEntry, [mod()])).toBeNull();

    const balamodEntry = { ...rootEntry, loader: "Balamod" as const };
    expect(findCatalogMatch(balamodEntry, [mod()])).toBeNull();
  });

  it.each([
    ["JokerDisplay", "nh6574/JokerDisplay"],
    ["Other Mod", "nh6574/other-mod"],
    ["Joyous Spring", "example/JoyousSpring"],
  ])("allows %s regardless of author or past disputes", (title, repo) => {
    const entry = parseAwesomeBalatroMarkdown(`
## Mods (Require [**Lovely**](https://example.test))
### GUI
- [${title}](https://github.com/${repo}) - Display.
`).entries[0];
    const matched = mod({ title, repo: `https://github.com/${repo}` });
    expect(entry.safetyWarning).toBeUndefined();
    expect(findCatalogMatch(entry, [matched])).toBe(matched);
    expect(
      findCatalogMatch(
        { ...entry, safetyWarning: "This author withdrew the mod from BMM." },
        [matched],
      ),
    ).toBe(matched);
    expect(getAwesomeInstallSource(entry, matched)).toMatchObject({
      kind: "download",
      source: "indexed",
    });
  });

  it("retains the unsupported Multiplayer installation layout restriction", () => {
    const entry = parseAwesomeBalatroMarkdown(`
## Mods (Require [**Lovely**](https://example.test))
- [Multiplayer](https://github.com/Balatro-Multiplayer/BalatroMultiplayer) - Multiplayer.
`).entries[0];
    expect(entry.safetyWarning).toContain(
      "installation layout BMM does not support",
    );
    expect(
      findCatalogMatch(entry, [
        mod({ title: entry.title, repo: entry.sourceUrl }),
      ]),
    ).toBeNull();
    expect(
      getAwesomeInstallSource({ ...entry, safetyWarning: undefined }),
    ).toMatchObject({ kind: "unsupported" });
  });

  it("ships JokerDisplay without a serialized author restriction", () => {
    for (const catalog of [snapshot, bundledAwesomeCatalog]) {
      const entry = catalog.entries.find(
        (candidate) => candidate.title === "JokerDisplay",
      );
      expect(entry).toBeDefined();
      expect(entry).not.toHaveProperty("safetyWarning", expect.any(String));
    }
  });

  it.each([false, true])(
    "migrates cached author bans without resetting the cache (refresh: %s)",
    async (refresh) => {
      const additions = parseAwesomeBalatroMarkdown(`
## Mods (Require [**Lovely**](https://example.test))
- [Other Mod](https://github.com/nh6574/other-mod) - Display.
- [Joyous Spring](https://github.com/example/JoyousSpring) - Cards.
- [Multiplayer](https://github.com/Balatro-Multiplayer/BalatroMultiplayer) - Multiplayer.
`).entries;
      const restrictedTitles = [
        "JokerDisplay",
        "Other Mod",
        "Joyous Spring",
        "Multiplayer",
      ];
      const entries = [...bundledAwesomeCatalog.entries, ...additions].map(
        (entry) => ({
          ...entry,
          ...(restrictedTitles.includes(entry.title)
            ? {
                safetyWarning:
                  "This author withdrew the mod from BMM. The source is shown for reference only.",
              }
            : {}),
        }),
      );
      localStorage.setItem(
        "awesome-balatro-catalog-v1",
        JSON.stringify({ ...bundledAwesomeCatalog, entries }),
      );
      const result = await loadAwesomeCatalog({
        refresh,
        fetchImpl: vi.fn().mockRejectedValue(new Error("offline")),
      });
      expect(result.origin).toBe("cache");
      for (const title of restrictedTitles.slice(0, -1)) {
        const entry = result.catalog.entries.find(
          (candidate) => candidate.title === title,
        )!;
        const matched = mod({ title, repo: entry.sourceUrl });
        expect(entry.safetyWarning).toBeUndefined();
        expect(findCatalogMatch(entry, [matched])).toBe(matched);
        expect(getAwesomeInstallSource(entry, matched)).toMatchObject({
          kind: "download",
          source: "indexed",
        });
      }
      const multiplayer = result.catalog.entries.find(
        (entry) => entry.title === "Multiplayer",
      )!;
      expect(multiplayer.safetyWarning).toContain(
        "installation layout BMM does not support",
      );
      const rewritten = JSON.parse(
        localStorage.getItem("awesome-balatro-catalog-v1")!,
      );
      expect(
        rewritten.entries.some((entry: { safetyWarning?: string }) =>
          /withdrew|withdrawn/i.test(entry.safetyWarning || ""),
        ),
      ).toBe(false);
      expect(rewritten.entries).toHaveLength(entries.length);
    },
  );

  it("falls back to the bundled snapshot after a failed live refresh", async () => {
    const fetchImpl = vi.fn().mockRejectedValue(new Error("offline"));
    const result = await loadAwesomeCatalog({ refresh: true, fetchImpl });
    expect(result.origin).toBe("snapshot");
    expect(result.catalog.entries.length).toBeGreaterThan(200);
    expect(result.warning).toContain("offline");
  });

  it("removes missing file links without removing their available repository", () => {
    const parsed = parseAwesomeBalatroMarkdown(`
## Mods (Require [**Steamodded**](https://example.test))
### Decks
- [Dead deck](https://github.com/Steamodded/examples/blob/master/Mods/AbsoluteDeck.lua) - Missing file.
- [Examples](https://github.com/Steamodded/examples) | [old file](https://github.com/Steamodded/examples/blob/master/Mods/AbsoluteDeck.lua) - Available repository.
`);
    const filtered = filterUnavailableAwesomeEntries(parsed);
    expect(filtered.entries.map((entry) => entry.title)).toEqual(["Examples"]);
    expect(filtered.entries[0].links).toHaveLength(1);
    expect(filtered.source.entryCount).toBe(1);
  });

  it("cleans old caches so a refresh failure cannot resurrect missing entries", async () => {
    const dead = parseAwesomeBalatroMarkdown(`
## Mods (Require [**Steamodded**](https://example.test))
- [Missing](https://github.com/mwithington/SilverSeal) - Removed repository.
`).entries[0];
    localStorage.setItem(
      "awesome-balatro-catalog-v1",
      JSON.stringify({
        ...bundledAwesomeCatalog,
        entries: [...bundledAwesomeCatalog.entries, dead],
      }),
    );
    const result = await loadAwesomeCatalog({
      refresh: true,
      fetchImpl: vi.fn().mockRejectedValue(new Error("offline")),
    });
    expect(result.origin).toBe("cache");
    expect(
      result.catalog.entries.some((entry) => entry.title === "Missing"),
    ).toBe(false);
    expect(
      JSON.parse(localStorage.getItem("awesome-balatro-catalog-v1")!).entries,
    ).toHaveLength(bundledAwesomeCatalog.entries.length);
  });

  it("filters confirmed missing entries from a successful upstream refresh", async () => {
    const markdown =
      `## Mods (Require [**Steamodded**](https://example.test))\n` +
      Array.from(
        { length: 201 },
        (_, index) =>
          `- [Test ${index}](https://github.com/example/mod${index}) - Available or unchecked.`,
      ).join("\n") +
      "\n- [Missing](https://github.com/mwithington/SilverSeal) - Missing.";
    const result = await loadAwesomeCatalog({
      refresh: true,
      fetchImpl: vi.fn().mockResolvedValue(new Response(markdown)),
    });
    expect(result.origin).toBe("live");
    expect(result.catalog.entries).toHaveLength(201);
    expect(
      result.catalog.entries.some((entry) => entry.title === "Missing"),
    ).toBe(false);
  });
});
