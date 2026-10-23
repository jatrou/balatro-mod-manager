import { beforeEach, describe, expect, it, vi } from "vitest";
import { isKnownMissingSource } from "./repositoryMetadata";
import type { Mod } from "../stores/modStore";
import {
  bundledAwesomeCatalog,
  canonicalGitHubRepository,
  findCatalogMatch,
  filterUnavailableAwesomeEntries,
  loadAwesomeCatalog,
  parseAwesomeBalatroMarkdown,
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
      entryCount: 314,
    });
    expect(mods).toHaveLength(310);
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

  it("never maps withdrawn or unsupported entries to the install catalog", () => {
    const entries = parseAwesomeBalatroMarkdown(`
## Mods (Require [**Lovely**](https://example.test))
### GUI
- [JokerDisplay](https://github.com/nh6574/JokerDisplay) - Display.
- [Multiplayer](https://github.com/Balatro-Multiplayer/BalatroMultiplayer) - Multiplayer.
`).entries;
    expect(entries.every((entry) => entry.safetyWarning)).toBe(true);
    expect(
      findCatalogMatch(entries[0], [
        mod({
          title: "JokerDisplay",
          repo: "https://github.com/nh6574/JokerDisplay",
        }),
      ]),
    ).toBeNull();
  });

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
    ).toHaveLength(314);
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
