export const RELEASES_URL =
  "https://github.com/jatrou/balatro-mod-manager/releases";

const LATEST_RELEASE_API =
  "https://api.github.com/repos/jatrou/balatro-mod-manager/releases/latest";

// A tag alone does not mean there is a downloadable application. In particular,
// a new fork inherits the original project's tags without inheriting releases.
export async function fetchLatestAppVersion(
  request: typeof fetch = fetch,
): Promise<string | null> {
  const response = await request(LATEST_RELEASE_API, {
    headers: { Accept: "application/vnd.github+json" },
    signal: AbortSignal.timeout(15_000),
  });
  if (response.status === 404) return null;
  if (!response.ok) throw new Error(`Release check failed: HTTP ${response.status}`);
  const release = await response.json();
  if (
    release.draft ||
    release.prerelease ||
    typeof release.tag_name !== "string" ||
    !/^v?\d+\.\d+\.\d+$/i.test(release.tag_name) ||
    !Array.isArray(release.assets) ||
    release.assets.length === 0
  ) {
    return null;
  }
  return release.tag_name.replace(/^v/i, "");
}
