import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/svelte";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { writable } from "svelte/store";
import AwesomeMods from "./AwesomeMods.svelte";
import { installationStatus, modsStore, type Mod } from "../../stores/modStore";
import { installedModsCache, forceRefreshCache } from "../../stores/modCache";
import { openExternal } from "$lib/opener";

vi.mock("$lib/opener", () => ({
  openExternal: vi.fn(),
}));

vi.mock("../../stores/modCache", () => ({
  installedModsCache: writable([]),
  fetchCachedMods: vi.fn(),
  forceRefreshCache: vi.fn(),
}));

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));

const catalogMod: Mod = {
  title: "Redux-Arcanum",
  description: "Catalog description",
  image: "",
  categories: [],
  colors: { color1: "#000", color2: "#fff" },
  requires_steamodded: true,
  requires_talisman: false,
  publisher: "jumbocarrot0",
  repo: "https://github.com/jumbocarrot0/Redux-Arcanum",
  downloadURL: "bmi://redux-arcanum",
  installed: false,
  last_updated: 0,
};

describe("AwesomeMods", () => {
  beforeEach(() => {
    Object.defineProperty(HTMLDialogElement.prototype, "showModal", {
      configurable: true,
      value() {
        this.open = true;
      },
    });
    localStorage.clear();
    modsStore.set([catalogMod]);
    installationStatus.set({});
    installedModsCache.set([]);
    vi.clearAllMocks();
  });

  afterEach(() => {
    cleanup();
    modsStore.set([]);
    installationStatus.set({});
    installedModsCache.set([]);
  });

  it("opens an exact current-catalog match through the parent callback", async () => {
    const onViewCatalog = vi.fn();
    render(AwesomeMods, { props: { onViewCatalog } });
    await fireEvent.input(
      screen.getByRole("searchbox", { name: /search awesome balatro/i }),
      { target: { value: "Redux-Arcanum" } },
    );

    const card = screen
      .getByRole("heading", { name: "Redux-Arcanum" })
      .closest("article");
    expect(card).not.toBeNull();
    await fireEvent.click(
      within(card as HTMLElement).getByRole("button", {
        name: "View in catalog",
      }),
    );
    expect(onViewCatalog).toHaveBeenCalledWith(catalogMod);
  });

  it("keeps withdrawn entries source-only even if the old catalog contains them", async () => {
    modsStore.set([
      {
        ...catalogMod,
        title: "JokerDisplay",
        repo: "https://github.com/nh6574/JokerDisplay",
      },
    ]);
    render(AwesomeMods);
    await fireEvent.input(
      screen.getByRole("searchbox", { name: /search awesome balatro/i }),
      { target: { value: "JokerDisplay" } },
    );

    const card = screen
      .getByRole("heading", { name: "JokerDisplay" })
      .closest("article");
    expect(card).not.toBeNull();
    expect(
      within(card as HTMLElement).queryByRole("button", {
        name: "View in catalog",
      }),
    ).toBeNull();
    expect(
      within(card as HTMLElement).getByText(/withdrew the mod from BMM/i),
    ).toBeTruthy();
  });

  it("offers Discord download and local import for a Discord-only entry", async () => {
    modsStore.set([]);
    render(AwesomeMods);
    await fireEvent.input(
      screen.getByRole("searchbox", { name: /search awesome balatro/i }),
      { target: { value: "1 Hand You're Joking" } },
    );
    const card = screen
      .getByRole("heading", { name: "1 Hand? You're Joking?" })
      .closest("article");
    expect(card).not.toBeNull();
    await fireEvent.click(
      within(card as HTMLElement).getByRole("button", {
        name: "Download from Discord",
      }),
    );
    await waitFor(() =>
      expect(openExternal).toHaveBeenCalledWith(
        "https://discord.com/channels/1116389027176787968/1214857669932744714",
      ),
    );
    expect(
      screen.getByRole("textbox", {
        name: "Downloaded file or folder location",
      }),
    ).toBeTruthy();
  });

  it("tracks an Awesome-only installed mod even without a BMM catalog match", async () => {
    modsStore.set([]);
    installedModsCache.set([
      {
        name: "Absolute Deck",
        path: "C:\\Balatro\\Mods\\Absolute Deck",
        orphaned: false,
      },
    ]);
    render(AwesomeMods);
    await fireEvent.input(
      screen.getByRole("searchbox", { name: /search awesome balatro/i }),
      {
        target: { value: "Absolute Deck" },
      },
    );
    const card = screen
      .getByRole("heading", { name: "Absolute Deck" })
      .closest("article");
    expect(card).not.toBeNull();
    expect(within(card as HTMLElement).getByText("Installed")).toBeTruthy();
    expect(
      within(card as HTMLElement).queryByRole("button", { name: "Install" }),
    ).toBeNull();
    expect(forceRefreshCache).not.toHaveBeenCalled();
  });
});
