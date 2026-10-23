<script lang="ts">
  import type { InstalledMod, Mod } from "../../stores/modStore";
  import { onMount } from "svelte";
  import {
    installationStatus,
    modsStore,
    loadingStates2 as loadingStates,
    uninstallDialogStore,
  } from "../../stores/modStore";
  import { fuzzyFilter } from "../../utils/fuzzy";
  import { showModPreviews, modsPerPage } from "../../stores/ui";
  import { currentSort, SortOption } from "../../stores/modStore";
  import {
    getModRepository,
    getRepositoryStars,
  } from "$lib/repositoryMetadata";
  import { pageCount, pageItems } from "./browseList";
  import { currentModView } from "../../stores/modStore";
  import { invoke } from "@tauri-apps/api/core";
  import { fetchCachedMods } from "../../stores/modCache";
  import { addMessage } from "$lib/stores";
  import { fade } from "svelte/transition";
  import ModCard from "./ModCard.svelte";
  import {
    collectionsStore,
    activeCollectionIds,
    removeActiveCollection,
  } from "../../stores/collections";
  import { get } from "svelte/store";

  let searchQuery = $state("");
  let page = $state(1);
  let installedMods = $state<InstalledMod[]>([]);
  let mod = $state<Mod | null>(null);
  let searchInput: HTMLInputElement;
  let searchResults = $derived.by(() => {
    const results = fuzzyFilter(
      $modsStore,
      searchQuery,
      (item) => `${item.title} ${item.publisher} ${item.description ?? ""}`,
    );
    if (
      $currentSort === SortOption.StarsAsc ||
      $currentSort === SortOption.StarsDesc
    ) {
      return [...results].sort((a, b) => {
        const left = getRepositoryStars(getModRepository(a));
        const right = getRepositoryStars(getModRepository(b));
        if (left === null)
          return right === null ? a.title.localeCompare(b.title) : 1;
        if (right === null) return -1;
        return (
          ($currentSort === SortOption.StarsAsc
            ? left - right
            : right - left) || a.title.localeCompare(b.title)
        );
      });
    }
    if (
      $currentSort === SortOption.NameAsc ||
      $currentSort === SortOption.NameDesc
    ) {
      return [...results].sort(
        (a, b) =>
          ($currentSort === SortOption.NameAsc ? 1 : -1) *
          a.title.localeCompare(b.title),
      );
    }
    return results;
  });
  let totalPages = $derived(pageCount(searchResults.length, $modsPerPage));
  let visibleResults = $derived(pageItems(searchResults, page, $modsPerPage));
  $effect(() => {
    searchQuery;
    $modsPerPage;
    $currentSort;
    page = 1;
  });
  $effect(() => {
    if (page > totalPages) page = totalPages;
  });
  onMount(() => searchInput?.focus());

  function handleModClick(mod: Mod) {
    currentModView.set(mod);
    invoke("track_event", {
      name: "mod_browsed",
      props: { mod: mod.title },
    }).catch(() => {});
  }

  const { onCheckDependencies } = $props<{
    onCheckDependencies?: (
      requirements: { steamodded: boolean; talisman: boolean },
      downloadAction: () => Promise<void>,
    ) => void;
  }>();

  const getAllInstalledMods = async () => {
    try {
      const installed = await fetchCachedMods();
      installedMods = installed.map((mod) => ({
        name: mod.name,
        path: mod.path,
      }));
    } catch (error) {
      console.error("Failed to get installed mods:", error);
    }
  };

  const uninstallMod = async (mod: Mod) => {
    const isCoreMod = ["steamodded", "talisman"].includes(
      mod.title.toLowerCase(),
    );

    try {
      await getAllInstalledMods();
      const installedMod = installedMods.find(
        (m) => m.name.toLowerCase() === mod.title.toLowerCase(),
      );

      if (isCoreMod) {
        // Get dependents
        const dependents = await invoke<string[]>("get_dependents", {
          modName: mod.title,
        });

        // Always show dialog for core mods, even if no dependents
        uninstallDialogStore.set({
          show: true,
          modName: mod.title,
          // Path may be resolved in the dialog if missing
          modPath: installedMod?.path || "",
          dependents,
        });
      } else {
        // Immediate uninstall for normal mods
        if (!installedMod) {
          console.error("Mod not found in installed mods");
          return;
        }
        await invoke("remove_installed_mod", {
          name: mod.title,
          path: installedMod.path,
        });
        installationStatus.update((s) => ({
          ...s,
          [mod.title]: false,
        }));

        // Deactivate any active collections that contain this mod
        const collections = get(collectionsStore);
        const activeIds = get(activeCollectionIds);
        for (const id of activeIds) {
          const collection = collections.find((c) => c.id === id);
          if (collection && collection.modTitles.includes(mod.title)) {
            removeActiveCollection(id);
          }
        }
      }
    } catch (error) {
      console.error("Uninstall failed:", error);
    }
  };

  const installMod = async (mod: Mod) => {
    // Guard: don't allow re-entrancy while already loading
    if ($loadingStates[mod.title]) return;
    // Set loading immediately to prevent double-clicks
    loadingStates.update((s) => ({ ...s, [mod.title]: true }));

    // Create a closure-safe reference to the mod
    const modToInstall = { ...mod };

    // Define the actual download function
    const performDownload = async () => {
      // Re-set loading state (may be called later from dependency popup)
      loadingStates.update((s) => ({ ...s, [mod.title]: true }));
      try {
        // Create dependencies list
        const dependencies = [];
        if (modToInstall.requires_steamodded) dependencies.push("Steamodded");
        if (modToInstall.requires_talisman) dependencies.push("Talisman");

        const installedPath = await invoke<string>("install_mod", {
          url: modToInstall.downloadURL,
          folderName:
            modToInstall.folderName || modToInstall.title.replace(/\s+/g, ""),
        });

        await invoke("add_installed_mod", {
          name: modToInstall.title,
          path: installedPath,
          dependencies,
          currentVersion: modToInstall.version || "",
        });

        await getAllInstalledMods();
        installationStatus.update((s) => ({
          ...s,
          [modToInstall.title]: true,
        }));
      } catch (error) {
        console.error("Failed to install mod:", error);
        const raw = error instanceof Error ? error.message : String(error);
        const onlyUrlMsg = raw.includes("Download URL not reachable")
          ? raw.match(/Download URL not reachable[^"]*/)?.[0] || raw
          : `Failed to install ${modToInstall.title}: ${raw}`;
        addMessage(onlyUrlMsg as string, "error");
      } finally {
        loadingStates.update((s) => ({
          ...s,
          [modToInstall.title]: false,
        }));
      }
    };

    // Check dependencies first
    if (modToInstall.requires_steamodded || modToInstall.requires_talisman) {
      const steamoddedInstalled = modToInstall.requires_steamodded
        ? await invoke<boolean>("check_mod_installation", {
            modType: "Steamodded",
          })
        : true;

      const talismanInstalled = modToInstall.requires_talisman
        ? await invoke<boolean>("check_mod_installation", {
            modType: "Talisman",
          })
        : true;

      if (
        (modToInstall.requires_steamodded && !steamoddedInstalled) ||
        (modToInstall.requires_talisman && !talismanInstalled)
      ) {
        // Clear loading state before showing dependency popup
        // performDownload() will re-set it if user confirms
        loadingStates.update((s) => ({ ...s, [mod.title]: false }));
        // Key change: pass both requirements AND download function
        onCheckDependencies?.(
          {
            steamodded:
              modToInstall.requires_steamodded && !steamoddedInstalled,
            talisman: modToInstall.requires_talisman && !talismanInstalled,
          },
          performDownload,
        );
        return;
      }
    }

    // Execute download if no dependencies are missing
    await performDownload();
  };

  const isModInstalled = async (mod: Mod) => {
    if (!mod) return false;

    await getAllInstalledMods();
    const status = installedMods.some((m) => m.name === mod.title);

    // Only update the store if the status has changed
    const currentStatus = $installationStatus[mod.title];
    if (currentStatus !== status) {
      installationStatus.update((s) => ({ ...s, [mod.title]: status }));
    }

    return status;
  };

  let prevMod: Mod | null = null;

  $effect(() => {
    const newMod = $currentModView;

    // Only proceed if newMod is different from the previous mod
    if (newMod && (!prevMod || newMod.title !== prevMod.title)) {
      prevMod = newMod;
      mod = newMod;

      // Move the installation check outside of the reactive context
      setTimeout(() => {
        isModInstalled(newMod);
      }, 0);
    }
  });

  let scrollContainer: HTMLDivElement | null = $state(null);
  $effect(() => {
    page;
    searchQuery;
    if (scrollContainer) scrollContainer.scrollTop = 0;
  });
</script>

<div class="search-container">
  <div class="search-bar">
    <input
      bind:this={searchInput}
      type="search"
      bind:value={searchQuery}
      placeholder="Search mods, authors, descriptions..."
      aria-label="Search all mods"
      class="search-input"
    />
    <label
      ><input type="checkbox" bind:checked={$showModPreviews} /> Previews</label
    >
    <label
      >Per page <select
        aria-label="Search results per page"
        bind:value={$modsPerPage}
        ><option value={10}>10</option><option value={25}>25</option><option
          value={50}>50</option
        ><option value={100}>100</option><option value="all">All</option
        ></select
      ></label
    >
    <select aria-label="Sort search results" bind:value={$currentSort}
      ><option value={SortOption.NameAsc}>Name (A-Z)</option><option
        value={SortOption.NameDesc}>Name (Z-A)</option
      ><option value={SortOption.StarsDesc}>Stars (most first)</option><option
        value={SortOption.StarsAsc}>Stars (fewest first)</option
      ></select
    >
  </div>
  <div
    class="results-scroll-container default-scrollbar"
    bind:this={scrollContainer}
  >
    <div class="results-container">
      <p class="result-count">
        {searchResults.length}
        {searchResults.length === 1 ? "mod" : "mods"}
      </p>
      {#if searchResults.length === 0}
        <p>No mods found matching '{searchQuery}'.</p>
      {:else}
        <div class="results-wrapper">
          {#each visibleResults as mod (mod.id || mod._dirName || mod.title)}
            <ModCard
              {mod}
              oninstallclick={installMod}
              onuninstallclick={uninstallMod}
              onmodclick={handleModClick}
            />
          {/each}
        </div>
      {/if}
    </div>
  </div>
  {#if totalPages > 1}
    <div class="search-pagination">
      <button disabled={page === 1} onclick={() => (page -= 1)}>Previous</button
      ><span>{page} / {totalPages}</span><button
        disabled={page === totalPages}
        onclick={() => (page += 1)}>Next</button
      >
    </div>
  {/if}
</div>

<style>
  .search-container {
    display: flex;
    flex-direction: column;
    width: 100%;
    min-width: 0;
    min-height: 0;
  }
  .search-bar {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 10px;
    padding: 10px 16px;
  }
  .search-input {
    flex: 1;
    min-width: 200px;
    padding: 8px 10px;
    border: 1px solid var(--ui-mod-input-border);
    border-radius: 5px;
    background: var(--ui-mod-input-bg);
    color: var(--ui-text);
    font-size: 14px;
  }
  label {
    display: inline-flex;
    gap: 5px;
    align-items: center;
    font-size: 13px;
    color: var(--ui-text);
  }
  select,
  button {
    padding: 6px 8px;
    color: var(--ui-text);
    background: var(--ui-mod-chip-bg);
    border: 1px solid var(--ui-mod-chip-border);
    border-radius: 5px;
    font-size: 13px;
  }
  button {
    cursor: pointer;
  }
  button:disabled {
    opacity: 0.45;
    cursor: default;
  }
  .results-container {
    padding: 0 16px 16px;
  }
  .result-count {
    margin: 4px 0 10px;
    color: var(--ui-text);
    font-size: 13px;
  }
  .results-wrapper {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(min(240px, 100%), 1fr));
    gap: 12px;
    align-content: start;
  }
  .results-scroll-container {
    overflow-y: auto;
    flex: 1;
    min-height: 0;
    scrollbar-gutter: stable;
  }
  .search-pagination {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 12px;
    padding: 10px;
    font-size: 13px;
  }
</style>
