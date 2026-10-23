import type { Mod } from "../stores/modStore";
import type { AwesomeEntry } from "./awesomeCatalog";
import { canonicalGitHubRepository } from "./awesomeCatalog";

export type AwesomeInstallSource =
  | {
      kind: "download";
      url: string;
      source: "indexed" | "github" | "direct";
      discordUrl?: string;
    }
  | {
      kind: "local";
      sourceUrl: string;
      reason: string;
      discordUrl?: string;
    }
  | {
      kind: "unsupported";
      sourceUrl: string;
      reason: string;
      discordUrl?: string;
    };

type DownloadableSource = Extract<AwesomeInstallSource, { kind: "download" }>;

const INSTALLABLE_FILE = /\.(?:lua|zip|tar|tar\.gz|tgz)$/i;
const ARCHIVE_FILE = /\.(?:zip|tar|tar\.gz|tgz)$/i;
const GITHUB_SEGMENT = /^[a-z0-9_.-]+$/i;

function parsePublicHttpsUrl(value: string): URL | null {
  try {
    const url = new URL(value);
    if (
      url.protocol !== "https:" ||
      !url.hostname ||
      url.username ||
      url.password ||
      url.port ||
      url.hostname === "localhost" ||
      url.hostname === "localhost." ||
      url.hostname.endsWith(".localhost") ||
      url.hostname.startsWith("[") ||
      /^\d+\.\d+\.\d+\.\d+$/.test(url.hostname)
    ) {
      return null;
    }
    return url;
  } catch {
    return null;
  }
}

function isDiscord(url: URL): boolean {
  return [
    "discord.com",
    "www.discord.com",
    "canary.discord.com",
    "ptb.discord.com",
    "discordapp.com",
    "www.discordapp.com",
    "discord.gg",
    "www.discord.gg",
  ].includes(url.hostname.toLowerCase());
}

/** Only recognize URLs that identify a file or GitHub source the installer can resolve. */
function downloadableSource(value: string): DownloadableSource | null {
  const url = parsePublicHttpsUrl(value);
  if (!url) return null;
  const host = url.hostname.toLowerCase();
  const path = url.pathname;

  if (host === "github.com") {
    const segments = path.split("/").filter(Boolean);
    if (
      segments.length < 2 ||
      !GITHUB_SEGMENT.test(segments[0]) ||
      !GITHUB_SEGMENT.test(segments[1])
    ) {
      return null;
    }
    // README anchors can identify one child of a multi-mod repository. The
    // repository URL alone would install the wrong scope.
    if (
      url.hash &&
      !(segments[2] === "blob" && /^#L\d+(?:-L\d+)?$/i.test(url.hash))
    ) {
      return null;
    }
    if (segments.length === 2) {
      return { kind: "download", source: "github", url: url.origin + path };
    }
    if (
      segments[2] === "tree" &&
      segments.length >= 4 &&
      segments.slice(3).every((segment) => !/%2f|%5c/i.test(segment))
    ) {
      return { kind: "download", source: "github", url: url.origin + path };
    }
    if (
      segments[2] === "blob" &&
      segments.length >= 5 &&
      INSTALLABLE_FILE.test(segments.at(-1) || "") &&
      segments.slice(3).every((segment) => !/%2f|%5c/i.test(segment))
    ) {
      return { kind: "download", source: "github", url: url.origin + path };
    }
    if (
      segments[2] === "releases" &&
      segments[3] === "download" &&
      segments.length >= 6 &&
      ARCHIVE_FILE.test(segments.at(-1) || "")
    ) {
      return { kind: "download", source: "direct", url: value };
    }
    if (
      segments[2] === "releases" &&
      segments[3] === "latest" &&
      segments[4] === "download" &&
      segments.length >= 6 &&
      ARCHIVE_FILE.test(segments.at(-1) || "")
    ) {
      return { kind: "download", source: "direct", url: value };
    }
    if (
      segments[2] === "archive" &&
      segments.length >= 4 &&
      ARCHIVE_FILE.test(segments.at(-1) || "")
    ) {
      return { kind: "download", source: "direct", url: value };
    }
    return null;
  }

  if (host === "raw.githubusercontent.com") {
    const segments = path.split("/").filter(Boolean);
    if (
      segments.length >= 4 &&
      GITHUB_SEGMENT.test(segments[0]) &&
      GITHUB_SEGMENT.test(segments[1]) &&
      INSTALLABLE_FILE.test(segments.at(-1) || "")
    ) {
      return { kind: "download", source: "direct", url: value };
    }
    return null;
  }

  return INSTALLABLE_FILE.test(path)
    ? { kind: "download", source: "direct", url: value }
    : null;
}

function isExactIndexedMatch(entry: AwesomeEntry, mod: Mod): boolean {
  const normalizeTitle = (value: string) =>
    value.toLowerCase().replace(/[^a-z0-9]+/g, "");
  return (
    !mod.orphaned &&
    mod.downloadURL.startsWith("bmi://") &&
    entry.title.length > 0 &&
    normalizeTitle(mod.title) === normalizeTitle(entry.title) &&
    canonicalGitHubRepository(mod.repo) !== null &&
    canonicalGitHubRepository(mod.repo) ===
      canonicalGitHubRepository(entry.sourceUrl)
  );
}

/** Pick an installation route without treating a web page as an archive. */
export function getAwesomeInstallSource(
  entry: AwesomeEntry,
  matchedMod?: Mod | null,
): AwesomeInstallSource {
  const sourceUrl = entry.sourceUrl;
  const links = [sourceUrl, ...entry.links.map((link) => link.url)].filter(
    (value, index, all) => all.indexOf(value) === index,
  );
  const discordUrl = links.find((value) => {
    const url = parsePublicHttpsUrl(value);
    return url !== null && isDiscord(url);
  });

  if (entry.safetyWarning) {
    return {
      kind: "unsupported",
      sourceUrl,
      reason: entry.safetyWarning,
      discordUrl,
    };
  }
  if (entry.kind === "loader") {
    return {
      kind: "unsupported",
      sourceUrl,
      reason:
        "This is a mod loader. Install it using its own setup instructions.",
      discordUrl,
    };
  }
  if (entry.loader === "Balamod" || entry.loader === "None") {
    return {
      kind: "unsupported",
      sourceUrl,
      reason:
        entry.loader === "Balamod"
          ? "Balamod mods may need a different destination and setup. Follow the source instructions."
          : "This entry does not identify a supported mod loader or installation destination. Follow the source instructions.",
      discordUrl,
    };
  }
  if (matchedMod && isExactIndexedMatch(entry, matchedMod)) {
    return {
      kind: "download",
      source: "indexed",
      url: matchedMod.downloadURL,
      discordUrl,
    };
  }
  if (entry.parentTitle) {
    return {
      kind: "local",
      sourceUrl,
      reason: `This entry is part of ${entry.parentTitle}. Choose the individual downloaded file or folder from the parent package.`,
      discordUrl,
    };
  }

  for (const link of links) {
    const download = downloadableSource(link);
    if (download) return { ...download, discordUrl };
  }

  return {
    kind: "local",
    sourceUrl,
    reason: discordUrl
      ? "Download the mod from Discord, then choose the downloaded file or folder."
      : "This page does not provide a direct installable file or supported GitHub source. Download the mod, then choose its file or folder.",
    discordUrl,
  };
}
