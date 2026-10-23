<script lang="ts">
  import { fade, scale } from "svelte/transition";
  import { invoke } from "@tauri-apps/api/core";
  import { addMessage } from "$lib/stores";
  import { lovelyPopupStore } from "../stores/modStore";

  let isWindows = $state<boolean>(false);
  let balatroPath = $state<string | null>(null);
  let normalizedPath = $state<string | null>(null);
  let installing = $state(false);
  let installError: string | null = $state(null);

  async function init() {
    // Simple OS check for showing Windows instructions
    const ua = navigator.userAgent.toLowerCase();
    isWindows = ua.includes("windows");

    try {
      const path = await invoke<string | null>("get_balatro_path");
      balatroPath = path ?? null;
    } catch (e) {
      balatroPath = null;
    }

    // Normalize path for Windows Explorer address bar
    if (isWindows && balatroPath) {
      let p = balatroPath;
      // Ensure backslashes
      p = p.replace(/\//g, "\\");
      // Collapse repeated backslashes to a single backslash
      p = p.replace(/\\{2,}/g, "\\");
      normalizedPath = p;
    } else {
      normalizedPath = balatroPath;
    }
  }

  $effect(() => {
    if ($lovelyPopupStore.visible) {
      init();
    }
  });

  async function copyPath() {
    if (!normalizedPath) {
      addMessage(
        "Game path not set; open Settings and set it first.",
        "warning",
      );
      return;
    }
    try {
      await navigator.clipboard.writeText(normalizedPath);
      addMessage("Copied to clipboard", "success");
    } catch (e) {
      addMessage("Failed to copy path", "error");
    }
  }

  function close() {
    lovelyPopupStore.update((s) => ({ ...s, visible: false }));
  }

  async function installLovely() {
    installError = null;
    installing = true;
    try {
      // Install/Update Lovely to latest which also sets DB version
      await invoke<string>("update_lovely_to_latest");
      addMessage("Lovely installed/updated to latest", "success");
      // Verify installation
      try {
        const present = await invoke<boolean>("is_lovely_installed");
        if (!present) {
          installError =
            "Lovely still not detected. Add a Defender exclusion and try again.";
          return;
        }
      } catch (_) {
        // ignore post-check errors
      }
      close();
      // If this popup was from a launch attempt, offer to continue via callback
      const cb = $lovelyPopupStore.onLaunchAnyway;
      if ($lovelyPopupStore.source === "launch" && cb) {
        await cb();
      }
    } catch (e) {
      installError = e instanceof Error ? e.message : String(e);
      addMessage(`Failed to install Lovely: ${installError}`, "error");
    } finally {
      installing = false;
    }
  }
</script>

{#if $lovelyPopupStore.visible}
  <div class="modal-background" transition:fade={{ duration: 160 }}>
    <div
      class="modal"
      transition:scale={{ duration: 160, start: 0.95, opacity: 1 }}
    >
      <h2>Lovely Not Detected</h2>
      <p>The Lovely injector was not detected. Mods require Lovely to load.</p>

      {#if isWindows}
        <div class="steps">
          <p>Windows Defender may remove the injector DLL. To prevent this:</p>
          <ol>
            <li>
              Open Windows Security → Virus & threat protection → Manage
              settings.
            </li>
            <li>
              Temporarily disable Real-time protection (Windows re-enables it
              automatically later).
            </li>
            <li>Scroll to Add or remove exclusions and confirm if prompted.</li>
            <li>Add a folder exclusion for the Balatro directory.</li>
          </ol>
          <div class="copy-row">
            <button class="copy-button" onclick={copyPath}>
              Copy Balatro Path
            </button>
            {#if normalizedPath}
              <span class="path">{normalizedPath}</span>
            {/if}
          </div>
        </div>
      {/if}

      {#if installError}
        <p class="error">{installError}</p>
      {/if}

      <div class="buttons">
        <button
          class="install-button"
          onclick={installLovely}
          disabled={installing}
        >
          {#if installing}Installing...{:else}Install Lovely{/if}
        </button>
        {#if $lovelyPopupStore.source === "launch" && $lovelyPopupStore.onLaunchAnyway}
          <button
            class="launch-anyway"
            onclick={() => {
              const cb = $lovelyPopupStore.onLaunchAnyway;
              close();
              cb && cb();
            }}
          >
            Launch Anyway
          </button>
        {/if}
        <button class="close-button" onclick={close}>Close</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .modal-background {
    position: fixed;
    top: 0;
    left: 0;
    width: 100%;
    height: 100%;
    background: rgba(0, 0, 0, 0.6);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 2000;
  }
  .modal {
    background: var(--ui-surface);
    outline: 1px solid var(--ui-border);
    padding: 20px;
    border-radius: 8px;
    box-shadow: 0 0 15px rgba(0, 0, 0, 0.7);
    max-width: 560px;
    width: 92%;
    text-align: left;
  }
  h2 {
    color: var(--ui-accent);
    margin-bottom: 0.75rem;
    font-family: var(--ui-font);
  }
  p,
  li,
  .path {
    color: var(--ui-text);
    font-size: 14px;
    font-family: var(--ui-font);
  }
  ol {
    padding-left: 1.2rem;
    margin: 0.5rem 0 1rem 0;
  }
  .copy-row {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    margin-top: 0.5rem;
    flex-wrap: wrap;
  }
  .copy-button {
    background: var(--ui-surface);
    outline: var(--ui-border) solid 1px;
    color: var(--ui-text);
    padding: 0.6rem 1rem;
    border: none;
    border-radius: 4px;
    cursor: pointer;
    font-family: var(--ui-font);
    font-size: 14px;
    transition:
      transform 0.15s ease,
      box-shadow 0.15s ease,
      background 0.15s ease;
  }
  .copy-button:hover {
    transform: none;
    background: var(--ui-glass-strong);
    box-shadow: 0 4px 8px rgba(0, 0, 0, 0.3);
  }
  .copy-button:active {
    transform: none;
    box-shadow: none;
  }
  .buttons {
    display: flex;
    justify-content: flex-end;
    margin-top: 1rem;
  }
  .buttons {
    gap: 0.5rem;
    flex-wrap: wrap;
  }
  .close-button {
    background: var(--ui-success);
    outline: var(--ui-border) solid 1px;
    color: var(--ui-text);
    padding: 0.6rem 1.2rem;
    border: none;
    border-radius: 4px;
    cursor: pointer;
    font-family: var(--ui-font);
    font-size: 14px;
    transition:
      transform 0.15s ease,
      box-shadow 0.15s ease,
      background 0.15s ease;
  }
  .close-button:hover {
    transform: none;
    background: var(--ui-success-hover);
    box-shadow: 0 4px 8px rgba(0, 0, 0, 0.3);
  }
  .close-button:active {
    transform: none;
    box-shadow: none;
  }
  .install-button {
    background: var(--ui-info);
    outline: var(--ui-border) solid 1px;
    color: var(--ui-text);
    padding: 0.6rem 1.2rem;
    border: none;
    border-radius: 4px;
    cursor: pointer;
    font-family: var(--ui-font);
    font-size: 14px;
    transition:
      transform 0.15s ease,
      box-shadow 0.15s ease,
      background 0.15s ease;
  }
  .install-button:hover:not(:disabled) {
    transform: none;
    background: var(--ui-info);
    box-shadow: 0 4px 8px rgba(0, 0, 0, 0.3);
  }
  .install-button:active:not(:disabled) {
    transform: none;
    box-shadow: none;
  }
  .install-button:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }
  .launch-anyway {
    background: var(--ui-danger);
    outline: var(--ui-border) solid 1px;
    color: var(--ui-text);
    padding: 0.6rem 1.2rem;
    border: none;
    border-radius: 4px;
    cursor: pointer;
    font-family: var(--ui-font);
    font-size: 14px;
    transition:
      transform 0.15s ease,
      box-shadow 0.15s ease,
      background 0.15s ease;
  }
  .launch-anyway:hover {
    transform: none;
    background: var(--ui-danger-hover);
    box-shadow: 0 4px 8px rgba(0, 0, 0, 0.3);
  }
  .launch-anyway:active {
    transform: none;
    box-shadow: none;
  }
  .error {
    color: #e1a3a8;
    font-family: var(--ui-font);
    margin-top: 0.5rem;
  }
  @media (max-width: 1160px) {
    .modal {
      max-width: 90%;
      padding: 16px;
    }
  }
  button:focus-visible {
    outline: 2px solid var(--ui-accent);
    outline-offset: 2px;
  }
</style>
