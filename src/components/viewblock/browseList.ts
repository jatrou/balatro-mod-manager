export type PageSize = 10 | 25 | 50 | 100 | "all";

export function pageCount(count: number, size: PageSize): number {
  return size === "all" ? 1 : Math.max(1, Math.ceil(count / size));
}

/** Clamp before slicing so removing the final item on a page cannot hide the list. */
export function pageItems<T>(items: T[], page: number, size: PageSize): T[] {
  if (size === "all") return items;
  const safePage = Math.min(Math.max(1, page), pageCount(items.length, size));
  return items.slice((safePage - 1) * size, safePage * size);
}
