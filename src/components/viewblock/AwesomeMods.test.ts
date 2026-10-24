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
import { modsPerPage, showModPreviews } from "../../stores/ui";
import { bundledAwesomeCatalog } from "$lib/awesomeCatalog";
import { getRepositoryStars } from "$lib/repositoryMetadata";

vi.mock("$lib/repositoryMetadata", async (importOriginal) => ({
  ...(await importOriginal<typeof import("$lib/repositoryMetadata")>()),
  getRepositoryStars: vi.fn(() => null),
}));

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

async function selectOption(name: string, value: string): Promise<void> {
  const select = screen.getByRole("combobox", { name }) as HTMLSelectElement;
  const option = Array.from(select.options).find(
    (item) => item.value === value,
  );
  if (!option) throw new Error(`Missing option ${value}`);
  option.selected = true;
  // happy-dom 20 only implements :checked for inputs. Svelte uses this query
  // to read select bindings, so supply the browser's selected option here.
  const querySelector = select.querySelector.bind(select);
  const checkedQuery = vi
    .spyOn(select, "querySelector")
    .mockImplementation((selector: string) =>
      selector === ":checked" ? option : querySelector(selector),
    );
  try {
    await fireEvent.change(select);
  } finally {
    checkedQuery.mockRestore();
  }
}

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
    vi.mocked(getRepositoryStars).mockReturnValue(null);
    modsPerPage.set(25);
    showModPreviews.set(false);
  });

  afterEach(() => {
    cleanup();
    modsStore.set([]);
    installationStatus.set({});
    installedModsCache.set([]);
  });

  it("fuzzy-matches a mistyped title and opens its GitHub repository", async () => {
    render(AwesomeMods);
    await fireEvent.input(
      screen.getByRole("searchbox", { name: /search awesome balatro/i }),
      {
        target: { value: "Redux-Arcanm" },
      },
    );
    const link = screen.getByRole("link", { name: "Redux-Arcanum" });
    expect(link.getAttribute("href")?.toLowerCase()).toBe(
      "https://github.com/jumbocarrot0/redux-arcanum",
    );
    await fireEvent.click(link);
    expect(openExternal).toHaveBeenCalledWith(link.getAttribute("href"));
  });

  it("shows one Deselect All row with loader-specific sources and safe catalog matching", async () => {
    modsStore.set([
      {
        ...catalogMod,
        title: "Deselect All",
        repo: "https://github.com/Zei33/deselect-all-steamodded",
        downloadURL: "bmi://Zei33@DeselectAll",
      },
    ]);
    render(AwesomeMods);
    await fireEvent.input(
      screen.getByRole("searchbox", { name: /search awesome balatro/i }),
      { target: { value: "Deselect All" } },
    );
    expect(
      screen.getAllByRole("heading", { name: "Deselect All" }),
    ).toHaveLength(1);
    let card = screen
      .getByRole("heading", { name: "Deselect All" })
      .closest("article")!;
    expect(within(card).getByRole("button", { name: "Install" })).toBeTruthy();
    expect(
      within(card).getByRole("button", { name: "View in catalog" }),
    ).toBeTruthy();
    await fireEvent.click(
      within(card).getByRole("button", { name: "Balamod source" }),
    );
    expect(openExternal).toHaveBeenLastCalledWith(
      "https://github.com/Zei33/deselect-all-balamod",
    );

    await selectOption("Filter by loader", "Balamod");
    expect(
      screen.getAllByRole("heading", { name: "Deselect All" }),
    ).toHaveLength(1);
    card = screen
      .getByRole("heading", { name: "Deselect All" })
      .closest("article")!;
    expect(within(card).queryByRole("button", { name: "Install" })).toBeNull();
    expect(
      within(card).queryByRole("button", { name: "View in catalog" }),
    ).toBeNull();
    await fireEvent.click(
      within(card).getByRole("button", { name: "Source instructions" }),
    );
    expect(openExternal).toHaveBeenLastCalledWith(
      "https://github.com/Zei33/deselect-all-balamod",
    );
  });

  it("paginates the directory and keeps the selected page size", async () => {
    modsPerPage.set(10);
    const firstRender = render(AwesomeMods);
    expect(screen.getAllByRole("article")).toHaveLength(10);
    const firstTitle = within(screen.getAllByRole("article")[0]).getByRole(
      "heading",
    ).textContent;
    await fireEvent.click(screen.getByRole("button", { name: /^Next$/ }));
    expect(
      within(screen.getAllByRole("article")[0]).getByRole("heading")
        .textContent,
    ).not.toBe(firstTitle);
    expect(screen.getByText(/Page 2 of/)).toBeTruthy();
    await selectOption("Mods per page", "50");
    expect(screen.getAllByRole("article")).toHaveLength(50);
    expect(screen.getByText(/Page 1 of/)).toBeTruthy();
    expect(localStorage.getItem("ui.modsPerPage")).toBe("50");
    firstRender.unmount();
    render(AwesomeMods);
    expect(screen.getAllByRole("article")).toHaveLength(50);
    await selectOption("Mods per page", "all");
    expect(screen.queryByRole("navigation", { name: "Mod pages" })).toBeNull();
    expect(screen.getAllByRole("article").length).toBeGreaterThan(100);
  });

  it("offers previews only for available pictures and retains the preference", async () => {
    modsStore.set([
      {
        ...catalogMod,
        image: "https://example.com/redux.png",
        _hasThumbnail: true,
      },
    ]);
    const firstRender = render(AwesomeMods);
    expect(
      screen.queryByRole("img", { name: "Redux-Arcanum preview" }),
    ).toBeNull();
    await fireEvent.click(screen.getByRole("checkbox", { name: "Previews" }));
    expect(localStorage.getItem("ui.showModPreviews")).toBe("true");
    await fireEvent.input(
      screen.getByRole("searchbox", { name: /search awesome balatro/i }),
      { target: { value: "Redux-Arcanum" } },
    );
    expect(
      screen.getByRole("img", { name: "Redux-Arcanum preview" }),
    ).toBeTruthy();
    firstRender.unmount();
    render(AwesomeMods);
    await fireEvent.input(
      screen.getByRole("searchbox", { name: /search awesome balatro/i }),
      { target: { value: "Redux-Arcanum" } },
    );
    expect(
      (screen.getByRole("checkbox", { name: "Previews" }) as HTMLInputElement)
        .checked,
    ).toBe(true);
    expect(
      screen.getByRole("img", { name: "Redux-Arcanum preview" }),
    ).toBeTruthy();
    modsStore.set([
      {
        ...catalogMod,
        image: "https://example.com/placeholder.png",
        _hasThumbnail: false,
      },
    ]);
    await waitFor(() =>
      expect(screen.queryByRole("checkbox", { name: "Previews" })).toBeNull(),
    );
    expect(screen.queryByRole("img")).toBeNull();
  });

  it("sorts by checked GitHub stars without treating unknown counts as zero", async () => {
    vi.mocked(getRepositoryStars).mockImplementation((url) =>
      url?.toLowerCase().includes("jumbocarrot0/redux-arcanum") ? 123 : null,
    );
    render(AwesomeMods);
    await selectOption("Sort mods", "stars");
    const card = screen.getAllByRole("article")[0];
    expect(
      within(card).getByRole("heading", { name: "Redux-Arcanum" }),
    ).toBeTruthy();
    expect(within(card).getByText("123")).toBeTruthy();
  });

  it("uses the sidebar category and reports refreshed directory categories", () => {
    const category = bundledAwesomeCatalog.entries.find(
      (entry) => entry.kind === "mod" && entry.categories.length,
    )?.categories[0];
    expect(category).toBeTruthy();
    const onCategoriesChange = vi.fn();
    render(AwesomeMods, { props: { category, onCategoriesChange } });
    expect(
      screen.getByRole("heading", { name: category, level: 2 }),
    ).toBeTruthy();
    expect(onCategoriesChange).toHaveBeenCalledWith(
      expect.arrayContaining([category]),
    );
    expect(
      screen.queryByRole("combobox", { name: "Filter by category" }),
    ).toBeNull();
    for (const card of screen.getAllByRole("article")) {
      expect(card.textContent).toContain(category);
    }
  });

  it("returns to mods when the sidebar category is selected from loaders", async () => {
    const rendered = render(AwesomeMods, {
      props: { category: "All", categorySelection: 0 },
    });
    await fireEvent.click(screen.getByRole("tab", { name: "Loaders" }));
    expect(
      screen
        .getByRole("tab", { name: "Loaders" })
        .getAttribute("aria-selected"),
    ).toBe("true");
    await rendered.rerender({ category: "All", categorySelection: 1 });
    expect(
      screen.getByRole("tab", { name: "Mods" }).getAttribute("aria-selected"),
    ).toBe("true");
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

  it("offers JokerDisplay installation with an old catalog match and cached author ban", async () => {
    modsStore.set([
      {
        ...catalogMod,
        title: "JokerDisplay",
        repo: "https://github.com/nh6574/JokerDisplay",
      },
    ]);
    localStorage.setItem(
      "awesome-balatro-catalog-v1",
      JSON.stringify({
        ...bundledAwesomeCatalog,
        entries: bundledAwesomeCatalog.entries.map((entry) =>
          entry.title === "JokerDisplay"
            ? {
                ...entry,
                safetyWarning: "This author withdrew the mod from BMM.",
              }
            : entry,
        ),
      }),
    );
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
      within(card as HTMLElement).getByRole("button", {
        name: "View in catalog",
      }),
    ).toBeTruthy();
    expect(
      within(card as HTMLElement).getByRole("button", { name: /^Install$/ }),
    ).toBeTruthy();
    expect(
      within(card as HTMLElement).queryByText(/withdrew|withdrawn/i),
    ).toBeNull();
    const cached = JSON.parse(
      localStorage.getItem("awesome-balatro-catalog-v1")!,
    );
    expect(
      cached.entries.find(
        (entry: { title: string }) => entry.title === "JokerDisplay",
      ).safetyWarning,
    ).toBeUndefined();
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
        name: "Redux-Arcanum",
        path: "C:\\Balatro\\Mods\\Redux-Arcanum",
        orphaned: false,
      },
    ]);
    render(AwesomeMods);
    await fireEvent.input(
      screen.getByRole("searchbox", { name: /search awesome balatro/i }),
      {
        target: { value: "Redux-Arcanum" },
      },
    );
    const card = screen
      .getByRole("heading", { name: "Redux-Arcanum" })
      .closest("article");
    expect(card).not.toBeNull();
    expect(within(card as HTMLElement).getByText("Installed")).toBeTruthy();
    expect(
      within(card as HTMLElement).queryByRole("button", { name: "Install" }),
    ).toBeNull();
    expect(forceRefreshCache).not.toHaveBeenCalled();
  });
});
