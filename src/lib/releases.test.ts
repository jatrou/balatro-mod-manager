import { describe, expect, it, vi } from "vitest";
import { fetchLatestAppVersion } from "./releases";

describe("application release checks", () => {
  it("uses the fork and ignores missing releases on a new fork", async () => {
    const request = vi.fn().mockResolvedValue(new Response(null, { status: 404 }));
    expect(await fetchLatestAppVersion(request)).toBeNull();
    expect(request.mock.calls[0][0]).toBe(
      "https://api.github.com/repos/jatrou/balatro-mod-manager/releases/latest",
    );
  });

  it.each([
    { tag_name: "v0.5.0", assets: [] },
    { tag_name: "v0.5.0", assets: [{}], draft: true },
    { tag_name: "v0.5.0-beta", assets: [{}], prerelease: true },
    { tag_name: "not-a-version", assets: [{}] },
  ])("ignores a release that is not ready for installation: %j", async (release) => {
    const request = vi.fn().mockResolvedValue(Response.json(release));
    expect(await fetchLatestAppVersion(request)).toBeNull();
  });

  it("returns a published stable version with downloads", async () => {
    const request = vi.fn().mockResolvedValue(
      Response.json({ tag_name: "v0.5.0", assets: [{ name: "BMM.exe" }] }),
    );
    expect(await fetchLatestAppVersion(request)).toBe("0.5.0");
  });

  it("reports an unsuccessful response instead of trusting its body", async () => {
    const request = vi.fn().mockResolvedValue(new Response(null, { status: 403 }));
    await expect(fetchLatestAppVersion(request)).rejects.toThrow("HTTP 403");
  });
});
