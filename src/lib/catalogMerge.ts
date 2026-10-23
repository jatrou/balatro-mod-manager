import type { Mod } from "../stores/modStore";

type PersistedMod = Omit<Mod, "description" | "image" | "imageFallback">;

export function prepareModsForCache(mods: Mod[]): PersistedMod[] {
  return mods.map((mod) => ({
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
  return value
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
    });
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

/** Merge a complete, validated catalog refresh into cached display data. */
export function mergeAuthoritativeCatalog(
  existingMods: Mod[],
  incomingMods: Mod[],
): { mods: Mod[]; prunedCount: number } {
  const incoming = new Map<string, Mod>();
  for (const mod of incomingMods) incoming.set(mod.title, mod);

  const incomingOrder: string[] = [];
  const seen = new Set<string>();
  for (const mod of incomingMods) {
    if (seen.has(mod.title)) continue;
    seen.add(mod.title);
    incomingOrder.push(mod.title);
  }

  const existingByTitle = new Map(existingMods.map((mod) => [mod.title, mod]));
  const mods: Mod[] = [];
  let prunedCount = 0;

  for (const title of incomingOrder) {
    const incomingMod = incoming.get(title);
    if (!incomingMod) continue;
    const existing = existingByTitle.get(title);
    if (!existing) {
      mods.push(incomingMod);
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
    if (incoming.has(existing.title)) continue;
    if (existing._catalogEntry || existing._dirName) {
      prunedCount++;
    } else {
      mods.push(existing);
    }
  }

  return { mods, prunedCount };
}
