<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { FolderDot, FileDigit } from "lucide-svelte";
  import type { DialogFilter } from "@tauri-apps/plugin-dialog";
  import { addMessage } from "$lib/stores";
  import { onMount } from "svelte";
  import { browser } from "$app/environment";

  let selectedPath = "";
  let placeholder = "Choose Balatro Path";
  let isLoading = false;
  let selectMode = "directory"; // Can be "directory" or "executable"
  let isLinux = false;
  let showSteamOnlyNotice = false;

  onMount(async () => {
    try {
      const { platform } = await import("@tauri-apps/plugin-os");
      isLinux = (await platform()) === "linux";
      showSteamOnlyNotice = isLinux;
    } catch (_) {
      if (browser) {
        isLinux =
          navigator.userAgent.toLowerCase().includes("linux") &&
          !navigator.userAgent.toLowerCase().includes("android");
        showSteamOnlyNotice = isLinux;
      }
    }
  });

  const truncatePath = (path: string) => {
    const maxLength = 50;
    return path.length > maxLength
      ? path.substring(0, maxLength - 3) + "..."
      : path;
  };

  const getBalatroPath = async () => {
    const path = await invoke("get_balatro_path");
    if (path) {
      selectedPath = path as string;
      placeholder = path as string;
    } else {
      placeholder = "Choose Balatro Path";
    }
  };

  getBalatroPath();

  const handlePathSelect = async () => {
    const { open } = await import("@tauri-apps/plugin-dialog");
    const { platform } = await import("@tauri-apps/plugin-os");

    let filters: DialogFilter[] | null = null;

    if (selectMode === "executable") {
      const currentPlatform = await platform();
      filters =
        currentPlatform === "macos"
          ? [{ name: "App Bundle", extensions: ["app"] }]
          : [{ name: "Executable", extensions: ["exe"] }];
    }

    const selected = await open({
      directory: selectMode === "directory",
      multiple: false,
      title:
        selectMode === "directory"
          ? "Select Balatro Directory"
          : "Select Balatro Executable",
      filters: filters ?? undefined,
    });

    if (selected) {
      selectedPath = selected as string;

      // If executable was selected, get the parent directory
      let pathToCheck = selectedPath;
      if (selectMode === "executable") {
        // Extract directory from file path
        const lastSlashIndex =
          selectedPath.lastIndexOf("/") !== -1
            ? selectedPath.lastIndexOf("/")
            : selectedPath.lastIndexOf("\\");

        if (lastSlashIndex !== -1) {
          pathToCheck = selectedPath.substring(0, lastSlashIndex);
        }
      }

      const isValid = await invoke("check_custom_balatro", {
        path: pathToCheck,
      });

      if (isValid) {
        const storedPath = (await invoke("get_balatro_path")) as
          string | null | void;
        const normalizedPath =
          typeof storedPath === "string" && storedPath.length > 0
            ? storedPath
            : pathToCheck;
        selectedPath = normalizedPath;
        placeholder = normalizedPath;
        addMessage(
          "Balatro path set successfully! You can now manage mods for this installation.",
          "success",
        );
      } else {
        addMessage(
          "Invalid Balatro path. Please select a directory containing the game files.",
          "error",
        );
        selectedPath = "";
      }
    }
  };

  const setSteamPath = async () => {
    isLoading = true;
    try {
      const paths: string[] = await invoke("find_steam_balatro");
      if (paths.length === 0) {
        addMessage("Balatro not found in Steam installation", "error");
      } else {
        selectedPath = paths[0];
        await invoke("set_balatro_path", { path: selectedPath });
        addMessage("Successfully set Steam path", "success");
      }
    } catch (error) {
      addMessage("Error finding Steam path: " + error, "error");
    } finally {
      isLoading = false;
    }
  };
</script>

<div class="path-selector">
  {#if !isLinux}
    <div class="toggle-buttons">
      <button
        class="path-type-button"
        class:active={selectMode === "directory"}
        on:click={() => (selectMode = "directory")}
      >
        <FolderDot size={16} />
        Select Directory
      </button>
      <button
        class="path-type-button"
        class:active={selectMode === "executable"}
        on:click={() => (selectMode = "executable")}
      >
        <FileDigit size={16} />
        Select Executable
      </button>
    </div>

    <p class="description-small">
      {#if selectMode === "executable"}
        Select a Balatro executable to manage mods for a specific installation.
        This is useful for separate vanilla/modded setups.
      {:else}
        Select the Balatro game directory. This is the standard method.
      {/if}
    </p>

    <div class="input-container">
      <input
        type="text"
        placeholder={truncatePath(placeholder)}
        value={selectedPath ? truncatePath(selectedPath) : ""}
        on:click={handlePathSelect}
        readonly
        disabled={isLoading}
      />
    </div>
  {:else if showSteamOnlyNotice}
    <p class="description-small">
      On Linux, Balatro must be launched via Steam. Click below to set the Steam
      path.
    </p>
  {/if}
  <button class="steam-button" on:click={setSteamPath} disabled={isLoading}>
    {#if isLoading}
      <div class="throbber"></div>
    {:else}
      <FolderDot size={20} />
      Set to Steam Path
    {/if}
  </button>
</div>

<style>
  .path-selector {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 8px;
    margin: 12px 0;
  }
  .toggle-buttons {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    padding: 7px 10px;
    border: 1px solid var(--ui-border);
    border-radius: 5px;
    background: var(--ui-button-blue);
    color: var(--ui-text);
    font: inherit;
    font-size: 0.875rem;
    cursor: pointer;
  }
  button:hover:not(:disabled) {
    background: var(--ui-button-blue-hover);
  }
  .path-type-button.active {
    border-color: var(--ui-accent);
    background: var(--ui-button-blue-active);
  }
  button:disabled {
    cursor: not-allowed;
    opacity: 0.5;
  }
  button:focus-visible,
  input:focus-visible {
    outline: 2px solid var(--ui-accent);
    outline-offset: 2px;
  }
  .description-small {
    color: var(--ui-text-muted);
    font-size: 0.875rem;
    margin: 0;
    line-height: 1.55;
    max-width: 650px;
  }
  .input-container {
    width: min(100%, 500px);
  }
  input[type="text"] {
    box-sizing: border-box;
    width: 100%;
    padding: 8px 10px;
    border: 1px solid var(--ui-input-border);
    border-radius: 5px;
    background: var(--ui-input-bg);
    color: var(--ui-text);
    font: inherit;
    font-size: 0.875rem;
    cursor: pointer;
    text-overflow: ellipsis;
  }
  input[type="text"]::placeholder {
    color: var(--ui-input-placeholder);
  }
  .throbber {
    width: 16px;
    height: 16px;
    border: 2px solid var(--ui-spinner);
    border-radius: 50%;
    border-top-color: transparent;
    animation: spin 1s linear infinite;
  }
  .steam-button :global(svg) {
    width: 16px;
    height: 16px;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
