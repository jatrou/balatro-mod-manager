import { canonicalGitHubRepository } from "./repositoryMetadata";

/** Formatting differences are aliases; retain non-Latin letters and numbers. */
export function normalizedModTitle(value: string): string {
  return value
    .normalize("NFKC")
    .toLowerCase()
    .replace(/[^\p{L}\p{N}]+/gu, "");
}

/** Preserve file/subfolder scope: a repository can contain many distinct mods. */
export function modSourceIdentity(value: string): string {
  const repository = canonicalGitHubRepository(value);
  try {
    const url = new URL(value.trim());
    const path = url.pathname.replace(/\/+$/, "");
    if (
      repository &&
      path.split("/").filter(Boolean).length === 2 &&
      !url.hash &&
      !url.search
    ) {
      return `github:${repository}`;
    }
    url.hostname = url.hostname.toLowerCase();
    url.pathname = path;
    return url.toString().replace(/\/$/, "");
  } catch {
    return repository ? `github:${repository}` : value.trim();
  }
}
