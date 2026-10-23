<script lang="ts">
  import LaunchButton from "../../components/LaunchButton.svelte";
  import Mods from "../../components/viewblock/Mods.svelte";
  import AwesomeMods from "../../components/viewblock/AwesomeMods.svelte";
  import Settings from "../../components/viewblock/Settings.svelte";
  import RequiresPopup from "../../components/RequiresPopup.svelte";
  import WarningPopup from "../../components/WarningPopup.svelte";
  import SecurityPopup from "../../components/SecurityPopup.svelte";
  import LovelyMissingPopup from "../../components/LovelyMissingPopup.svelte";
  import CollectionImportPopup from "../../components/CollectionImportPopup.svelte";
  import type { DependencyCheck, InstalledMod } from "../../stores/modStore";
  import {
    currentModView,
    currentCategory,
    modsStore,
  } from "../../stores/modStore";
  import { selectedModStore, dependentsStore } from "../../stores/modStore";
  import { currentPage, paginationWindow } from "../../stores/modStore";
  import {
    installationStatus,
    showWarningPopup,
    requiresPopupStore,
    securityPopupStore,
  } from "../../stores/modStore";
  import { invoke } from "@tauri-apps/api/core";
  import { fetchCachedMods, forceRefreshCache } from "../../stores/modCache";
  import { addMessage } from "$lib/stores";
  import UninstallDialog from "../../components/UninstallDialog.svelte";
  import { onMount } from "svelte";
  import { lovelyPopupStore } from "../../stores/modStore";
  import { cardScale } from "../../stores/ui";
  import { get } from "svelte/store";
  import ReportIssue from "../../components/ReportIssue.svelte";
  import CollectionPicker from "../../components/CollectionPicker.svelte";
  import BackupsPanel from "../../components/BackupsPanel.svelte";
  import CreateBackupModal from "../../components/CreateBackupModal.svelte";
  import RestoreBackupPopup from "../../components/RestoreBackupPopup.svelte";
  import DeleteBackupPopup from "../../components/DeleteBackupPopup.svelte";
  import { fade } from "svelte/transition";
  import { backupsStore } from "../../stores/backups";

  import {
    Archive,
    Check,
    FolderHeart,
    Grid2X2,
    Layers,
    Package,
    Settings2,
  } from "lucide-svelte";
  import { bundledAwesomeCatalog } from "$lib/awesomeCatalog";

  let currentSection = $state("awesome");
  let selectedAwesomeCategory = $state("All");
  let selectedAwesomeRevision = $state(0);
  let awesomeCategories = $state(
    Array.from(
      new Set(
        bundledAwesomeCatalog.entries
          .filter((entry) => entry.kind === "mod")
          .flatMap((entry) => entry.categories),
      ),
    ).sort((a, b) => a.localeCompare(b)),
  );

  function selectLibrary(category: string) {
    currentModView.set(null);
    currentCategory.set(category);
    currentPage.set(1);
    currentSection = "mods";
  }

  function selectAwesome(category: string) {
    selectedAwesomeRevision += 1;
    selectedAwesomeCategory = category;
    currentSection = "awesome";
  }
  let appVersion = $state("");

  let requiresPopupDismissedAt = 0;
  let wasRequiresPopupVisible = false;

  let storedDownloadAction: (() => Promise<void>) | null = $state(null);
  let originalDownloadAction: (() => Promise<void>) | null = $state(null);

  // Function to check if security warning needs to be shown
  async function checkSecurityAcknowledgment(): Promise<boolean> {
    try {
      const isAcknowledged = await invoke<boolean>(
        "is_security_warning_acknowledged",
      );
      return isAcknowledged;
    } catch (error) {
      console.error("Failed to check security acknowledgment:", error);
      return false; // If there's an error, show the popup anyway
    }
  }

  // Modified to include security check
  async function handleDependencyCheck(
    requirements: DependencyCheck,
    downloadAction?: () => Promise<void>,
  ) {
    if (Date.now() - requiresPopupDismissedAt < 200) {
      return;
    }
    modRequirements = requirements;
    if (downloadAction) {
      originalDownloadAction = downloadAction;

      // Check if we need to show the security popup first
      const isSecurityAcknowledged = await checkSecurityAcknowledgment();

      if (!isSecurityAcknowledged) {
        // Store the action but don't execute it yet - show security popup first
        storedDownloadAction = null;
        securityPopupStore.set({
          visible: true,
          onAcknowledge: handleSecurityAcknowledge,
          onCancel: handleSecurityCancel,
        });
      } else {
        // Security already acknowledged, proceed with dependency check
        storedDownloadAction = downloadAction;
        requiresPopupStore.set({
          visible: true,
          requiresSteamodded: requirements.steamodded,
          requiresTalisman: requirements.talisman,
          onProceed: handleProceedDownload,
          onDependencyClick: handleDependencyClick,
        });
      }
    } else {
      console.warn("handleDependencyCheck called without a download action");
      storedDownloadAction = null;
      originalDownloadAction = null;
    }
  }

  // Handle security acknowledgment
  async function handleSecurityAcknowledge() {
    securityPopupStore.update((s) => ({ ...s, visible: false }));

    // Now proceed with dependency check if there was an action
    if (originalDownloadAction) {
      storedDownloadAction = originalDownloadAction;
      requiresPopupStore.set({
        visible: true,
        requiresSteamodded: modRequirements.steamodded,
        requiresTalisman: modRequirements.talisman,
        onProceed: handleProceedDownload,
        onDependencyClick: handleDependencyClick,
      });
    }
  }

  // Handle security cancellation
  function handleSecurityCancel() {
    securityPopupStore.update((s) => ({ ...s, visible: false }));
    storedDownloadAction = null;
    originalDownloadAction = null;
  }

  function nextPage() {
    if ($currentPage < $paginationWindow.totalPages) {
      currentPage.update((n) => n + 1);
    }
  }

  function previousPage() {
    if ($currentPage > 1) {
      currentPage.update((n) => n - 1);
    }
  }

  function goToPage(page: number) {
    if (page < 1 || page > $paginationWindow.totalPages) return;
    currentPage.set(page);
  }

  function handleProceedDownload() {
    if (storedDownloadAction) {
      storedDownloadAction().catch((error) => {
        console.error("Error during download action execution:", error);
        showError(error);
      });
    } else {
      console.warn(
        "Proceed action requested, but no download action was stored.",
      );
    }
    storedDownloadAction = null; // Clear the stored action
    originalDownloadAction = null; // Clear the original action too
  }

  let contentElement: HTMLDivElement;

  let showUninstallDialog = $state(false);
  const selectedMod = $derived($selectedModStore);

  async function handleRefresh() {
    // Force-refresh cache so removal reflects immediately
    await forceRefreshCache();
    const installedMods: InstalledMod[] = await fetchCachedMods();
    installationStatus.set(
      Object.fromEntries(
        installedMods.map((mod: InstalledMod) => [mod.name, true]),
      ),
    );
  }

  function showError(error: unknown) {
    addMessage(
      `Uninstall failed: ${error instanceof Error ? error.message : String(error)}`,
      "error",
    );
  }

  function onError(event: { detail: unknown }) {
    showError(event.detail);
  }

  function onUninstalled(_event: {
    detail: { modName: string; success: boolean; action: string };
  }) {
    handleRefresh();
  }

  let modRequirements = $state({
    steamodded: false,
    talisman: false,
  });

  function handleDependencyClick(dependency: string) {
    // Find the mod in the store
    const mods = get(modsStore);
    const foundMod = mods.find(
      (m) => m.title.toLowerCase() === dependency.toLowerCase(),
    );

    // If found, open it in the mod view
    if (foundMod) {
      currentModView.set(foundMod);
    } else {
      console.warn(`Dependency mod not found: ${dependency}`);
    }
  }

  function handleRequestUninstall(
    event: CustomEvent<{ mod: InstalledMod; dependents: string[] }>,
  ) {
    selectedModStore.set(event.detail.mod);
    dependentsStore.set(event.detail.dependents);
    showUninstallDialog = true;
  }

  interface AppInitData {
    version: string;
    existing_installation: string | null;
    security_acknowledged: boolean;
    lovely_installed: boolean;
    lovely_update_available: string | null;
    launch_mode: string;
  }

  onMount(async () => {
    handleRefresh();

    // Fetch all init data in a single batched IPC call
    try {
      const initData = await invoke<AppInitData>("get_app_init_data");
      appVersion = initData.version;

      // Check if we need to show the security popup
      if (!initData.security_acknowledged) {
        securityPopupStore.set({
          visible: true,
          onAcknowledge: handleSecurityAcknowledge,
          onCancel: handleSecurityCancel,
        });
      }

      // Check Lovely status
      if (!initData.lovely_installed) {
        // Not installed: show install prompt
        lovelyPopupStore.set({ visible: true });
      } else if (initData.lovely_update_available) {
        // Lovely is installed but update available
        showWarningPopup.set({
          visible: true,
          message: `An update for Lovely (v${initData.lovely_update_available}) is available. Do you want to update?`,
          onConfirm: async () => {
            try {
              const updated = await invoke<string>("update_lovely_to_latest");
              addMessage(`Lovely updated to v${updated}`, "success");
            } catch (e) {
              addMessage(
                `Failed to update Lovely: ${e instanceof Error ? e.message : String(e)}`,
                "error",
              );
            }
            showWarningPopup.update((p) => ({ ...p, visible: false }));
          },
          onCancel: () => {
            showWarningPopup.update((p) => ({ ...p, visible: false }));
          },
        });
      }
    } catch (error) {
      console.error("Failed to load init data:", error);
      // Fallback to individual calls if batch fails
      try {
        appVersion = await invoke<string>("get_app_version");
      } catch (_) {
        appVersion = "";
      }
    }
  });

  $effect(() => {
    if (wasRequiresPopupVisible && !$requiresPopupStore.visible) {
      storedDownloadAction = null;
      originalDownloadAction = null;
      requiresPopupDismissedAt = Date.now();
    }
    wasRequiresPopupVisible = $requiresPopupStore.visible;
  });
</script>

<div class="main-page">
  <header>
    <div class="brand">
      <span class="brand-mark" aria-hidden="true"
        ><Package size={19} strokeWidth={1.7} /></span
      >
      <h1>Balatro <span>Mod Manager</span></h1>
      {#if appVersion}<span class="version-text">v{appVersion}</span>{/if}
    </div>
    <LaunchButton />
  </header>

  <div class="workspace">
    <aside class="sidebar" aria-label="Navigation">
      <nav class="library-nav" aria-label="Your library">
        <p class="nav-label">Library</p>
        <button
          class:active={currentSection === "mods" &&
            $currentCategory === "Installed Mods"}
          onclick={() => selectLibrary("Installed Mods")}
        >
          <Package size={15} />Installed mods
        </button>
        <button
          class:active={currentSection === "mods" &&
            $currentCategory === "Collections"}
          onclick={() => selectLibrary("Collections")}
        >
          <FolderHeart size={15} />Collections
        </button>
        <button
          class:active={currentSection === "mods" &&
            $currentCategory === "All Mods"}
          onclick={() => selectLibrary("All Mods")}
        >
          <Grid2X2 size={15} />Mod catalog
        </button>
      </nav>

      <nav
        class="catalog-nav default-scrollbar"
        aria-label="Awesome Balatro categories"
      >
        <p class="nav-label">Awesome Balatro</p>
        <button
          class:active={currentSection === "awesome" &&
            selectedAwesomeCategory === "All"}
          onclick={() => selectAwesome("All")}
        >
          <Layers size={15} />All mods
        </button>
        {#each awesomeCategories as category (category)}
          <button
            class="category-button"
            class:active={currentSection === "awesome" &&
              selectedAwesomeCategory === category}
            onclick={() => selectAwesome(category)}
          >
            <span>{category}</span>
            {#if currentSection === "awesome" && selectedAwesomeCategory === category}<Check
                size={13}
              />{/if}
          </button>
        {/each}
      </nav>

      <nav class="utility-nav" aria-label="Manager">
        <button
          class:active={currentSection === "backups"}
          onclick={() => {
            currentSection = "backups";
            backupsStore.load();
          }}
        >
          <Archive size={15} />Backups
        </button>
        <button
          class:active={currentSection === "settings"}
          onclick={() => (currentSection = "settings")}
        >
          <Settings2 size={15} />Settings
        </button>
        <ReportIssue inline />
      </nav>
    </aside>

    <main class="main-content">
      <div
        class="content"
        class:modal-open={!!$currentModView && currentSection == "mods"}
        bind:this={contentElement}
        style="--card-scale: {$cardScale}"
      >
        <!-- All sections stay mounted for smooth transitions -->
        <div
          class="section-wrapper"
          class:active={currentSection === "mods"}
          inert={currentSection !== "mods"}
        >
          <Mods mod={null} {handleDependencyCheck} />
        </div>

        <div
          class="section-wrapper"
          class:active={currentSection === "backups"}
          inert={currentSection !== "backups"}
        >
          <BackupsPanel />
        </div>

        <div
          class="section-wrapper"
          class:active={currentSection === "awesome"}
          inert={currentSection !== "awesome"}
        >
          <AwesomeMods
            category={selectedAwesomeCategory}
            categorySelection={selectedAwesomeRevision}
            onCategoriesChange={(categories) =>
              (awesomeCategories = categories)}
            onViewCatalog={(mod) => {
              currentModView.set(mod);
              currentSection = "mods";
            }}
          />
        </div>

        <div
          class="section-wrapper"
          class:active={currentSection === "settings"}
          inert={currentSection !== "settings"}
        >
          <Settings />
        </div>
      </div>

      {#if currentSection === "mods" && !$currentModView && $currentCategory !== "Search" && $currentCategory !== "Collections" && $paginationWindow.totalPages > 1}
        <div
          class="pagination-footer"
          in:fade={{ duration: 150 }}
          out:fade={{ duration: 120 }}
        >
          <div class="pagination-controls">
            <button onclick={previousPage} disabled={$currentPage === 1}>
              Previous
            </button>
            {#each Array(Math.min($paginationWindow.maxVisiblePages, $paginationWindow.totalPages)) as _, i (i)}
              {#if $paginationWindow.startPage + i <= $paginationWindow.totalPages}
                <button
                  class:active={$currentPage ===
                    $paginationWindow.startPage + i}
                  onclick={() => goToPage($paginationWindow.startPage + i)}
                >
                  {$paginationWindow.startPage + i}
                </button>
              {/if}
            {/each}
            <button
              onclick={nextPage}
              disabled={$currentPage === $paginationWindow.totalPages}
            >
              Next
            </button>
          </div>
        </div>
      {/if}
    </main>
  </div>

  <RequiresPopup />

  <WarningPopup
    visible={$showWarningPopup.visible}
    message={$showWarningPopup.message}
    onConfirm={$showWarningPopup.onConfirm}
    onCancel={$showWarningPopup.onCancel}
  />

  <SecurityPopup />

  <UninstallDialog
    bind:show={showUninstallDialog}
    modName={selectedMod?.name ?? ""}
    modPath={selectedMod?.path ?? ""}
    bind:dependents={$dependentsStore}
    {onUninstalled}
    {onError}
  />
</div>

<LovelyMissingPopup />
<CollectionPicker />
<CollectionImportPopup />

<CreateBackupModal />
<RestoreBackupPopup />
<DeleteBackupPopup />

<style>
  .main-page {
    width: 100%;
    height: 100%;
    display: flex;
    flex-direction: column;
    background: var(--ui-bg);
  }

  header {
    height: 62px;
    flex-shrink: 0;
    padding: 0 22px;
    display: flex;
    justify-content: space-between;
    align-items: center;
    border-bottom: 1px solid var(--ui-border);
  }

  .brand {
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 0;
  }
  .brand-mark {
    display: flex;
    color: var(--ui-accent);
  }
  h1 {
    font-size: 17px;
    font-weight: 650;
    letter-spacing: -0.3px;
    margin: 0;
  }
  h1 span {
    font-weight: 400;
    color: var(--ui-text-muted);
    margin-left: 3px;
  }
  .version-text {
    color: var(--ui-text-muted);
    font-size: 11px;
    margin-left: 5px;
  }
  .workspace {
    display: flex;
    flex: 1;
    min-height: 0;
  }
  .sidebar {
    width: 190px;
    flex-shrink: 0;
    display: flex;
    flex-direction: column;
    padding: 14px 10px 10px;
    box-sizing: border-box;
    border-right: 1px solid var(--ui-border);
    background: #1b1e21;
  }
  nav {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .nav-label {
    color: var(--ui-text-muted);
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.8px;
    text-transform: uppercase;
    margin: 0 10px 7px;
  }
  .library-nav {
    margin-bottom: 19px;
  }
  .catalog-nav {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding-bottom: 10px;
  }
  .utility-nav {
    border-top: 1px solid var(--ui-border);
    padding-top: 9px;
    margin-top: 4px;
  }
  nav button {
    display: flex;
    align-items: center;
    gap: 9px;
    min-height: 30px;
    flex-shrink: 0;
    padding: 6px 10px;
    border: 0;
    border-radius: 4px;
    background: transparent;
    color: var(--ui-text-muted);
    font-size: 12px;
    text-align: left;
    cursor: pointer;
  }
  nav button:hover {
    color: var(--ui-text);
    background: var(--ui-glass);
  }
  nav button.active {
    color: var(--ui-mod-chip-active-text);
    background: var(--ui-mod-chip-active-bg);
  }
  nav .category-button {
    min-height: 27px;
    padding: 4px 10px 4px 34px;
    justify-content: space-between;
    font-size: 12px;
  }
  .main-content {
    display: flex;
    flex-direction: column;
    min-width: 0;
    flex: 1;
  }
  .content {
    position: relative;
    flex: 1;
    min-height: 0;
    overflow: hidden;
  }
  .section-wrapper {
    position: absolute;
    inset: 0;
    visibility: hidden;
    overflow-y: auto;
    overflow-x: hidden;
  }
  .section-wrapper.active {
    visibility: visible;
  }
  .pagination-footer {
    display: flex;
    justify-content: center;
    padding: 10px 18px;
    border-top: 1px solid var(--ui-border);
  }
  .pagination-controls {
    display: flex;
    gap: 4px;
  }
  .pagination-controls button {
    background: var(--ui-surface);
    border: 1px solid var(--ui-border);
    color: var(--ui-text-muted);
    padding: 5px 10px;
    border-radius: 4px;
    font-size: 12px;
    cursor: pointer;
  }
  .pagination-controls button:hover:not(:disabled),
  .pagination-controls button.active {
    color: var(--ui-text);
    background: var(--ui-mod-chip-active-bg);
    border-color: var(--ui-success);
  }
  .pagination-controls button:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  @media (max-width: 900px) {
    .sidebar {
      width: 172px;
    }
    header {
      padding: 0 16px;
    }
    .version-text {
      display: none;
    }
  }
</style>
