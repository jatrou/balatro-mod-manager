<script lang="ts">
  import { fade, scale } from "svelte/transition";
  import { addMessage } from "$lib/stores";
  import {
    collectionImportStore,
    closeCollectionImport,
    importCollectionCode,
  } from "../stores/collections";

  let importCode = $state("");

  $effect(() => {
    if ($collectionImportStore.open) {
      importCode = $collectionImportStore.code;
    }
  });

  function close() {
    closeCollectionImport();
    importCode = "";
  }

  function handleImport() {
    const result = importCollectionCode(importCode);
    if (!result.ok) {
      addMessage(result.error || "Failed to import collection.", "error");
      return;
    }
    addMessage("Collection imported.", "success");
    close();
  }
</script>

{#if $collectionImportStore.open}
  <div
    class="import-backdrop"
    transition:fade={{ duration: 160 }}
    role="button"
    tabindex="0"
    onpointerdown={close}
    onkeydown={(e) => (e.key === "Escape" || e.key === "Esc") && close()}
  >
    <div
      class="import-modal"
      role="dialog"
      aria-modal="true"
      tabindex="-1"
      transition:scale={{ duration: 160, start: 0.96 }}
      onpointerdown={(e) => e.stopPropagation()}
      onkeydown={(e) => (e.key === "Escape" || e.key === "Esc") && close()}
    >
      <h3>Import collection</h3>
      <p class="subtitle">Paste a collection code.</p>
      <textarea
        class="import-textarea"
        placeholder="BMMCOLL1:..."
        bind:value={importCode}
      ></textarea>
      <div class="import-actions">
        <button class="ghost neutral" type="button" onclick={close}>
          Cancel
        </button>
        <button class="primary" type="button" onclick={handleImport}>
          Import
        </button>
      </div>
    </div>
  </div>
{/if}

<style>
  .import-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.55);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 2400;
  }

  .import-modal {
    box-sizing: border-box;
    max-height: 90vh;
    overflow-y: auto;
    width: 520px;
    max-width: 92vw;
    background: var(--ui-surface);
    color: var(--ui-text);
    border: 1px solid var(--ui-border);
    border-radius: 10px;
    padding: 20px;
    box-shadow: 0 10px 28px rgba(0, 0, 0, 0.45);
  }

  h3 {
    margin: 0;
    font-family: var(--ui-font);
    font-size: 18px;
  }

  .subtitle {
    margin: 0.4rem 0 1rem;
    color: var(--ui-text-muted);
    font-size: 14px;
  }

  .import-textarea {
    width: 100%;
    box-sizing: border-box;
    min-height: 140px;
    background: var(--ui-input-bg);
    color: var(--ui-text);
    border: 1px solid var(--ui-border);
    border-radius: 6px;
    padding: 0.7rem 0.8rem;
    font-family: var(--ui-font);
    font-size: 1rem;
    resize: vertical;
  }

  .import-textarea:focus {
    outline: none;
    border-color: var(--ui-mod-input-focus);
    box-shadow: 0 0 0 2px var(--ui-mod-input-focus-glow);
  }

  .import-textarea::selection {
    background: var(--ui-mod-chip-active-bg);
    color: var(--ui-mod-chip-active-text);
  }

  .import-actions {
    display: flex;
    justify-content: flex-end;
    gap: 0.7rem;
    margin-top: 1rem;
  }

  .primary,
  .ghost {
    border: 1px solid var(--ui-border);
    background: var(--ui-surface);
    color: var(--ui-text);
    padding: 0.6rem 1rem;
    border-radius: 6px;
    font-family: var(--ui-font);
    font-size: 1rem;
    cursor: pointer;
    transition:
      transform 0.15s ease,
      box-shadow 0.15s ease,
      background 0.15s ease;
  }

  .ghost.neutral {
    background: var(--ui-surface);
    color: var(--ui-text);
  }

  .primary:hover,
  .ghost:hover {
    transform: none;
    box-shadow: 0 4px 10px rgba(0, 0, 0, 0.2);
  }

  .primary:active,
  .ghost:active {
    transform: none;
    box-shadow: none;
  }
  button:focus-visible {
    outline: 2px solid var(--ui-accent);
    outline-offset: 2px;
  }
</style>
