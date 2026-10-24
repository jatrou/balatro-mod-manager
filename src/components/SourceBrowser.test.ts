import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/svelte";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { sourcePage } from "$lib/opener";
import SourceBrowser from "./SourceBrowser.svelte";

const { invokeMock } = vi.hoisted(() => ({ invokeMock: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({
  invoke: invokeMock,
  isTauri: () => true,
}));
vi.mock("$lib/stores", () => ({ addMessage: vi.fn() }));

const url = "https://github.com/example/mod";
const status = { url, loading: false, download: null };
let observedResize: (() => void) | null;

describe("native source browser controls", () => {
  beforeEach(() => {
    sourcePage.set(null);
    observedResize = null;
    vi.stubGlobal(
      "ResizeObserver",
      class {
        constructor(callback: () => void) {
          observedResize = callback;
        }
        observe() {}
        disconnect() {}
      },
    );
    invokeMock.mockReset();
    invokeMock.mockImplementation((command: string) =>
      Promise.resolve(
        command === "open_source_browser" || command === "source_browser_status"
          ? status
          : null,
      ),
    );
    vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({
      x: 0,
      y: 72,
      width: 1035,
      height: 678,
      top: 72,
      left: 0,
      right: 1035,
      bottom: 750,
      toJSON: () => ({}),
    });
  });

  afterEach(async () => {
    cleanup();
    await Promise.resolve();
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
    sourcePage.set(null);
  });

  it("creates a native view below the toolbar and routes its controls through app commands", async () => {
    render(SourceBrowser);
    sourcePage.set({ url });
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("open_source_browser", {
        url,
        bounds: { x: 0, y: 72, width: 1035, height: 678 },
      }),
    );
    expect(
      screen.getByRole("textbox", { name: "Source page URL" }),
    ).toHaveProperty("value", url);
    await fireEvent.click(screen.getByRole("button", { name: "Back" }));
    await fireEvent.click(screen.getByRole("button", { name: "Forward" }));
    await fireEvent.click(screen.getByRole("button", { name: "Reload" }));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("source_browser_action", {
        action: "reload",
      }),
    );
    expect(invokeMock).toHaveBeenCalledWith("source_browser_action", {
      action: "back",
    });
    expect(invokeMock).toHaveBeenCalledWith("source_browser_action", {
      action: "forward",
    });
    observedResize?.();
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("resize_source_browser", {
        bounds: { x: 0, y: 72, width: 1035, height: 678 },
      }),
    );
    await fireEvent.click(screen.getByRole("button", { name: "Close source" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(invokeMock).toHaveBeenCalledWith("close_source_browser");
  });

  it("waits for native creation before closing a page clicked during startup", async () => {
    let finishOpen: (value: typeof status) => void = () => {};
    invokeMock.mockImplementation((command: string) =>
      command === "open_source_browser"
        ? new Promise((resolve) => {
            finishOpen = resolve;
          })
        : Promise.resolve(null),
    );
    render(SourceBrowser);
    sourcePage.set({ url });
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(
        "open_source_browser",
        expect.any(Object),
      ),
    );
    await fireEvent.click(screen.getByRole("button", { name: "Close source" }));
    expect(
      invokeMock.mock.calls.filter(
        ([command]) => command === "close_source_browser",
      ),
    ).toHaveLength(1);
    finishOpen(status);
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(
      invokeMock.mock.calls.filter(
        ([command]) => command === "close_source_browser",
      ),
    ).toHaveLength(2);
  });

  it("retires a native child left by main-page reload before opening the first new source", async () => {
    let finishCleanup: () => void = () => {};
    let cleanupStarted = false;
    invokeMock.mockImplementation((command: string) => {
      if (command === "close_source_browser" && !cleanupStarted) {
        cleanupStarted = true;
        return new Promise<void>((resolve) => {
          finishCleanup = resolve;
        });
      }
      return Promise.resolve(command === "open_source_browser" ? status : null);
    });
    render(SourceBrowser);
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("close_source_browser"),
    );
    expect(screen.queryByRole("dialog")).toBeNull();
    sourcePage.set({ url });
    await Promise.resolve();
    expect(invokeMock).not.toHaveBeenCalledWith(
      "open_source_browser",
      expect.any(Object),
    );
    finishCleanup();
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(
        "open_source_browser",
        expect.any(Object),
      ),
    );
    expect(screen.getByRole("button", { name: "Close source" })).toBeTruthy();
    expect(
      invokeMock.mock.calls.slice(0, 2).map(([command]) => command),
    ).toEqual(["close_source_browser", "open_source_browser"]);
  });

  it("reports native failures in the app and lets Reload recreate the view", async () => {
    let attempts = 0;
    invokeMock.mockImplementation((command: string) => {
      if (command === "open_source_browser") {
        attempts++;
        return attempts === 1
          ? Promise.reject("WebView2 unavailable")
          : Promise.resolve(status);
      }
      return Promise.resolve(null);
    });
    render(SourceBrowser);
    sourcePage.set({ url });
    await waitFor(() =>
      expect(screen.getByRole("alert").textContent).toContain(
        "WebView2 unavailable",
      ),
    );
    await fireEvent.click(screen.getByRole("button", { name: "Reload" }));
    await waitFor(() => expect(screen.queryByRole("alert")).toBeNull());
    expect(attempts).toBe(2);
    expect(screen.getByRole("dialog")).toBeTruthy();
  });
});
