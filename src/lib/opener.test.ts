import { beforeEach, describe, expect, it, vi } from "vitest";
import { get } from "svelte/store";
import { openExternal, sourcePage } from "./opener";

const { isTauriMock, addMessageMock } = vi.hoisted(() => ({
  isTauriMock: vi.fn(() => true),
  addMessageMock: vi.fn(),
}));
vi.mock("@tauri-apps/api/core", () => ({ isTauri: isTauriMock }));
vi.mock("$lib/stores", () => ({ addMessage: addMessageMock }));

describe("in-app source links", () => {
  beforeEach(() => {
    sourcePage.set(null);
    isTauriMock.mockReturnValue(true);
    addMessageMock.mockClear();
  });

  it("opens a normalized source request without invoking a system browser", async () => {
    const externalWindow = vi.spyOn(window, "open");
    await openExternal("  https://github.com/example/mod#readme  ");
    expect(get(sourcePage)).toEqual({
      url: "https://github.com/example/mod#readme",
    });
    expect(externalWindow).not.toHaveBeenCalled();
    externalWindow.mockRestore();
  });

  it.each([
    "javascript:alert(1)",
    "file:///C:/secret",
    "ftp://example.com",
    "relative",
    "https://",
    "https://user:password@example.com",
  ])("rejects unsafe or malformed URLs: %s", async (url) => {
    await openExternal(url);
    expect(get(sourcePage)).toBeNull();
    expect(addMessageMock).toHaveBeenCalledWith(
      expect.stringContaining("HTTP"),
      "error",
    );
  });

  it("leaves web preview inside its current tab and explains desktop availability", async () => {
    isTauriMock.mockReturnValue(false);
    const externalWindow = vi.spyOn(window, "open");
    await openExternal("https://github.com/example/mod");
    expect(get(sourcePage)).toBeNull();
    expect(externalWindow).not.toHaveBeenCalled();
    expect(addMessageMock).toHaveBeenCalledWith(
      expect.stringContaining("desktop app"),
      "error",
    );
    externalWindow.mockRestore();
  });
});
