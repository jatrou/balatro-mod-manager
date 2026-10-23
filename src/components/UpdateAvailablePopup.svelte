<script lang="ts">
  import { fade, scale } from "svelte/transition";
  import { onMount, onDestroy } from "svelte";
  import { openExternal } from "$lib/opener";
  import { updatePopupStore } from "../stores/modStore";
  import { RELEASES_URL } from "$lib/releases";

  async function handleDownload() {
    try {
      await openExternal(RELEASES_URL);
    } catch (e) {
      console.error("Failed to open download page:", e);
    }
    $updatePopupStore.onClose();
    updatePopupStore.update((s) => ({ ...s, visible: false }));
  }

  function handleClose() {
    $updatePopupStore.onClose();
    updatePopupStore.update((s) => ({ ...s, visible: false }));
  }

  function handleDontShow() {
    $updatePopupStore.onDontShow();
    updatePopupStore.update((s) => ({ ...s, visible: false }));
  }

  let keyHandler: ((e: KeyboardEvent) => void) | null = null;
  onMount(() => {
    keyHandler = (e: KeyboardEvent) => {
      if (e.key === "Escape" && $updatePopupStore.visible) {
        e.preventDefault();
        handleClose();
      }
    };
    window.addEventListener("keydown", keyHandler);
  });
  onDestroy(() => {
    if (keyHandler) window.removeEventListener("keydown", keyHandler);
  });
</script>

{#if $updatePopupStore.visible}
  <div class="modal-background" transition:fade={{ duration: 160 }}>
    <div
      class="modal"
      transition:scale={{ duration: 160, start: 0.95, opacity: 1 }}
    >
      <h2>Update Available</h2>
      <p>A newer version of Balatro Mod Manager is available.</p>
      <p class="ver">
        Current: <span class="version">v{$updatePopupStore.currentVersion}</span
        >
        • Latest:
        <span class="version">v{$updatePopupStore.latestVersion}</span>
      </p>

      <div class="buttons">
        <button class="download-button" onclick={handleDownload}
          >Download</button
        >
        <button class="close-button" onclick={handleClose}>Close</button>
        <button class="dontshow-button" onclick={handleDontShow}
          >Don't show anymore</button
        >
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
    max-width: 420px;
    width: 86%;
    text-align: center;
  }
  h2 {
    color: var(--ui-accent);
    margin-bottom: 0.75rem;
    font-family: var(--ui-font);
  }
  p,
  .ver {
    color: var(--ui-text);
    font-size: 14px;
    font-family: var(--ui-font);
    margin: 0.25rem 0 0.75rem 0;
  }
  .ver .version {
    color: var(--ui-accent);
  }
  .buttons {
    display: flex;
    justify-content: center;
    gap: 0.6rem;
    flex-wrap: wrap;
    margin-top: 16px;
  }
  button {
    padding: 0.6rem 1.2rem;
    border: none;
    border-radius: 4px;
    cursor: pointer;
    font-size: 1rem;
    transition: all 0.2s ease;
    font-family: var(--ui-font);
    color: var(--ui-text);
  }
  .download-button {
    background: var(--ui-info);
    outline: var(--ui-border) solid 1px;
  }
  .close-button {
    background: var(--ui-success);
    outline: var(--ui-border) solid 1px;
  }
  .dontshow-button {
    background: var(--ui-danger);
    outline: var(--ui-border) solid 1px;
  }
  button:hover {
    opacity: 0.9;
    transform: none;
  }
  button:focus-visible {
    outline: 2px solid var(--ui-accent);
    outline-offset: 2px;
  }
</style>
