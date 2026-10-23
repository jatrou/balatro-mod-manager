/** Run with `bun scripts/refresh-catalog.ts`. Requires an authenticated gh CLI.
 * Credentials stay in memory and are sent only to api.github.com.
 * Every original README source is rechecked, so revived links can return.
 */
import { execFileSync } from "node:child_process";
import { readFile, writeFile } from "node:fs/promises";
import { parseAwesomeBalatroMarkdown } from "../src/lib/awesomeCatalog";
import {
  canonicalGitHubRepository,
  normalizedSourceUrl,
  isKnownMissingSource,
  type LinkStatus,
  type RepositoryMetadata,
  type RepositoryRecord,
} from "../src/lib/repositoryMetadata";

const ROOT = new URL("../", import.meta.url);
const DATA = new URL("src/lib/data/", ROOT);
const AWESOME = "jie65535/awesome-balatro";
const PRIMARY = "kasimeka/balatro-mod-index";
const CONCURRENCY = 12;
const token = execFileSync("gh", ["auth", "token"], {
  encoding: "utf8",
  stdio: ["ignore", "pipe", "pipe"],
}).trim();
const checkedAt = new Date().toISOString();
type Check = { status: LinkStatus; httpStatus?: number };
interface GitHubRepository {
  private: boolean;
  stargazers_count: number;
  full_name: string;
}
interface GitHubReadme {
  content: string;
  sha: string;
}
interface GitHubTree {
  sha: string;
  truncated: boolean;
  tree: { path: string; type: string }[];
}
const previous = JSON.parse(
  await readFile(new URL("repository-metadata.json", DATA), "utf8"),
) as RepositoryMetadata;

async function request<T = unknown>(
  url: string,
  json = true,
): Promise<{ status: number; data: T | null }> {
  const headers: Record<string, string> = {
    "User-Agent": "BMM-public-catalog-health",
    Accept: json ? "application/vnd.github+json" : "*/*",
  };
  if (new URL(url).hostname === "api.github.com")
    headers.Authorization = `Bearer ${token}`;
  const response = await fetch(url, {
    headers,
    signal: AbortSignal.timeout(15000),
    redirect: "follow",
  });
  const data = json ? ((await response.json()) as T) : null;
  if (!json) await response.body?.cancel();
  return { status: response.status, data };
}

async function requiredJson<T>(url: string): Promise<T> {
  const result = await request<T>(url);
  if (result.status !== 200)
    throw new Error(
      `Required public catalog request returned HTTP ${result.status}: ${url}`,
    );
  return result.data as T;
}

async function parallel<T, R>(
  items: T[],
  task: (item: T) => Promise<R>,
): Promise<R[]> {
  const output: R[] = [];
  let next = 0;
  await Promise.all(
    Array.from({ length: CONCURRENCY }, async () => {
      while (next < items.length) {
        const index = next++;
        output[index] = await task(items[index]);
      }
    }),
  );
  return output;
}

function classify(status: number): LinkStatus {
  if (status === 404 || status === 410) return "missing";
  if (status === 401 || status === 403) return "restricted";
  if (status >= 200 && status < 400) return "available";
  return "unknown";
}

/** Confirm a missing result twice; rate limits, network failures and 5xx never prune. */
async function check<T = unknown>(
  url: string,
  json = true,
): Promise<{ check: Check; data?: T | null }> {
  try {
    const first = await request<T>(url, json);
    if (classify(first.status) === "missing") {
      const second = await request(url, json);
      return {
        check: {
          status: classify(second.status) === "missing" ? "missing" : "unknown",
          httpStatus: second.status,
        },
      };
    }
    return {
      check: { status: classify(first.status), httpStatus: first.status },
      data: first.data,
    };
  } catch {
    return { check: { status: "unknown" } };
  }
}

const readmeFile = await requiredJson<GitHubReadme>(
  `https://api.github.com/repos/${AWESOME}/contents/README.md`,
);
const markdown = Buffer.from(readmeFile.content, "base64").toString("utf8");
const catalog = parseAwesomeBalatroMarkdown(markdown, {
  revision: readmeFile.sha,
  fetchedAt: checkedAt,
});
if (catalog.entries.length < 200)
  throw new Error(
    "Unexpectedly small upstream README; refusing to replace snapshot",
  );

const tree = await requiredJson<GitHubTree>(
  `https://api.github.com/repos/${PRIMARY}/git/trees/main?recursive=1`,
);
if (tree.truncated) throw new Error("Truncated primary catalog tree");
const metaPaths = tree.tree.filter(
  (entry: { path: string; type: string }) =>
    entry.type === "blob" && /^mods\/[^/]+\/meta\.json$/.test(entry.path),
);
const invalidPrimaryMetadata: string[] = [];
console.log(
  `Checking ${catalog.entries.length} Awesome entries and reading ${metaPaths.length} primary catalog metadata files (concurrency ${CONCURRENCY}).`,
);
const primary = await parallel(metaPaths, async (entry: { path: string }) => {
  const url = `https://raw.githubusercontent.com/${PRIMARY}/${tree.sha}/${entry.path.split("/").map(encodeURIComponent).join("/")}`;
  const response = await fetch(url, { signal: AbortSignal.timeout(15000) });
  if (!response.ok)
    throw new Error(`Primary metadata HTTP ${response.status}: ${entry.path}`);
  try {
    return (await response.json()) as {
      repo?: string;
      repository?: string;
      meta?: { repo?: string };
    };
  } catch {
    invalidPrimaryMetadata.push(entry.path);
    return {};
  }
});
const sourceUrls = [
  ...new Set(
    catalog.entries
      .flatMap((entry) => [
        entry.sourceUrl,
        ...entry.links.map((link) => link.url),
      ])
      .map(normalizedSourceUrl),
  ),
];
const repositories = [
  ...new Set(
    [
      ...sourceUrls.map(canonicalGitHubRepository),
      ...primary.map((entry) =>
        canonicalGitHubRepository(
          entry.repo || entry.repository || entry.meta?.repo || "",
        ),
      ),
    ].filter((entry): entry is string => entry !== null),
  ),
].sort();
console.log(
  `Checking ${repositories.length} unique public GitHub repositories and ${sourceUrls.length} source links.`,
);
const records = await parallel(
  repositories,
  async (repo): Promise<[string, RepositoryRecord]> => {
    const result = await check<GitHubRepository>(
      `https://api.github.com/repos/${repo}`,
    );
    if (result.check.status === "available" && result.data?.private === false) {
      return [
        repo,
        {
          ...result.check,
          stars: result.data.stargazers_count,
          canonicalRepository: result.data.full_name.toLowerCase(),
        },
      ];
    }
    return [
      repo,
      {
        ...result.check,
        status: result.data?.private ? "restricted" : result.check.status,
        stars: null,
      },
    ];
  },
);
const metadata: RepositoryMetadata = {
  checkedAt,
  repositories: Object.fromEntries(records),
  links: {},
};
// Preserve a previously confirmed missing source during an uncertain refresh, until
// a successful check explicitly proves it is accessible again.
for (const [repo, record] of Object.entries(metadata.repositories)) {
  if (
    record.status !== "available" &&
    record.status !== "missing" &&
    previous.repositories[repo]?.status === "missing"
  ) {
    metadata.repositories[repo] = previous.repositories[repo];
  }
}

const linkRecords = await parallel(
  sourceUrls,
  async (url): Promise<[string, Check]> => {
    const repository = canonicalGitHubRepository(url);
    if (repository) {
      const repoCheck = metadata.repositories[repository];
      if (repoCheck.status !== "available")
        return [
          url,
          { status: repoCheck.status, httpStatus: repoCheck.httpStatus },
        ];
      const parsed = new URL(url);
      const parts = parsed.pathname.replace(/^\/+|\/+$/g, "").split("/");
      if (parts.length === 2)
        return [url, { status: "available", httpStatus: repoCheck.httpStatus }];
      if (
        (parts[2] === "blob" || parts[2] === "tree" || parts[2] === "raw") &&
        parts.length >= 4
      ) {
        const ref = parts[3];
        const path = parts.slice(4).join("/");
        let result = await check(
          `https://api.github.com/repos/${repository}/contents/${path}?ref=${encodeURIComponent(ref)}`,
        );
        // Slash-containing refs are ambiguous in a GitHub URL. Confirm against
        // the actual public page before treating a failed contents lookup as dead.
        if (result.check.status === "missing") result = await check(url, false);
        return [url, result.check];
      }
    }
    const result = await check(url, false);
    // Discord's generic HTML shell does not establish that a thread exists. Even
    // a 404 may be access-gating; never delete these without authenticated evidence.
    if (
      /(^|\.)(discord\.com|discordapp\.com|discord\.gg)$/.test(
        new URL(url).hostname,
      )
    ) {
      return [url, { ...result.check, status: "restricted" }];
    }
    return [url, result.check];
  },
);
metadata.links = Object.fromEntries(
  linkRecords.sort(([a], [b]) => a.localeCompare(b)),
);
for (const [url, record] of Object.entries(metadata.links)) {
  if (
    record.status !== "available" &&
    record.status !== "missing" &&
    previous.links[url]?.status === "missing"
  )
    metadata.links[url] = previous.links[url];
}
const removed = catalog.entries.filter((entry) =>
  isKnownMissingSource(entry.sourceUrl, metadata),
);
catalog.entries = catalog.entries
  .filter((entry) => !isKnownMissingSource(entry.sourceUrl, metadata))
  .map((entry) => ({
    ...entry,
    links: entry.links.filter(
      (link) => !isKnownMissingSource(link.url, metadata),
    ),
  }));
catalog.source.entryCount = catalog.entries.length;
const output = {
  ...metadata,
  primaryCatalog: {
    repository: PRIMARY,
    revision: tree.sha,
    records: primary.length,
    invalidMetadata: invalidPrimaryMetadata.sort(),
  },
  awesomeCatalog: {
    repository: AWESOME,
    readmeRevision: readmeFile.sha,
    originalEntries: catalog.entries.length + removed.length,
    retainedEntries: catalog.entries.length,
  },
  removedEntries: removed.map(({ title, sourceUrl, sourceLines }) => ({
    title,
    sourceUrl,
    sourceLines,
  })),
};
await writeFile(
  new URL("repository-metadata.json", DATA),
  JSON.stringify(output, null, 2) + "\n",
);
await writeFile(
  new URL("awesome-balatro.json", DATA),
  JSON.stringify(catalog, null, 2) + "\n",
);
const count = (status: LinkStatus) =>
  Object.values(metadata.links).filter((item) => item.status === status).length;
console.log(
  JSON.stringify(
    {
      checkedAt,
      repositories: records.length,
      sourceLinks: sourceUrls.length,
      available: count("available"),
      missing: count("missing"),
      restricted: count("restricted"),
      unknown: count("unknown"),
      removedEntries: removed.length,
      retainedEntries: catalog.entries.length,
    },
    null,
    2,
  ),
);
for (const entry of removed)
  console.log(`Removed ${entry.title}: ${entry.sourceUrl}`);
