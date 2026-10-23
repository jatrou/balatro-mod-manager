<script lang="ts">
  import PathSelector from "../PathSelector.svelte";
  import {
    Settings2,
    RefreshCw,
    Folder,
    Save,
    Info,
    Copy,
  } from "lucide-svelte";
  import { addMessage } from "$lib/stores";
  import { onMount } from "svelte";
  import { fade, fly } from "svelte/transition";
  import { invoke } from "@tauri-apps/api/core";
  import {
    cachedVersions,
    catalogLastRefreshed,
    catalogResetNonce,
    currentJokerView,
    currentModView,
    modsStore,
    searchResults,
  } from "../../stores/modStore";
  import { descriptionsStore } from "../../stores/descriptions";
  import {
    cardScale,
    modsPerPage,
    showModPreviews,
    MODS_PER_PAGE_OPTIONS,
  } from "../../stores/ui";
  import { browser } from "$app/environment";
  import { openExternal } from "$lib/opener";

  let isReindexing = $state(false);
  let isClearingCache = $state(false);
  let isConsoleEnabled = $state(false);
  let lastReindexStats = $state({
    removedFiles: 0,
    cleanedEntries: 0,
  });
  let isDiscordRpcEnabled = $state(false);
  let isLinux = $state(false);
  let linuxPrefix = $state("");
  let isCompatHelperEnabled = $state(false);
  let showLinuxHelp = $state(false);
  let activeHelpImage = $state<{ src: string; alt: string } | null>(null);

  export async function performReindexMods() {
    isReindexing = true;
    try {
      const result = await invoke<[number, number]>("reindex_mods");
      lastReindexStats = {
        removedFiles: result[0], // Will always be 0
        cleanedEntries: result[1],
      };
      addMessage(
        `Reindex complete: Cleaned ${result[1]} database entries`,
        "success",
      );
    } catch (error) {
      addMessage("Reindex failed: " + error, "error");
    } finally {
      isReindexing = false;
    }
  }

  async function clearCache() {
    isClearingCache = true;
    try {
      await invoke("clear_cache");
      // Also wipe on-disk catalog/thumbnail/details directories so a stale
      // entry can't keep crashing the next load.
      try {
        await invoke("clear_app_state");
      } catch (e) {
        console.warn("clear_app_state failed:", e);
      }
      // Also clear small UI caches persisted in localStorage
      try {
        localStorage.removeItem("version-cache-steamodded");
        localStorage.removeItem("version-cache-talisman");
        localStorage.removeItem("mods-cache");
        localStorage.removeItem("mods-cache-ts");
        localStorage.removeItem("mods-descriptions-cache");
      } catch (e) {
        // ignore storage errors
      }
      // Clear in-memory stores so the Mods view refetches fresh data.
      modsStore.set([]);
      searchResults.set([]);
      currentModView.set(null);
      currentJokerView.set(null);
      catalogLastRefreshed.set(null);
      catalogResetNonce.update((n) => n + 1);
      cachedVersions.set({ steamodded: [], talisman: [] });
      descriptionsStore.set({});
      addMessage("Successfully cleared all caches!", "success");
    } catch (error) {
      addMessage("Failed to clear cache: " + error, "error");
    } finally {
      isClearingCache = false;
    }
  }

  async function handleDiscordRpcChange() {
    const newValue = !isDiscordRpcEnabled;
    try {
      await invoke("set_discord_rpc_status", { enabled: newValue });
      isDiscordRpcEnabled = newValue;
      addMessage(
        `Discord Rich Presence ${newValue ? "enabled" : "disabled"}`,
        "success",
      );
    } catch (error) {
      console.error("Failed to set Discord RPC status:", error);
      addMessage("Failed to update Discord Rich Presence status", "error");
    }
  }

  async function openModsFolder() {
    try {
      const modsPath: string = await invoke("get_mods_folder");
      await invoke("open_directory", { path: modsPath });
    } catch (error) {
      addMessage(`Failed to open mods directory: ${error}`, "error");
    }
  }

  async function handleConsoleChange() {
    const newValue = !isConsoleEnabled;
    try {
      await invoke("set_lovely_console_status", { enabled: newValue });
      isConsoleEnabled = newValue;
      addMessage(
        `Lovely Console ${newValue ? "enabled" : "disabled"}`,
        "success",
      );
    } catch (error) {
      console.error("Failed to set console status:", error);
      addMessage("Failed to update Lovely Console status", "error");
    }
  }

  async function handleCompatHelperChange() {
    const newValue = !isCompatHelperEnabled;
    try {
      await invoke("set_compat_helper_status", { enabled: newValue });
      isCompatHelperEnabled = newValue;
      addMessage(
        `Compatibility helper ${newValue ? "enabled" : "disabled"}`,
        "success",
      );
    } catch (error) {
      console.error("Failed to set compatibility helper:", error);
      addMessage("Failed to update compatibility helper", "error");
    }
  }

  async function handleLinuxPrefixChange() {
    const newValue = linuxPrefix.replace(/\s+/g, " ").trim();
    if (newValue && newValue !== linuxPrefix) {
      linuxPrefix = newValue;
      addMessage("Linux prefix had extra spaces and was normalized", "warning");
    }
    try {
      await invoke("set_linux_prefix", { value: newValue });
      if (newValue) {
        addMessage(`Linux prefix set to ${newValue}`, "success");
      } else {
        addMessage("Linux prefix cleared (will use native LOVE)", "success");
      }
    } catch (error) {
      console.error("Failed to set prefix:", error);
      addMessage("Failed to update Linux prefix", "error");
    }
  }

  async function copyLinuxLaunchOptions() {
    const text = 'WINEDLLOVERRIDES="winmm=n,b" %command%';
    try {
      await navigator.clipboard.writeText(text);
      addMessage("Copied Steam launch options to clipboard", "success");
    } catch (error) {
      console.error("Failed to copy launch options:", error);
      addMessage("Failed to copy Steam launch options", "error");
    }
  }

  function toggleLinuxHelp() {
    showLinuxHelp = !showLinuxHelp;
  }

  function openHelpImage(src: string, alt: string) {
    activeHelpImage = { src, alt };
  }

  function closeHelpImage() {
    activeHelpImage = null;
  }

  function handleModalKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      closeHelpImage();
      return;
    }
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      closeHelpImage();
    }
  }

  interface AllSettings {
    discord_rpc: boolean;
    lovely_console: boolean;
    background_enabled: boolean;
    compat_helper: boolean;
    linux_prefix: string;
    launch_mode: string;
  }

  onMount(() => {
    (async () => {
      // Detect platform for Linux-specific UI gating
      try {
        const { platform } = await import("@tauri-apps/plugin-os");
        isLinux = (await platform()) === "linux";
      } catch (_) {
        if (browser) {
          isLinux =
            navigator.userAgent.toLowerCase().includes("linux") &&
            !navigator.userAgent.toLowerCase().includes("android");
        }
      }

      // Fetch all settings in a single batched IPC call
      try {
        const settings = await invoke<AllSettings>("get_all_settings");
        isDiscordRpcEnabled = settings.discord_rpc;
        isConsoleEnabled = settings.lovely_console;
        isCompatHelperEnabled = settings.compat_helper;
        if (isLinux) {
          linuxPrefix = settings.linux_prefix;
          if (!linuxPrefix) {
            linuxPrefix = "steam -applaunch 2379780";
            await invoke("set_linux_prefix", { value: linuxPrefix });
            addMessage(
              "Linux prefix defaulted to steam -applaunch 2379780",
              "info",
            );
          }
        }
      } catch (error) {
        console.error("Failed to load settings:", error);
        addMessage("Error loading settings", "error");
      }
    })();
  });
</script>

<div class="container default-scrollbar">
  <div class="settings-container">
    <h2>Settings</h2>
    <div class="content">
      <h3>Browsing</h3>
      <div class="preference-row">
        <label for="mods-per-page">Mods per page</label>
        <select id="mods-per-page" bind:value={$modsPerPage}>
          {#each MODS_PER_PAGE_OPTIONS as count}
            <option value={count}>{count === "all" ? "All mods" : count}</option
            >
          {/each}
        </select>
      </div>
      <p class="description-small">
        Applies to every mod list and is remembered between sessions.
      </p>
      <label class="preview-preference">
        <input type="checkbox" bind:checked={$showModPreviews} />
        Show mod previews
      </label>
      <p class="description-small">
        You can also change this from any mod list.
      </p>
      <!-- Card size slider -->
      <div class="slider-row">
        <div class="slider-label">
          <span class="label-text">Card Size</span>
          <span class="value">{Math.round($cardScale * 100)}%</span>
        </div>
        <input
          class="range"
          type="range"
          min="0.75"
          max="1.4"
          step="0.05"
          bind:value={$cardScale}
          aria-label="Card size"
        />
      </div>
      <p class="description-small">
        Adjust how large mod cards render. Smaller cards fit more per row.
      </p>

      <h3>Game Path</h3>
      <PathSelector />
      <h3>Cache</h3>
      <button
        class="clear-cache-button"
        onclick={clearCache}
        disabled={isClearingCache}
      >
        {#if isClearingCache}
          <div class="throbber"></div>
        {:else}
          <RefreshCw size={20} />
          Clear Cache
        {/if}
      </button>

      <p class="description warning">
        <span class="warning-icon">⚠️</span>
        Frequent cache clearing may trigger API rate limits
      </p>

      <h3>Mods</h3>

      <div class="mods-settings">
        <button
          class="open-folder-button"
          onclick={openModsFolder}
          title="Open mods folder"
        >
          <Folder size={20} />
          Open Mods Folder
        </button>

        <p class="description">
          Open the folder where mods are stored on your system.
        </p>
        <div class="console-settings">
          <span class="label-text">Compatibility Helper (Experimental)</span>
          <div class="switch-container">
            <label class="switch">
              <input
                type="checkbox"
                checked={isCompatHelperEnabled}
                onchange={handleCompatHelperChange}
              /> <span class="slider"></span>
            </label>
          </div>
        </div>
        <p class="description-small">
          Enables a hidden Lovely helper that runs compatibility checks before
          mods load.
        </p>

        <button
          class="reindex-button"
          onclick={performReindexMods}
          disabled={isReindexing}
          title="Synchronize database with filesystem state"
        >
          {#if isReindexing}
            <div class="throbber"></div>
            Scanning...
          {:else}
            <Settings2 size={20} />
            Validate Mod Database
          {/if}
        </button>

        {#if lastReindexStats.removedFiles + lastReindexStats.cleanedEntries > 0}
          <div class="reindex-stats">
            <strong>Last cleanup:</strong>
            <span>Files removed: {lastReindexStats.removedFiles}</span>
            <span
              >Database entries cleaned: {lastReindexStats.cleanedEntries}</span
            >
          </div>
        {/if}
        <p class="description-small">
          Performs consistency check on the mod database. Will only remove:
          <br />• Database entries for missing mod installations
        </p>
        {#if isLinux}
          <div class="linux-note">
            <span class="linux-note-icon">
              <Info size={18} />
            </span>
            <div class="linux-note-content">
              <strong>Linux Steam launch:</strong>
              Set Steam launch options for Balatro to
              <code>WINEDLLOVERRIDES="winmm=n,b" %command%</code> so Lovely and
              mods load when using <code>steam -applaunch 2379780</code>.
              <div class="linux-note-actions">
                <button
                  class="linux-copy-button"
                  onclick={copyLinuxLaunchOptions}
                >
                  <Copy size={16} />
                  Copy launch options
                </button>
                <button class="linux-what-button" onclick={toggleLinuxHelp}>
                  {showLinuxHelp ? "Hide" : "What?"}
                </button>
                <button
                  class="linux-what-button"
                  onclick={() =>
                    openExternal(
                      "https://github.com/ethangreen-dev/lovely-injector/blob/master/README.md",
                    )}
                >
                  Official Lovely guide
                </button>
              </div>
              {#if showLinuxHelp}
                <div class="linux-help">
                  <figure>
                    <button
                      type="button"
                      class="linux-help-button"
                      onclick={() =>
                        openHelpImage(
                          "/images/steam-help-first.png",
                          "Open Balatro properties from Steam library",
                        )}
                      aria-label="Open step 1 help image"
                    >
                      <img
                        src="/images/steam-help-first.png"
                        alt="Open Balatro properties from Steam library"
                        draggable="false"
                      />
                    </button>
                    <figcaption>
                      Right-click Balatro, open Properties, and paste the line
                      shown above into Launch Options.
                    </figcaption>
                  </figure>
                </div>
              {/if}
            </div>
          </div>
          {#if activeHelpImage}
            <div
              class="image-modal"
              role="button"
              aria-label="Close image preview"
              tabindex="0"
              onclick={closeHelpImage}
              onkeydown={handleModalKeydown}
              transition:fade={{ duration: 120 }}
            >
              <div
                class="image-modal-content"
                role="presentation"
                onclick={(event) => event.stopPropagation()}
                transition:fly={{ y: 12, duration: 180 }}
              >
                <button class="image-modal-close" onclick={closeHelpImage}>
                  ×
                </button>
                <img
                  src={activeHelpImage.src}
                  alt={activeHelpImage.alt}
                  draggable="false"
                />
                <p>{activeHelpImage.alt}</p>
              </div>
            </div>
          {/if}
          <input
            type="text"
            bind:value={linuxPrefix}
            placeholder="Linux prefix (e.g. proton, protontricks-launch)"
            class="prefix-input"
          />
          <button
            class="prefix-button"
            onclick={handleLinuxPrefixChange}
            title="Update Linux prefix"
          >
            <Save size={20} />
            Save Prefix
          </button>
          <p class="description-small">
            Launch command for Linux (Proton/Wine/Steam). Leave blank to use
            native LOVE. Use `{"{exe}"}` to place the Balatro.exe path, or
            `steam -applaunch 2379780` to launch via Steam. Lovely requires
            `WINEDLLOVERRIDES=winmm=n,b` and uses the Proton Mods folder (linked
            from your host Mods dir).
          </p>
        {/if}
      </div>
      <div class="console-settings">
        <span class="label-text">Enable Discord Rich Presence</span>
        <div class="switch-container">
          <label class="switch">
            <input
              type="checkbox"
              checked={isDiscordRpcEnabled}
              onchange={handleDiscordRpcChange}
            /> <span class="slider"></span>
          </label>
        </div>
      </div>
      <p class="description-small">
        Show your Balatro activity in Discord. Displays your current status and
        mod manager usage.
      </p>

      <h3>Privacy</h3>
      <p class="description-small">
        This maintained fork does not collect or send analytics or usage data.
      </p>

      <h3>Developer Options</h3>
      <div class="console-settings">
        <span class="label-text">Enable Lovely Console</span>
        <div class="switch-container">
          <label class="switch">
            <input
              type="checkbox"
              checked={isConsoleEnabled}
              onchange={handleConsoleChange}
            /> <span class="slider"></span>
          </label>
        </div>
      </div>
    </div>
  </div>
</div>

<style>
  .settings-container {
    padding: 20px;
    max-width: 1000px;
  }
  h2 {
    margin: 0 0 20px;
    font-size: 1.45rem;
    color: var(--ui-heading);
  }
  h3 {
    margin: 24px 0 12px;
    font-size: 1.05rem;
    font-weight: 600;
    color: var(--ui-heading);
  }
  h3:first-child {
    margin-top: 0;
  }
  .content {
    max-width: 760px;
  }
  button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    border: 1px solid var(--ui-border);
    border-radius: 5px;
    padding: 7px 10px;
    background: var(--ui-button-blue);
    color: var(--ui-text);
    font: inherit;
    font-size: 0.875rem;
    cursor: pointer;
  }
  button:hover:not(:disabled) {
    background: var(--ui-button-blue-hover);
  }
  button:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  button:focus-visible,
  input:focus-visible,
  select:focus-visible {
    outline: 2px solid var(--ui-accent);
    outline-offset: 2px;
  }
  button :global(svg) {
    width: 16px;
    height: 16px;
  }
  .open-folder-button,
  .prefix-button {
    margin-top: 8px;
  }
  .description,
  .description-small {
    color: var(--ui-text-muted);
    font-size: 0.875rem;
    margin: 6px 0 14px;
    line-height: 1.55;
    max-width: 680px;
  }
  .warning {
    border-left: 2px solid var(--ui-warning-border);
    padding-left: 10px;
  }
  .warning-icon {
    display: none;
  }
  .console-settings,
  .preference-row,
  .preview-preference {
    display: flex;
    align-items: center;
    gap: 12px;
    font-size: 0.875rem;
    color: var(--ui-text);
  }
  .console-settings {
    margin-top: 12px;
    justify-content: space-between;
    max-width: 420px;
  }
  .preference-row {
    justify-content: space-between;
    max-width: 420px;
  }
  .preview-preference {
    width: fit-content;
    cursor: pointer;
  }
  .preview-preference input {
    width: 16px;
    height: 16px;
    accent-color: var(--ui-accent);
  }
  select,
  .prefix-input {
    background: var(--ui-input-bg);
    color: var(--ui-text);
    border: 1px solid var(--ui-input-border);
    border-radius: 5px;
    padding: 7px 9px;
    font: inherit;
  }
  select {
    min-width: 110px;
  }
  .prefix-input {
    display: block;
    width: 100%;
    box-sizing: border-box;
    margin-top: 12px;
  }
  .switch {
    display: inline-flex;
    align-items: center;
  }
  .switch input {
    width: 16px;
    height: 16px;
    margin: 0;
    accent-color: var(--ui-accent);
  }
  .slider {
    display: none;
  }
  .slider-row {
    max-width: 420px;
    margin-top: 14px;
  }
  .slider-label {
    display: flex;
    justify-content: space-between;
    font-size: 0.875rem;
  }
  .value {
    color: var(--ui-text-muted);
  }
  .range {
    width: 100%;
    accent-color: var(--ui-accent);
    margin: 8px 0 0;
  }
  .linux-note {
    display: flex;
    gap: 8px;
    border: 1px solid var(--ui-border);
    border-radius: 6px;
    padding: 12px;
    margin-top: 12px;
    background: var(--ui-note-bg);
    font-size: 0.875rem;
    line-height: 1.55;
  }
  .linux-note-icon {
    color: var(--ui-note-icon);
  }
  .linux-note-content {
    min-width: 0;
  }
  .linux-note code {
    font-family: ui-monospace, monospace;
    overflow-wrap: anywhere;
  }
  .linux-note-actions {
    display: flex;
    gap: 6px;
    margin-top: 8px;
  }
  .linux-help {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
    gap: 12px;
    margin-top: 12px;
  }
  .linux-help figure {
    margin: 0;
  }
  .linux-help-button {
    padding: 0;
    cursor: zoom-in;
  }
  .linux-help img {
    width: 100%;
    display: block;
    border-radius: 5px;
  }
  .linux-help figcaption {
    font-size: 0.8rem;
    color: var(--ui-text-muted);
    margin-top: 5px;
  }
  .image-modal {
    position: fixed;
    inset: 0;
    z-index: 1000;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 24px;
    background: var(--ui-modal-overlay);
  }
  .image-modal-content {
    position: relative;
    max-width: min(1100px, 92vw);
    padding: 16px;
    border: 1px solid var(--ui-border);
    border-radius: 8px;
    background: var(--ui-help-bg);
  }
  .image-modal-content img {
    width: 100%;
    max-height: 75vh;
    object-fit: contain;
  }
  .image-modal-content p {
    margin: 8px 0 0;
    font-size: 0.875rem;
  }
  .image-modal-close {
    position: absolute;
    top: 8px;
    right: 8px;
  }
  .throbber {
    width: 16px;
    height: 16px;
    border: 2px solid var(--ui-spinner);
    border-top-color: transparent;
    border-radius: 50%;
    animation: spin 1s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
  @media (max-width: 650px) {
    .settings-container {
      padding: 14px;
    }
  }
</style>
