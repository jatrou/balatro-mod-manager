import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { get } from "svelte/store";

describe("ui store", () => {
  let mockStorage: Record<string, string>;
  const originalKeys = Object.keys.bind(Object);

  beforeEach(() => {
    mockStorage = {};

    // Mock localStorage
    vi.stubGlobal("localStorage", {
      getItem: (key: string) => mockStorage[key] ?? null,
      setItem: (key: string, value: string) => {
        mockStorage[key] = value;
      },
      removeItem: (key: string) => {
        delete mockStorage[key];
      },
      clear: () => {
        mockStorage = {};
      },
      key: (index: number) => originalKeys(mockStorage)[index] ?? null,
      get length() {
        return originalKeys(mockStorage).length;
      },
    });

    // Mock window for browser detection
    vi.stubGlobal("window", {});

    // Reset modules to pick up fresh localStorage
    vi.resetModules();
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  describe("cardScale", () => {
    it("should have default value of 1", async () => {
      const { cardScale } = await import("./ui");
      expect(get(cardScale)).toBe(1);
    });

    it("should load persisted value from localStorage", async () => {
      mockStorage["ui.cardScale"] = "1.2";
      const { cardScale } = await import("./ui");
      expect(get(cardScale)).toBe(1.2);
    });

    it("should clamp to minimum of 0.75", async () => {
      mockStorage["ui.cardScale"] = "0.5";
      const { cardScale } = await import("./ui");
      expect(get(cardScale)).toBe(0.75);
    });

    it("should clamp to maximum of 1.4", async () => {
      mockStorage["ui.cardScale"] = "2.0";
      const { cardScale } = await import("./ui");
      expect(get(cardScale)).toBe(1.4);
    });

    it("should persist changes to localStorage", async () => {
      const { cardScale } = await import("./ui");
      cardScale.set(1.1);
      expect(mockStorage["ui.cardScale"]).toBe("1.1");
    });

    it("should handle invalid localStorage value", async () => {
      mockStorage["ui.cardScale"] = "not-a-number";
      const { cardScale } = await import("./ui");
      // NaN check fails, should use fallback
      expect(get(cardScale)).toBe(1);
    });
  });

  describe("browsing preferences", () => {
    it("uses dark mode even after upgrading from a light preference", async () => {
      mockStorage["ui.darkMode"] = "false";
      const { darkMode } = await import("./ui");
      expect(get(darkMode)).toBe(true);
    });

    it("defaults to compact browsing with previews off and 25 mods", async () => {
      const { showModPreviews, modsPerPage } = await import("./ui");
      expect(get(showModPreviews)).toBe(false);
      expect(get(modsPerPage)).toBe(25);
    });

    it.each([10, 25, 50, 100, "all"])(
      "restores saved page size %s",
      async (value) => {
        mockStorage["ui.modsPerPage"] = String(value);
        const { modsPerPage } = await import("./ui");
        expect(get(modsPerPage)).toBe(value);
      },
    );

    it.each(["12", "0", "-1", "Infinity", "garbage", ""])(
      "rejects invalid saved size %s",
      async (value) => {
        mockStorage["ui.modsPerPage"] = value;
        const { modsPerPage } = await import("./ui");
        expect(get(modsPerPage)).toBe(25);
      },
    );

    it("persists and restores both browsing preferences", async () => {
      const { modsPerPage, showModPreviews } = await import("./ui");
      modsPerPage.set("all");
      showModPreviews.set(true);
      expect(mockStorage["ui.modsPerPage"]).toBe("all");
      expect(mockStorage["ui.showModPreviews"]).toBe("true");
      vi.resetModules();
      const restored = await import("./ui");
      expect(get(restored.modsPerPage)).toBe("all");
      expect(get(restored.showModPreviews)).toBe(true);
    });

    it("works when browser storage is unavailable", async () => {
      vi.stubGlobal("localStorage", {
        getItem() {
          throw new Error("disabled");
        },
        setItem() {
          throw new Error("disabled");
        },
      });
      const { modsPerPage, showModPreviews } = await import("./ui");
      modsPerPage.set(100);
      showModPreviews.set(true);
      expect(get(modsPerPage)).toBe(100);
      expect(get(showModPreviews)).toBe(true);
    });
  });
});
