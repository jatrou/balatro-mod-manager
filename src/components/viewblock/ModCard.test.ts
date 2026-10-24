import {
  cleanup,
  fireEvent,
  render,
  screen,
  within,
  waitFor,
} from "@testing-library/svelte";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { get } from "svelte/store";
import { invoke } from "@tauri-apps/api/core";
import { addMessage } from "$lib/stores";
import ModCard from "./ModCard.svelte";
import LocalModCard from "./LocalModCard.svelte";
import {
  installationStatus,
  modEnabledStore,
  modsStore,
  selectedModsStore,
  type LocalMod,
  type Mod,
} from "../../stores/modStore";
import { descriptionsStore } from "../../stores/descriptions";
import { showModPreviews } from "../../stores/ui";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
  convertFileSrc: (path: string) => path,
}));
vi.mock("@tauri-apps/api/path", () => ({
  configDir: async () => "",
  join: async (...parts: string[]) => parts.join("/"),
}));
vi.mock("$lib/platform", () => ({ isLinuxPlatform: async () => false }));
vi.mock("$lib/opener", () => ({ openExternal: vi.fn() }));
vi.mock("$lib/stores", () => ({ addMessage: vi.fn() }));
vi.mock("../../stores/modCache", () => ({ forceRefreshCache: vi.fn() }));
vi.mock("../../utils/resize-observer-pool", () => ({
  observeResize: () => () => {},
}));

const placeholder = (overrides: Partial<Mod> = {}): Mod => ({
  title: "Installed Example",
  description: "",
  image: "/images/cover.jpg",
  colors: { color1: "#000000", color2: "#111111" },
  categories: [],
  requires_steamodded: false,
  requires_talisman: false,
  publisher: "Installed",
  repo: "",
  downloadURL: "",
  installed: true,
  last_updated: 0,
  orphaned: true,
  _hasThumbnail: false,
  _installedPath: "C:/Mods/InstalledExample",
  ...overrides,
});

describe("installed mod cards", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(invoke).mockReset();
    showModPreviews.set(false);
    installationStatus.set({ "Installed Example": true });
    modEnabledStore.set({});
    descriptionsStore.set({});
    selectedModsStore.set(new Set());
    modsStore.set([]);
  });
  afterEach(() => {
    cleanup();
    installationStatus.set({});
    descriptionsStore.set({});
    selectedModsStore.set(new Set());
    modsStore.set([]);
  });

  it.each([false, true])(
    "shows truthful primary catalog status with previews %s",
    (previews) => {
      showModPreviews.set(previews);
      render(ModCard, { props: { mod: placeholder() } });
      const badge = screen.getByText("Not in primary catalog");
      expect(badge.getAttribute("title")).toBe(
        "Installed locally; not listed in the current primary catalog.",
      );
      expect(screen.queryByText(/removed from index/i)).toBeNull();
      expect(screen.getByRole("button", { name: "Installed" })).toBeTruthy();
    },
  );

  it("replaces an impossible description load with a static fallback", () => {
    const rendered = render(ModCard, { props: { mod: placeholder() } });
    expect(
      screen.getByText("Description unavailable for this installed mod."),
    ).toBeTruthy();
    expect(rendered.container.querySelector(".desc-skeleton")).toBeNull();
  });

  it("retains description loading for primary entries that have a fetchable catalog ID", () => {
    const rendered = render(ModCard, {
      props: {
        mod: placeholder({ _dirName: "author@example", _catalogEntry: true }),
      },
    });
    expect(rendered.container.querySelector(".desc-skeleton")).not.toBeNull();
    expect(
      screen.queryByText("Description unavailable for this installed mod."),
    ).toBeNull();
  });

  it("shows cached descriptions for installed placeholders", () => {
    descriptionsStore.set({
      "Installed Example": "Description from a local cache.",
    });
    const rendered = render(ModCard, { props: { mod: placeholder() } });
    expect(screen.getByText("Description from a local cache.")).toBeTruthy();
    expect(rendered.container.querySelector(".desc-skeleton")).toBeNull();
    expect(
      screen.queryByText("Description unavailable for this installed mod."),
    ).toBeNull();
  });

  it("uses full-path state and toggles same-name copies independently", async () => {
    const manual: LocalMod = {
      name: "Example (Manual)",
      id: "example",
      author: ["Author"],
      description: "Local copy",
      prefix: "example",
      path: "C:/Mods/CopyA",
      dependencies: [],
      conflicts: [],
      is_tracked: false,
    };
    modEnabledStore.set({
      "C:/Mods/CopyA": false,
      "C:/Mods/CopyB": true,
      CopyA: true,
      CopyB: false,
      "Example (Manual)": true,
    });
    const first = render(LocalModCard, {
      props: { mod: manual, onUninstall: vi.fn() },
    });
    const second = render(LocalModCard, {
      props: {
        mod: { ...manual, path: "C:/Mods/CopyB" },
        onUninstall: vi.fn(),
      },
    });
    expect(
      within(first.container)
        .getByRole("button", { name: "Enable Mod" })
        .textContent?.trim(),
    ).toBe("OFF");
    expect(
      within(second.container)
        .getByRole("button", { name: "Disable Mod" })
        .textContent?.trim(),
    ).toBe("ON");
    await fireEvent.click(
      within(first.container).getByRole("button", { name: "Enable Mod" }),
    );
    expect(invoke).toHaveBeenCalledWith("toggle_mod_enabled_by_path", {
      modPath: "C:/Mods/CopyA",
      enabled: true,
    });
    await fireEvent.click(
      within(second.container).getByRole("button", { name: "Disable Mod" }),
    );
    expect(invoke).toHaveBeenCalledWith("toggle_mod_enabled_by_path", {
      modPath: "C:/Mods/CopyB",
      enabled: false,
    });
    expect(get(modEnabledStore)).toMatchObject({
      "C:/Mods/CopyA": true,
      "C:/Mods/CopyB": false,
      CopyA: true,
      CopyB: false,
      "Example (Manual)": true,
    });
    expect(
      within(first.container)
        .getByRole("button", { name: "Disable Mod" })
        .textContent?.trim(),
    ).toBe("ON");
    expect(
      within(second.container)
        .getByRole("button", { name: "Enable Mod" })
        .textContent?.trim(),
    ).toBe("OFF");
  });

  it("supports the legacy folder key and reports toggle failures without changing state", async () => {
    const manual: LocalMod = {
      name: "Example (Manual)",
      id: "example",
      author: ["Author"],
      description: "Local copy",
      prefix: "example",
      path: "C:/Mods/LegacyCopy",
      dependencies: [],
      conflicts: [],
      is_tracked: false,
    };
    modEnabledStore.set({ LegacyCopy: false, "Example (Manual)": true });
    vi.mocked(invoke).mockRejectedValueOnce(new Error("Access denied"));
    const consoleError = vi
      .spyOn(console, "error")
      .mockImplementation(() => {});
    render(LocalModCard, { props: { mod: manual, onUninstall: vi.fn() } });
    const toggle = screen.getByRole("button", { name: "Enable Mod" });
    expect(toggle.textContent?.trim()).toBe("OFF");
    await fireEvent.click(toggle);
    await waitFor(() =>
      expect(addMessage).toHaveBeenCalledWith(
        "Failed to toggle Example (Manual): Error: Access denied",
        "error",
      ),
    );
    expect(get(modEnabledStore)).toEqual({
      LegacyCopy: false,
      "Example (Manual)": true,
    });
    expect(
      screen.getByRole("button", { name: "Enable Mod" }).textContent?.trim(),
    ).toBe("OFF");
    expect((toggle as HTMLButtonElement).disabled).toBe(false);
    consoleError.mockRestore();
  });

  it("keeps duplicate manual copies visible and selects them by their distinct paths", async () => {
    const manual: LocalMod = {
      name: "Example (Manual)",
      id: "example",
      author: ["Author"],
      description: "Local copy",
      prefix: "example",
      path: "C:/Mods/CopyA",
      dependencies: [],
      conflicts: [],
      is_tracked: false,
    };
    const first = render(LocalModCard, {
      props: { mod: manual, onUninstall: vi.fn(), showCheckbox: true },
    });
    const second = render(LocalModCard, {
      props: {
        mod: { ...manual, path: "C:/Mods/CopyB" },
        onUninstall: vi.fn(),
        showCheckbox: true,
      },
    });
    expect(
      screen.getAllByRole("heading", { name: "Example (Manual)" }),
    ).toHaveLength(2);
    await fireEvent.click(within(first.container).getByTitle("Select"));
    await fireEvent.click(within(second.container).getByTitle("Select"));
    expect(get(selectedModsStore)).toEqual(
      new Set(["C:/Mods/CopyA", "C:/Mods/CopyB"]),
    );
  });
});
