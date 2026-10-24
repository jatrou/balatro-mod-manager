import type { InstalledMod, Mod } from "../stores/modStore";
import { modSourceIdentity, normalizedModTitle } from "./modIdentity";

type PersistedMod = Omit<Mod, "description" | "image" | "imageFallback">;

export function prepareModsForCache(mods: Mod[]): PersistedMod[] {
  return deduplicateMods(mods).map((mod) => ({
    title: mod.title,
    categories: mod.categories,
    colors: mod.colors,
    requires_steamodded: mod.requires_steamodded,
    requires_talisman: mod.requires_talisman,
    publisher: mod.publisher,
    repo: mod.repo,
    downloadURL: "",
    folderName: mod.folderName ?? null,
    version: mod.version ?? null,
    installed: mod.installed,
    last_updated: mod.last_updated,
    _dirName: mod._dirName,
    _catalogEntry: mod._catalogEntry === true,
    _installedPath: mod._installedPath,
    _hasThumbnail: mod._hasThumbnail ?? true,
    orphaned: mod.orphaned,
    id: mod.id,
    downloads_total: mod.downloads_total,
  }));
}

export function hydrateModsFromCache(value: unknown): Mod[] {
  if (!Array.isArray(value)) return [];
  return deduplicateMods(
    value
      .filter(
        (entry): entry is Record<string, unknown> =>
          typeof entry === "object" &&
          entry !== null &&
          typeof entry.title === "string",
      )
      .map((entry) => {
        const explicitCatalogEntry = entry._catalogEntry;
        const clearlyLocalPlaceholder =
          typeof entry._installedPath === "string" ||
          (entry.publisher === "Installed" &&
            !entry.repo &&
            typeof entry._dirName !== "string");
        return {
          ...entry,
          description:
            typeof entry.description === "string" ? entry.description : "",
          image:
            typeof entry.image === "string" ? entry.image : "/images/cover.jpg",
          downloadURL: "",
          _catalogEntry:
            typeof explicitCatalogEntry === "boolean"
              ? explicitCatalogEntry
              : !clearlyLocalPlaceholder,
        } as Mod;
      }),
  );
}

function catalogIdentity(mod: Mod) {
  return mod._dirName || catalogIdFromBmiUrl(mod.downloadURL) || mod.id;
}

function sameMod(left: Mod, right: Mod): boolean {
  if (
    left._installedPath &&
    right._installedPath &&
    left._installedPath !== right._installedPath
  )
    return false;
  const leftId = catalogIdentity(left);
  const rightId = catalogIdentity(right);
  if (leftId && rightId && leftId === rightId) return true;
  if (normalizedModTitle(left.title) !== normalizedModTitle(right.title))
    return false;
  const leftSource = modSourceIdentity(left.repo || "");
  const rightSource = modSourceIdentity(right.repo || "");
  if (leftSource && rightSource) return leftSource === rightSource;
  // Local placeholders carry no source. Conflicting explicit IDs still identify
  // different catalog entries, even if they happen to share a display name.
  const isPlaceholder = (mod: Mod) =>
    Boolean(mod._installedPath) || mod.publisher === "Installed";
  return (
    !(leftId && rightId && leftId !== rightId) &&
    (isPlaceholder(left) ||
      isPlaceholder(right) ||
      (Boolean(left.publisher?.trim()) &&
        normalizedModTitle(left.publisher) ===
          normalizedModTitle(right.publisher || "")))
  );
}

function combineDuplicateMods(left: Mod, right: Mod): Mod {
  // Prefer a live catalog identity over a display-only cache or local placeholder.
  const preferred =
    (!left.downloadURL && right.downloadURL) ||
    (!left._catalogEntry && right._catalogEntry)
      ? right
      : left;
  const other = preferred === left ? right : left;
  const hasImage = (mod: Mod) =>
    Boolean(mod.image?.trim()) &&
    !/\bimages\/cover\.jpg$/i.test(mod.image.trim());
  const imageMod = hasImage(preferred) ? preferred : other;
  return {
    ...other,
    ...preferred,
    categories: [
      ...new Set([...(left.categories || []), ...(right.categories || [])]),
    ],
    description: preferred.description?.trim()
      ? preferred.description
      : other.description,
    image: imageMod.image,
    imageFallback: imageMod.imageFallback,
    _hasThumbnail: imageMod._hasThumbnail,
    id: catalogIdentity(preferred) || catalogIdentity(other),
    _dirName: preferred._dirName || other._dirName,
    _catalogEntry: left._catalogEntry || right._catalogEntry,
    _installedPath: left._installedPath || right._installedPath,
    installed: left.installed || right.installed,
    requires_steamodded: left.requires_steamodded || right.requires_steamodded,
    requires_talisman: left.requires_talisman || right.requires_talisman,
  };
}

/** One display row per proven identity, without merging different install paths. */
export function deduplicateMods(mods: readonly Mod[]): Mod[] {
  const result: Mod[] = [];
  const byId = new Map<string, Set<number>>();
  const byTitle = new Map<string, Set<number>>();
  const updateIndex = (
    lookup: Map<string, Set<number>>,
    key: string,
    index: number,
    add: boolean,
  ) => {
    if (add) {
      const indexes = lookup.get(key) || new Set<number>();
      indexes.add(index);
      lookup.set(key, indexes);
    } else {
      lookup.get(key)?.delete(index);
    }
  };
  const updateIndexes = (mod: Mod, index: number, add: boolean) => {
    const id = catalogIdentity(mod);
    if (id) updateIndex(byId, id, index, add);
    updateIndex(byTitle, normalizedModTitle(mod.title), index, add);
  };
  for (const mod of mods) {
    const id = catalogIdentity(mod);
    const candidates = new Set([
      ...(id ? byId.get(id) || [] : []),
      ...(byTitle.get(normalizedModTitle(mod.title)) || []),
    ]);
    // Keep the original first-match behavior when ID and title identify
    // different candidates; the full guard still checks source and install path.
    const index = [...candidates]
      .sort((left, right) => left - right)
      .find((candidate) => sameMod(result[candidate], mod));
    if (index === undefined) {
      const addedIndex = result.length;
      result.push(mod);
      updateIndexes(mod, addedIndex, true);
    } else {
      updateIndexes(result[index], index, false);
      result[index] = combineDuplicateMods(result[index], mod);
      updateIndexes(result[index], index, true);
    }
  }
  return result;
}

export function catalogIdFromBmiUrl(downloadUrl?: string) {
  if (!downloadUrl?.startsWith("bmi://")) return undefined;
  const id = downloadUrl.slice("bmi://".length);
  if (
    id.length === 0 ||
    id === "." ||
    id === ".." ||
    id.includes("/") ||
    id.includes("\\") ||
    Array.from(id).some((character) => {
      const codePoint = character.codePointAt(0);
      return codePoint !== undefined && (codePoint <= 31 || codePoint === 127);
    })
  ) {
    return undefined;
  }
  return id;
}

/** Installed records are authoritative for both setting and clearing membership flags. */
export function syncInstalledCatalogStatus(
  mods: Mod[],
  installed: readonly InstalledMod[],
): Mod[] {
  const byTitle = new Map(
    installed.map((item) => [normalizedModTitle(item.name), item]),
  );
  return mods.map((mod) => {
    const record = byTitle.get(normalizedModTitle(mod.title));
    return record && typeof record.orphaned === "boolean"
      ? { ...mod, orphaned: record.orphaned }
      : mod;
  });
}

/** Keep every DB installation visible even when a populated primary catalog omits it. */
export function ensureInstalledModCards(
  mods: Mod[],
  installed: readonly InstalledMod[],
  createPlaceholder: (installed: InstalledMod) => Mod,
): Mod[] {
  mods = deduplicateMods(mods);
  const titles = new Set(mods.map((mod) => normalizedModTitle(mod.title)));
  const additions: Mod[] = [];
  for (const record of installed) {
    const title = normalizedModTitle(record.name);
    if (titles.has(title)) continue;
    titles.add(title);
    additions.push(createPlaceholder(record));
  }
  return syncInstalledCatalogStatus([...additions, ...mods], installed);
}

/** Merge a complete, validated catalog refresh into cached display data. */
export function mergeAuthoritativeCatalog(
  existingMods: Mod[],
  incomingMods: Mod[],
): { mods: Mod[]; prunedCount: number } {
  incomingMods = deduplicateMods(incomingMods);
  existingMods = deduplicateMods(existingMods);
  const mods: Mod[] = [];
  let prunedCount = 0;

  for (const incomingMod of incomingMods) {
    const existing = existingMods.find((mod) => sameMod(mod, incomingMod));
    if (!existing) {
      mods.push({ ...incomingMod, orphaned: false });
      continue;
    }

    const keepExistingImage =
      Boolean(existing.image) &&
      existing.image.trim().length > 0 &&
      !/\bimages\/cover\.jpg$/i.test(existing.image.trim());
    const preferExistingDescription =
      (existing.description?.trim().length ?? 0) > 0;
    mods.push({
      ...existing,
      ...incomingMod,
      installed: existing.installed || incomingMod.installed,
      _installedPath: existing._installedPath || incomingMod._installedPath,
      orphaned: false,
      requires_steamodded: incomingMod.requires_steamodded,
      requires_talisman: incomingMod.requires_talisman,
      description: preferExistingDescription
        ? existing.description
        : (incomingMod.description ?? ""),
      colors: existing.colors,
      image: keepExistingImage ? existing.image : incomingMod.image,
      imageFallback: keepExistingImage
        ? existing.imageFallback
        : incomingMod.imageFallback,
    });
  }

  for (const existing of existingMods) {
    if (incomingMods.some((incoming) => sameMod(existing, incoming))) continue;
    if (existing._catalogEntry || existing._dirName) {
      prunedCount++;
    } else {
      mods.push(existing);
    }
  }

  return { mods, prunedCount };
}
