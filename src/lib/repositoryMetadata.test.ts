import { describe, expect, it } from "vitest";
import {
  canonicalGitHubRepository,
  getAwesomeRepository,
  getModRepository,
  getRepositoryStars,
  isKnownMissingSource,
  normalizedSourceUrl,
  repositoryMetadata,
  type RepositoryMetadata,
} from "./repositoryMetadata";

describe("repository metadata", () => {
  it("normalizes repository URLs without accepting lookalike hosts", () => {
    expect(
      canonicalGitHubRepository("git+https://github.com/Owner/Mod.git"),
    ).toBe("owner/mod");
    expect(canonicalGitHubRepository("Owner/Mod")).toBe("owner/mod");
    expect(
      canonicalGitHubRepository(
        "https://github.com/Owner/Mod/blob/main/mod.lua",
      ),
    ).toBe("owner/mod");
    expect(
      canonicalGitHubRepository("https://github.com.evil.test/Owner/Mod"),
    ).toBeNull();
    expect(
      canonicalGitHubRepository("javascript://github.com/owner/mod"),
    ).toBeNull();
    expect(canonicalGitHubRepository("https://github.com/owner")).toBeNull();
    expect(canonicalGitHubRepository("bmi://owner/mod")).toBeNull();
  });

  it("keeps a real zero separate from an unknown or missing count", () => {
    const metadata: RepositoryMetadata = {
      checkedAt: "test",
      links: {},
      repositories: {
        "owner/zero": { status: "available", stars: 0 },
        "owner/missing": { status: "missing", stars: null },
        "owner/rate-limited": {
          status: "unknown",
          stars: null,
          httpStatus: 429,
        },
      },
    };
    expect(getRepositoryStars("https://github.com/owner/zero", metadata)).toBe(
      0,
    );
    expect(getRepositoryStars("owner/missing", metadata)).toBeNull();
    expect(getRepositoryStars("owner/rate-limited", metadata)).toBeNull();
    expect(getRepositoryStars("owner/new", metadata)).toBeNull();
  });

  it("uses moved repositories' canonical URL and the same checked stars", () => {
    expect(
      getModRepository({ repo: "https://github.com/Steamopollys/Steamodded" }),
    ).toBe("https://github.com/steamodded/smods");
    expect(getRepositoryStars("steamopollys/steamodded")).toBe(
      getRepositoryStars(getModRepository({ repo: "steamopollys/steamodded" })),
    );
    expect(getRepositoryStars("steamodded/smods")).toBeGreaterThan(0);
  });

  it("finds a repository secondary link while preserving non-GitHub sources", () => {
    expect(
      getAwesomeRepository({
        sourceUrl: "https://discord.com/channels/example",
        links: [{ url: "https://github.com/stupxd/Cartomancer" }],
      }),
    ).toBe("https://github.com/stupxd/cartomancer");
    expect(getModRepository({ repo: "https://example.test/mod" })).toBeNull();
  });

  it("prunes only confirmed missing URLs and repos, and allows revived records", () => {
    const url = "https://github.com/owner/mod/blob/main/missing.lua";
    const metadata: RepositoryMetadata = {
      checkedAt: "test",
      repositories: {},
      links: {
        [normalizedSourceUrl(url)]: { status: "missing", httpStatus: 404 },
        "https://discord.com/channels/example": { status: "restricted" },
        "https://example.test/transient": {
          status: "unknown",
          httpStatus: 503,
        },
      },
    };
    expect(isKnownMissingSource(`${url}#L5`, metadata)).toBe(true);
    expect(isKnownMissingSource("https://github.com/owner/mod", metadata)).toBe(
      false,
    );
    expect(
      isKnownMissingSource("https://discord.com/channels/example", metadata),
    ).toBe(false);
    expect(
      isKnownMissingSource("https://example.test/transient", metadata),
    ).toBe(false);
    metadata.links[normalizedSourceUrl(url)] = {
      status: "available",
      httpStatus: 200,
    };
    expect(isKnownMissingSource(url, metadata)).toBe(false);
  });

  it("ships timestamps, real GitHub counts and no missing-source entries in the snapshot", () => {
    expect(Number.isFinite(Date.parse(repositoryMetadata.checkedAt))).toBe(
      true,
    );
    for (const record of Object.values(repositoryMetadata.repositories)) {
      if (record.status === "available") {
        expect(Number.isInteger(record.stars)).toBe(true);
        expect(record.stars).toBeGreaterThanOrEqual(0);
      } else expect(record.stars).toBeNull();
    }
  });
});
