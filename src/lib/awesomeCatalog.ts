import snapshot from "./data/awesome-balatro.json";
import type { Mod } from "../stores/modStore";

export const AWESOME_BALATRO_REPOSITORY = "jie65535/awesome-balatro";
export const AWESOME_BALATRO_README_URL =
  "https://github.com/jie65535/awesome-balatro/blob/main/README.md";
export const AWESOME_BALATRO_RAW_URL =
  "https://raw.githubusercontent.com/jie65535/awesome-balatro/main/README.md";
export const AWESOME_BALATRO_LICENSE_URL =
  "https://github.com/jie65535/awesome-balatro/blob/main/LICENSE";

const CACHE_KEY = "awesome-balatro-catalog-v1";
const MINIMUM_EXPECTED_MODS = 200;

export type AwesomeLoader = "Steamodded" | "Balamod" | "Lovely" | "None";
export type AwesomeEntryKind = "mod" | "loader";

export interface AwesomeBadge {
  label: string;
  url: string;
}

export interface AwesomeLink {
  label: string;
  url: string;
}

export interface AwesomeEntry {
  id: string;
  title: string;
  description: string;
  author: string;
  sourceUrl: string;
  links: AwesomeLink[];
  categories: string[];
  loader: AwesomeLoader;
  kind: AwesomeEntryKind;
  badges: AwesomeBadge[];
  sourceLines: number[];
  parentTitle?: string;
  safetyWarning?: string;
}

export interface AwesomeCatalogSource {
  repository: string;
  readmeUrl: string;
  rawUrl: string;
  license: "CC0-1.0";
  licenseUrl: string;
  revision: string;
  fetchedAt: string;
  entryCount: number;
}

export interface AwesomeCatalog {
  source: AwesomeCatalogSource;
  entries: AwesomeEntry[];
}

export interface AwesomeCatalogLoadResult {
  catalog: AwesomeCatalog;
  origin: "live" | "cache" | "snapshot";
  warning?: string;
}

export interface AwesomeCatalogLoadOptions {
  refresh?: boolean;
  fetchImpl?: typeof fetch;
  timeoutMs?: number;
}

function stripMarkdown(value: string): string {
  return value
    .replace(/<img\b[^>]*>/gi, " ")
    .replace(/\[([^\]]+)]\s*\([^)]+\)/g, "$1")
    .replace(/[*_`]/g, "")
    .replace(/<[^>]+>/g, " ")
    .replace(/\s+/g, " ")
    .trim();
}

function normalizeKey(value: string): string {
  return value.toLowerCase().replace(/[^a-z0-9]+/g, "");
}

function makeEntryId(
  title: string,
  sourceUrl: string,
  loader: AwesomeLoader,
): string {
  const url = sourceUrl
    .toLowerCase()
    .replace(/^https?:\/\//, "")
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-|-$/g, "");
  return `${loader.toLowerCase()}-${normalizeKey(title)}-${url}`;
}

function badgeLabel(url: string, fallback = "Status"): string {
  try {
    const pathname = new URL(url).pathname;
    const marker = "/badge/";
    const start = pathname.indexOf(marker);
    if (start < 0) return fallback;
    const encoded = pathname.slice(start + marker.length);
    const lastDash = encoded.lastIndexOf("-");
    const text = lastDash > 0 ? encoded.slice(0, lastDash) : encoded;
    return decodeURIComponent(text).replace(/--/g, "-").replace(/-/g, " ");
  } catch {
    return fallback;
  }
}

function parseBadges(line: string): AwesomeBadge[] {
  const badges: AwesomeBadge[] = [];
  const imagePattern = /<img\b([^>]*)>/gi;
  for (const image of line.matchAll(imagePattern)) {
    const attributes = image[1];
    const source = attributes.match(/\bsrc=["']([^"']+)["']/i)?.[1];
    if (!source) continue;
    const alt = attributes.match(/\balt=["']([^"']+)["']/i)?.[1];
    badges.push({
      label: stripMarkdown(alt || badgeLabel(source)),
      url: source,
    });
  }
  return badges;
}

function parseLeadingLinks(
  body: string,
  descriptionStart: number,
): AwesomeLink[] {
  const prefix = descriptionStart >= 0 ? body.slice(0, descriptionStart) : body;
  const links: AwesomeLink[] = [];
  const linkPattern = /\[([^\]]+)]\s*\((https?:\/\/[^)\s]+)\)/gi;
  for (const link of prefix.matchAll(linkPattern)) {
    links.push({ label: stripMarkdown(link[1]), url: link[2] });
  }
  return links;
}

function safetyWarning(title: string, sourceUrl: string) {
  const normalizedTitle = normalizeKey(title);
  const normalizedUrl = sourceUrl.toLowerCase();
  if (
    normalizedTitle === "jokerdisplay" ||
    normalizedUrl.includes("github.com/nh6574/")
  ) {
    return "This author withdrew the mod from BMM. The source is shown for reference only and cannot be installed through BMM.";
  }
  if (normalizedTitle.includes("joyousspring")) {
    return "Joyous Spring was withdrawn from BMM. The source is shown for reference only and cannot be installed through BMM.";
  }
  if (
    normalizedTitle === "multiplayer" ||
    normalizedTitle === "balatromultiplayer" ||
    normalizedUrl.includes("balatro-multiplayer/balatromultiplayer")
  ) {
    return "Balatro Multiplayer uses an installation layout BMM does not support. Follow its source instructions manually.";
  }
  return undefined;
}

function loaderForHeading(heading: string): AwesomeLoader | null {
  if (/^Mods \(Require Steamodded\)/i.test(heading)) return "Steamodded";
  if (/^Mods \(Require Balamod\)/i.test(heading)) return "Balamod";
  if (/^Mods \(Require Lovely\)/i.test(heading)) return "Lovely";
  if (/^Mods \(Other\)/i.test(heading)) return "None";
  return null;
}

/** Parse the upstream README without assuming every link is an installable archive. */
export function parseAwesomeBalatroMarkdown(
  markdown: string,
  metadata: Partial<Pick<AwesomeCatalogSource, "revision" | "fetchedAt">> = {},
): AwesomeCatalog {
  let section = "";
  let category = "";
  let loader: AwesomeLoader | null = null;
  let parent: AwesomeEntry | null = null;
  const entries: AwesomeEntry[] = [];
  const dedupe = new Map<string, AwesomeEntry>();

  const addEntry = (entry: AwesomeEntry): AwesomeEntry => {
    const key = `${entry.title.toLowerCase()}|${entry.sourceUrl.toLowerCase()}|${entry.loader}`;
    const existing = dedupe.get(key);
    if (existing) {
      for (const item of entry.categories) {
        if (!existing.categories.includes(item)) existing.categories.push(item);
      }
      for (const line of entry.sourceLines) {
        if (!existing.sourceLines.includes(line))
          existing.sourceLines.push(line);
      }
      return existing;
    }
    dedupe.set(key, entry);
    entries.push(entry);
    return entry;
  };

  const lines = markdown.split(/\r?\n/);
  for (let index = 0; index < lines.length; index += 1) {
    const rawLine = lines[index];
    const levelTwo = rawLine.match(/^##\s+(.+)/);
    if (levelTwo) {
      section = stripMarkdown(levelTwo[1]);
      category = "";
      loader = loaderForHeading(section);
      parent = null;
      continue;
    }

    const levelThree = rawLine.match(/^###\s+(.+)/);
    if (levelThree) {
      category = stripMarkdown(levelThree[1]);
      parent = null;
      continue;
    }

    const bullet = rawLine.match(/^(\s*)-\s+(.+)$/);
    if (!bullet) continue;
    const indent = bullet[1].length;
    const body = bullet[2];
    const isSpeedupTool =
      section === "Tools" && /^\[Balatro Ultimate Speedup Mod]/i.test(body);
    const isLoaderEntry = section === "Mod Loaders";
    if (loader === null && !isSpeedupTool && !isLoaderEntry) continue;

    if (indent >= 2 && loader !== null && parent) {
      const separator = body.indexOf(" - ");
      const title = stripMarkdown(
        separator >= 0 ? body.slice(0, separator) : body,
      );
      if (!title) continue;
      const description = stripMarkdown(
        separator >= 0 ? body.slice(separator + 3) : `Part of ${parent.title}`,
      );
      addEntry({
        id: makeEntryId(title, parent.sourceUrl, loader),
        title,
        description,
        author: parent.author,
        sourceUrl: parent.sourceUrl,
        links: parent.links,
        categories: [category || "Uncategorized"],
        loader,
        kind: "mod",
        badges: parent.badges,
        sourceLines: [index + 1],
        parentTitle: parent.title,
        safetyWarning: safetyWarning(title, parent.sourceUrl),
      });
      continue;
    }

    // The optional whitespace deliberately accepts upstream's malformed
    // `[Redux-Arcanum] (url)` entry.
    const primary = body.match(/^\[([^\]]+)]\s*\((https?:\/\/[^)\s]+)\)/i);
    if (!primary) continue;
    const title = stripMarkdown(primary[1]);
    const sourceUrl = primary[2];
    const descriptionStart = body.indexOf(" - ", primary[0].length);
    let details = stripMarkdown(
      descriptionStart >= 0 ? body.slice(descriptionStart + 3) : "",
    );
    let author = "";
    const authorAt = details.toLowerCase().lastIndexOf(" by ");
    if (authorAt >= 0) {
      author = stripMarkdown(details.slice(authorAt + 4));
      details = details.slice(0, authorAt).trim();
    }

    const kind: AwesomeEntryKind = isLoaderEntry ? "loader" : "mod";
    const entryLoader: AwesomeLoader =
      isSpeedupTool || isLoaderEntry ? "None" : (loader as AwesomeLoader);
    const entry = addEntry({
      id: makeEntryId(title, sourceUrl, entryLoader),
      title,
      description: details,
      author,
      sourceUrl,
      links: parseLeadingLinks(body, descriptionStart),
      categories: [
        isSpeedupTool
          ? "Tools"
          : isLoaderEntry
            ? "Mod Loaders"
            : category || "Uncategorized",
      ],
      loader: entryLoader,
      kind,
      badges: parseBadges(body),
      sourceLines: [index + 1],
      safetyWarning: safetyWarning(title, sourceUrl),
    });
    parent = kind === "mod" ? entry : null;
  }

  return {
    source: {
      repository: AWESOME_BALATRO_REPOSITORY,
      readmeUrl: AWESOME_BALATRO_README_URL,
      rawUrl: AWESOME_BALATRO_RAW_URL,
      license: "CC0-1.0",
      licenseUrl: AWESOME_BALATRO_LICENSE_URL,
      revision: metadata.revision || "main (runtime)",
      fetchedAt: metadata.fetchedAt || new Date().toISOString(),
      entryCount: entries.length,
    },
    entries,
  };
}

function isCatalog(value: unknown): value is AwesomeCatalog {
  if (!value || typeof value !== "object") return false;
  const candidate = value as Partial<AwesomeCatalog>;
  return (
    Array.isArray(candidate.entries) &&
    candidate.entries.length >= MINIMUM_EXPECTED_MODS &&
    !!candidate.source &&
    candidate.source.repository === AWESOME_BALATRO_REPOSITORY
  );
}

function readCachedCatalog(): AwesomeCatalog | null {
  if (typeof window === "undefined") return null;
  try {
    const parsed: unknown = JSON.parse(
      localStorage.getItem(CACHE_KEY) || "null",
    );
    return isCatalog(parsed) ? parsed : null;
  } catch {
    return null;
  }
}

function writeCachedCatalog(catalog: AwesomeCatalog): void {
  if (typeof window === "undefined") return;
  try {
    localStorage.setItem(CACHE_KEY, JSON.stringify(catalog));
  } catch {
    // The bundled snapshot remains available if storage is unavailable/full.
  }
}

export const bundledAwesomeCatalog = snapshot as AwesomeCatalog;

export async function loadAwesomeCatalog(
  options: AwesomeCatalogLoadOptions = {},
): Promise<AwesomeCatalogLoadResult> {
  const cached = readCachedCatalog();
  if (!options.refresh) {
    return cached
      ? { catalog: cached, origin: "cache" }
      : { catalog: bundledAwesomeCatalog, origin: "snapshot" };
  }

  const fetchImpl = options.fetchImpl || fetch;
  const controller = new AbortController();
  const timeout = setTimeout(
    () => controller.abort(),
    options.timeoutMs ?? 8000,
  );
  try {
    const response = await fetchImpl(AWESOME_BALATRO_RAW_URL, {
      cache: "no-store",
      signal: controller.signal,
      headers: { Accept: "text/plain" },
    });
    if (!response.ok)
      throw new Error(`GitHub returned HTTP ${response.status}`);
    const markdown = await response.text();
    const catalog = parseAwesomeBalatroMarkdown(markdown);
    if (catalog.entries.length < MINIMUM_EXPECTED_MODS) {
      throw new Error(
        `Upstream list parsed only ${catalog.entries.length} entries; keeping the last known-good catalog`,
      );
    }
    writeCachedCatalog(catalog);
    return { catalog, origin: "live" };
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    return cached
      ? { catalog: cached, origin: "cache", warning: message }
      : {
          catalog: bundledAwesomeCatalog,
          origin: "snapshot",
          warning: message,
        };
  } finally {
    clearTimeout(timeout);
  }
}

export function canonicalGitHubRepository(value: string): string | null {
  const input = value.trim().replace(/^git\+/, "");
  if (!input) return null;
  const shorthand = input.match(/^@?([^/\s]+)\/([^/#?\s]+?)(?:\.git)?$/);
  if (shorthand) return `${shorthand[1]}/${shorthand[2]}`.toLowerCase();
  try {
    const url = new URL(input);
    if (url.hostname.toLowerCase() !== "github.com") return null;
    const [owner, repository] = url.pathname
      .replace(/^\/+|\/+$/g, "")
      .split("/");
    if (!owner || !repository) return null;
    return `${owner}/${repository.replace(/\.git$/i, "")}`.toLowerCase();
  } catch {
    return null;
  }
}

function isRootGitHubRepositoryUrl(value: string): boolean {
  try {
    const url = new URL(value);
    if (url.hostname.toLowerCase() !== "github.com" || url.search || url.hash) {
      return false;
    }
    const parts = url.pathname
      .replace(/^\/+|\/+$/g, "")
      .split("/")
      .filter(Boolean);
    return parts.length === 2;
  } catch {
    return false;
  }
}

/** Return an exact current-index match, or null when matching is ambiguous/unsafe. */
export function findCatalogMatch(
  entry: AwesomeEntry,
  mods: readonly Mod[],
): Mod | null {
  if (
    entry.kind !== "mod" ||
    entry.safetyWarning ||
    entry.loader === "Balamod" ||
    entry.loader === "None" ||
    !isRootGitHubRepositoryUrl(entry.sourceUrl)
  ) {
    return null;
  }
  const repository = canonicalGitHubRepository(entry.sourceUrl);
  if (!repository) return null;
  const title = normalizeKey(entry.title);
  const matches = mods.filter(
    (mod) =>
      !mod.orphaned &&
      mod.downloadURL.startsWith("bmi://") &&
      normalizeKey(mod.title) === title &&
      canonicalGitHubRepository(mod.repo) === repository,
  );
  return matches.length === 1 ? matches[0] : null;
}
