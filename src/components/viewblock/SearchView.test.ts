import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/svelte";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import SearchView from "./SearchView.svelte";
import ModCard from "./ModCard.svelte";
import {
  currentModView,
  currentSort,
  installationStatus,
  modsStore,
  SortOption,
  type Mod,
} from "../../stores/modStore";
import { modsPerPage, showModPreviews } from "../../stores/ui";
import { openExternal } from "$lib/opener";

vi.mock("$lib/opener", () => ({ openExternal: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));
vi.mock("$lib/platform", () => ({
  isLinuxPlatform: vi.fn().mockResolvedValue(false),
}));
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue(null),
  convertFileSrc: vi.fn((path) => path),
}));
vi.mock("$lib/repositoryMetadata", () => ({
  getModRepository: (mod: Mod) => mod.repo || null,
  getRepositoryStars: (repo: string) => (repo?.endsWith("popular") ? 100 : 7),
}));

function mod(
  title: string,
  description = "",
  repo = "https://github.com/test/mod",
): Mod {
  return {
    title,
    description,
    image: "",
    categories: [],
    colors: { color1: "#000", color2: "#111" },
    requires_steamodded: false,
    requires_talisman: false,
    publisher: "Example Author",
    repo,
    downloadURL: `bmi://${title}`,
    installed: false,
    last_updated: 0,
  };
}

beforeEach(() => {
  vi.clearAllMocks();
  currentModView.set(null);
  installationStatus.set({});
  showModPreviews.set(false);
  modsPerPage.set(10);
  currentSort.set(SortOption.NameAsc);
  modsStore.set([
    mod("Alpha", "Rainbow decks"),
    mod("Zeta", "Other features", "https://github.com/test/popular"),
  ]);
});
afterEach(cleanup);

describe("catalog search", () => {
  it("renders separate mods sharing the same repository and archive", () => {
    modsStore.set([
      { ...mod("First"), downloadURL: "shared.zip" },
      { ...mod("Second"), downloadURL: "shared.zip" },
    ]);
    render(SearchView);
    expect(screen.getAllByRole("heading", { level: 3 })).toHaveLength(2);
  });
  it("preserves install and detail controls without opening the repository", async () => {
    const oninstallclick = vi.fn();
    const onmodclick = vi.fn();
    render(ModCard, { mod: mod("Controls"), oninstallclick, onmodclick });
    await fireEvent.click(screen.getByRole("button", { name: /^Download$/ }));
    expect(oninstallclick).toHaveBeenCalledOnce();
    await fireEvent.click(screen.getByRole("button", { name: "Mod details" }));
    expect(onmodclick).toHaveBeenCalledOnce();
    expect(openExternal).not.toHaveBeenCalled();
  });
  it("filters live with typo tolerance and reindexes same-size catalog updates", async () => {
    render(SearchView);
    const input = screen.getByRole("searchbox", { name: "Search all mods" });
    await fireEvent.input(input, { target: { value: "raibnow" } });
    await waitFor(() =>
      expect(
        screen
          .getAllByRole("heading", { level: 3 })
          .map((item) => item.textContent?.trim()),
      ).toEqual(["Alpha"]),
    );
    modsStore.set([
      mod("Alpha", "Changed features"),
      mod("Zeta", "Rainbow decks"),
    ]);
    await waitFor(() =>
      expect(
        screen
          .getAllByRole("heading", { level: 3 })
          .map((item) => item.textContent?.trim()),
      ).toEqual(["Zeta"]),
    );
  });
  it("sorts by real repository stars and opens the source from a card", async () => {
    const { container } = render(SearchView);
    currentSort.set(SortOption.StarsDesc);
    await waitFor(() =>
      expect(
        screen.getAllByRole("heading", { level: 3 })[0].textContent?.trim(),
      ).toBe("Zeta"),
    );
    await fireEvent.click(container.querySelector(".mod-card")!);
    expect(openExternal).toHaveBeenCalledWith(
      "https://github.com/test/popular",
    );
    expect(container.querySelector(".mod-image")).toBeNull();
    expect(screen.getAllByRole("button", { name: /^Download$/ })).toHaveLength(
      2,
    );
  });
  it("pages all matching results and restores a valid page when searching", async () => {
    modsStore.set(
      Array.from({ length: 12 }, (_, i) =>
        mod(`Mod ${String(i).padStart(2, "0")}`),
      ),
    );
    render(SearchView);
    expect(screen.getAllByRole("heading", { level: 3 })).toHaveLength(10);
    await fireEvent.click(screen.getByRole("button", { name: "Next" }));
    expect(screen.getAllByRole("heading", { level: 3 })).toHaveLength(2);
    await fireEvent.input(screen.getByRole("searchbox"), {
      target: { value: "Mod 00" },
    });
    await waitFor(() =>
      expect(
        screen.getByRole("heading", { level: 3 }).textContent?.trim(),
      ).toBe("Mod 00"),
    );
  });
});
