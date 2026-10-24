import type { AwesomeBadge } from "./awesomeCatalog";

/** Upstream badges describe source status, not the result of local checks. */
export function isAwesomeCompatibilityWarning(badge: AwesomeBadge): boolean {
  return /\b(?:experimental|buggy|laggy|mod conflicts?|crashes?|patch in works|needs (?:an? )?api rewrite)\b/i.test(
    badge.label,
  );
}
