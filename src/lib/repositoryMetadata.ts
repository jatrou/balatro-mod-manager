import snapshot from "./data/repository-metadata.json";

export type LinkStatus = "available" | "missing" | "restricted" | "unknown";
export interface RepositoryRecord {
  status: LinkStatus;
  stars: number | null;
  canonicalRepository?: string;
  httpStatus?: number;
}
export interface RepositoryMetadata {
  checkedAt: string;
  repositories: Record<string, RepositoryRecord>;
  links: Record<string, { status: LinkStatus; httpStatus?: number }>;
}

const checkedMetadata = snapshot as RepositoryMetadata;
export const repositoryMetadata: RepositoryMetadata = {
  ...checkedMetadata,
  repositories: {
    ...Object.fromEntries(
      Object.values(checkedMetadata.repositories)
        .filter(
          (record) =>
            record.status === "available" && record.canonicalRepository,
        )
        .map((record) => [record.canonicalRepository!, record]),
    ),
    ...checkedMetadata.repositories,
  },
};

/** Accept GitHub repositories, subpaths, archive URLs and owner/repo shorthand. */
export function canonicalGitHubRepository(value: string): string | null {
  const input = value.trim().replace(/^git\+/, "");
  const shorthand = input.match(/^@?([\w.-]+)\/([\w.-]+?)(?:\.git)?$/);
  if (shorthand) return `${shorthand[1]}/${shorthand[2]}`.toLowerCase();
  try {
    const url = new URL(input);
    if (
      !["http:", "https:"].includes(url.protocol) ||
      url.hostname.toLowerCase() !== "github.com"
    )
      return null;
    const [owner, repo] = url.pathname.replace(/^\/+/, "").split("/");
    if (!owner || !repo || !/^[\w.-]+$/.test(owner) || !/^[\w.-]+$/.test(repo))
      return null;
    return `${owner}/${repo.replace(/\.git$/i, "")}`.toLowerCase();
  } catch {
    return null;
  }
}

export function normalizedSourceUrl(value: string): string {
  try {
    const url = new URL(value);
    url.hash = "";
    url.hostname = url.hostname.toLowerCase();
    url.pathname = url.pathname.replace(/\/+$/, "");
    return url.toString().replace(/\/$/, "");
  } catch {
    return value.trim();
  }
}

export function isKnownMissingSource(
  value: string,
  metadata = repositoryMetadata,
): boolean {
  const repo = canonicalGitHubRepository(value);
  return (
    metadata.links[normalizedSourceUrl(value)]?.status === "missing" ||
    (!!repo && metadata.repositories[repo]?.status === "missing")
  );
}

function repositoryUrl(value: string): string | null {
  const repo = canonicalGitHubRepository(value);
  if (!repo || repositoryMetadata.repositories[repo]?.status === "missing")
    return null;
  return `https://github.com/${repositoryMetadata.repositories[repo]?.canonicalRepository || repo}`;
}

/** Repository comes from the catalog's explicit source, never its download counter. */
export function getModRepository(mod: { repo?: string }): string | null {
  return repositoryUrl(mod.repo || "");
}

export function getAwesomeRepository(entry: {
  sourceUrl: string;
  links: { url: string }[];
}): string | null {
  return (
    repositoryUrl(entry.sourceUrl) ||
    entry.links.map((link) => repositoryUrl(link.url)).find(Boolean) ||
    null
  );
}

/** Unknown counts remain null; a checked repository with zero stars returns zero. */
export function getRepositoryStars(
  repoOrUrl: string | null = null,
  metadata = repositoryMetadata,
): number | null {
  const repo = canonicalGitHubRepository(repoOrUrl || "");
  if (!repo) return null;
  const record = metadata.repositories[repo];
  return record?.status === "available" ? record.stars : null;
}
