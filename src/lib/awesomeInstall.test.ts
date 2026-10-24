import { describe, expect, it } from "vitest";
import type { Mod } from "../stores/modStore";
import type { AwesomeEntry } from "./awesomeCatalog";
import { getAwesomeInstallSource } from "./awesomeInstall";

function entry(overrides: Partial<AwesomeEntry> = {}): AwesomeEntry {
  return {
    id: "sample",
    title: "Sample Mod",
    description: "",
    author: "",
    sourceUrl: "https://github.com/example/sample-mod",
    links: [],
    categories: [],
    loader: "Steamodded",
    kind: "mod",
    badges: [],
    sourceLines: [1],
    ...overrides,
  };
}

function mod(overrides: Partial<Mod> = {}): Mod {
  return {
    title: "Sample Mod",
    description: "",
    image: "",
    categories: [],
    colors: { color1: "#000", color2: "#fff" },
    requires_steamodded: true,
    requires_talisman: false,
    publisher: "example",
    repo: "https://github.com/example/sample-mod",
    downloadURL: "bmi://example-sample-mod",
    installed: false,
    last_updated: 0,
    ...overrides,
  };
}

describe("Awesome install source", () => {
  it("uses the exact indexed source only for a matching repository and title", () => {
    expect(getAwesomeInstallSource(entry(), mod())).toEqual({
      kind: "download",
      source: "indexed",
      url: "bmi://example-sample-mod",
      discordUrl: undefined,
    });
    expect(
      getAwesomeInstallSource(
        entry(),
        mod({ repo: "https://github.com/other/repo" }),
      ),
    ).toMatchObject({ kind: "download", source: "github" });
    expect(
      getAwesomeInstallSource(entry(), mod({ orphaned: true })),
    ).toMatchObject({ kind: "download", source: "github" });
  });

  it.each([
    "https://github.com/Steamodded/examples/blob/master/Mods/AbsoluteDeck.lua",
    "https://github.com/jie65535/JMods/blob/main/JCursor/JCursor.lua",
    "https://github.com/MathIsFun0/JensBalatroCollection/tree/main/Aurinko",
    "https://github.com/WilsontheWolf/balatro-mods/tree/master/steamodded/Rumble",
  ])(
    "keeps a GitHub file or subfolder for backend resolution: %s",
    (sourceUrl) => {
      expect(getAwesomeInstallSource(entry({ sourceUrl }))).toMatchObject({
        kind: "download",
        source: "github",
        url: sourceUrl,
      });
    },
  );

  it.each([
    "https://raw.githubusercontent.com/Steamodded/examples/master/Mods/AbsoluteDeck.lua",
    "https://github.com/example/repo/releases/download/v1.0/mod.zip",
    "https://example.com/mod.tar.gz?download=1",
  ])("accepts a direct Lua or supported archive URL: %s", (sourceUrl) => {
    expect(getAwesomeInstallSource(entry({ sourceUrl }))).toMatchObject({
      kind: "download",
      source: "direct",
      url: sourceUrl,
    });
  });

  it("uses an alternate download link while retaining the primary Discord post", () => {
    const discordUrl = "https://discord.com/channels/1/2";
    const sourceUrl = "https://github.com/example/sample-mod/tree/main/Mod";
    expect(
      getAwesomeInstallSource(
        entry({
          sourceUrl: discordUrl,
          links: [
            { label: "Discord", url: discordUrl },
            { label: "GitHub", url: sourceUrl },
          ],
        }),
      ),
    ).toEqual({
      kind: "download",
      source: "github",
      url: sourceUrl,
      discordUrl,
    });
  });

  it("routes Discord-only entries to a local file or folder selection", () => {
    const sourceUrl = "https://discord.com/channels/1/2";
    expect(getAwesomeInstallSource(entry({ sourceUrl }))).toMatchObject({
      kind: "local",
      sourceUrl,
      discordUrl: sourceUrl,
    });
  });

  it("preserves Discord invite links for the download workflow", () => {
    const sourceUrl = "https://discord.gg/balatro-mods";
    expect(getAwesomeInstallSource(entry({ sourceUrl }))).toMatchObject({
      kind: "local",
      discordUrl: sourceUrl,
    });
  });

  it("does not install a whole GitHub collection when a README anchor identifies one mod", () => {
    const sourceUrl =
      "https://github.com/nicholassam6425/balatro-mods?tab=readme-ov-file#coupon-book-modsp_coupon_booklua";
    expect(getAwesomeInstallSource(entry({ sourceUrl }))).toMatchObject({
      kind: "local",
      sourceUrl,
    });
    expect(
      getAwesomeInstallSource(
        entry({
          sourceUrl:
            "https://github.com/Steamodded/examples/blob/main/Mods/Foo.lua#L2",
        }),
      ),
    ).toMatchObject({ kind: "download", source: "github" });
  });

  it.each([
    "https://github.com/example/repo/releases/tag/v1.0",
    "https://github.com/example/repo/blob/main/README.md",
    "https://github.com/example/repo/issues/1",
    "https://nexusmods.com/balatro/mods/11",
    "http://example.com/mod.zip",
    "https://127.0.0.1/mod.zip",
    "https://[::1]/mod.zip",
    "https://user:password@example.com/mod.zip",
  ])(
    "does not mistake a page or unsafe URL for a download: %s",
    (sourceUrl) => {
      expect(getAwesomeInstallSource(entry({ sourceUrl }))).toMatchObject({
        kind: "local",
        sourceUrl,
      });
    },
  );

  it("requires individual selection for a child whose link identifies a parent pack", () => {
    expect(
      getAwesomeInstallSource(
        entry({ title: "Child", parentTitle: "Parent pack" }),
      ),
    ).toMatchObject({
      kind: "local",
      sourceUrl: "https://github.com/example/sample-mod",
    });
  });

  it.each([
    ["JokerDisplay", "nh6574/JokerDisplay"],
    ["Other Mod", "nh6574/other-mod"],
    ["JoyousSpring", "example/JoyousSpring"],
  ])(
    "ignores old author bans for %s even on direct entry objects",
    (title, repo) => {
      const sourceUrl = `https://github.com/${repo}`;
      const candidate = entry({
        title,
        sourceUrl,
        safetyWarning: "This author withdrew the mod from BMM.",
      });
      expect(getAwesomeInstallSource(candidate)).toMatchObject({
        kind: "download",
        source: "github",
        url: sourceUrl,
      });
      expect(
        getAwesomeInstallSource(candidate, mod({ title, repo: sourceUrl })),
      ).toMatchObject({ kind: "download", source: "indexed" });
    },
  );

  it("keeps special layouts and unsupported loaders out of one-click install", () => {
    const candidate = entry({
      title: "Multiplayer",
      sourceUrl: "https://github.com/Balatro-Multiplayer/BalatroMultiplayer",
    });
    expect(getAwesomeInstallSource(candidate)).toMatchObject({
      kind: "unsupported",
      reason: expect.stringContaining(
        "installation layout BMM does not support",
      ),
    });
    for (const loader of ["Balamod", "None"] as const) {
      expect(getAwesomeInstallSource(entry({ loader }), mod())).toMatchObject({
        kind: "unsupported",
      });
    }
    expect(
      getAwesomeInstallSource(entry({ kind: "loader" }), mod()),
    ).toMatchObject({
      kind: "unsupported",
    });
  });
});
