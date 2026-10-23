<script lang="ts">
  import { fade, scale } from "svelte/transition";
  import { createBackupPopupStore } from "../stores/modStore";
  import { backupsStore } from "../stores/backups";
  import { addMessage } from "$lib/stores";

  let backupName = $state("");
  let isCreating = $state(false);

  async function handleCreate() {
    if (isCreating) return;
    isCreating = true;

    const backup = await backupsStore.createBackup(
      backupName.trim() || undefined,
    );
    if (backup) {
      addMessage("Backup created successfully", "success");
      handleClose();
    } else if ($backupsStore.error) {
      addMessage($backupsStore.error, "error");
    }
    isCreating = false;
  }

  function handleClose() {
    backupName = "";
    createBackupPopupStore.set({ visible: false });
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "Enter" && !isCreating) {
      handleCreate();
    } else if (e.key === "Escape") {
      handleClose();
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

{#if $createBackupPopupStore.visible}
  <div class="modal-overlay" transition:fade={{ duration: 160 }}>
    <div
      class="modal-content"
      transition:scale={{ duration: 160, start: 0.96 }}
      role="dialog"
      aria-modal="true"
    >
      <h3>Create Backup</h3>
      <p class="subtitle">
        Give your backup a name (optional). If left empty, it will be labeled as
        "Manual backup".
      </p>

      <div class="input-group">
        <label for="backup-name">Backup Name</label>
        <input
          id="backup-name"
          class="text-input"
          type="text"
          placeholder="My backup (optional)"
          bind:value={backupName}
        />
      </div>

      <div class="modal-actions">
        <button
          class="create-button"
          onclick={handleCreate}
          disabled={isCreating}
        >
          {isCreating ? "Creating..." : "Create"}
        </button>
        <button
          class="cancel-button"
          onclick={handleClose}
          disabled={isCreating}
        >
          Cancel
        </button>
      </div>
    </div>
  </div>
{/if}

<style>
  .modal-overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.8);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 2000;
    backdrop-filter: blur(2px);
  }

  :global([data-platform="linux"]) .modal-overlay {
    backdrop-filter: none;
    background: rgba(0, 0, 0, 0.92);
  }

  .modal-content {
    box-sizing: border-box;
    max-height: 90vh;
    overflow-y: auto;
    width: 480px;
    max-width: 90vw;
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
    font-size: 22px;
    color: var(--ui-accent);
  }

  .subtitle {
    margin: 0.5rem 0 1.5rem;
    color: var(--ui-text-muted);
    font-size: 14px;
    line-height: 1.5;
  }

  .input-group {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    margin-bottom: 16px;
  }

  label {
    font-family: var(--ui-font);
    font-size: 14px;
    color: var(--ui-accent);
  }

  .text-input {
    background: var(--ui-input-bg);
    color: var(--ui-text);
    border: 1px solid var(--ui-border);
    border-radius: 6px;
    padding: 0.75rem 1rem;
    font-family: var(--ui-font);
    font-size: 14px;
  }

  .text-input:focus {
    outline: none;
    border-color: var(--ui-mod-input-focus);
    box-shadow: 0 0 0 2px var(--ui-mod-input-focus-glow);
  }

  .text-input::placeholder {
    color: var(--ui-text-muted);
  }

  .text-input::selection {
    background: var(--ui-mod-chip-active-bg);
    color: var(--ui-mod-chip-active-text);
  }

  .modal-actions {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 1rem;
  }

  button {
    border: none;
    border-radius: 6px;
    padding: 8px 12px;
    font-family: var(--ui-font);
    font-size: 14px;
    cursor: pointer;
    transition:
      transform 0.2s ease,
      background-color 0.2s ease;
    box-shadow: none;
  }

  button:hover:not(:disabled) {
    transform: none;
    box-shadow: none;
  }

  button:active:not(:disabled) {
    transform: translateY(0);
    box-shadow: none;
  }

  button:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .create-button {
    background: var(--ui-success);
    color: var(--ui-text);
    border: 1px solid var(--ui-border);
  }

  .create-button:hover:not(:disabled) {
    background: var(--ui-success-hover);
  }

  .cancel-button {
    background: var(--ui-surface);
    color: var(--ui-text);
    border: 1px solid var(--ui-border);
  }

  .cancel-button:hover:not(:disabled) {
    background: var(--ui-glass-strong);
  }

  @media (max-width: 768px) {
    .modal-content {
      box-sizing: border-box;
      max-height: 90vh;
      overflow-y: auto;
      padding: 16px;
    }

    h3 {
      font-size: 18px;
    }

    .subtitle {
      font-size: 1rem;
    }

    .text-input {
      font-size: 1rem;
      padding: 0.65rem 0.8rem;
    }

    .modal-actions {
      grid-template-columns: 1fr;
    }

    button {
      padding: 0.8rem 1rem;
      font-size: 1rem;
    }
  }
  button:focus-visible {
    outline: 2px solid var(--ui-accent);
    outline-offset: 2px;
  }
</style>
