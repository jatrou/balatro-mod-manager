<script lang="ts">
  import { fade, scale } from "svelte/transition";
  import { deleteBackupPopupStore } from "../stores/modStore";
  import { backupsStore } from "../stores/backups";
  import { addMessage } from "$lib/stores";

  let isProcessing = $state(false);

  async function handleDelete() {
    if (isProcessing) return;
    isProcessing = true;

    const backupId = $deleteBackupPopupStore.backupId;
    const success = await backupsStore.deleteBackup(backupId);

    if (success) {
      addMessage("Backup deleted", "success");
    } else if ($backupsStore.error) {
      addMessage($backupsStore.error, "error");
    }

    isProcessing = false;
    handleClose();
  }

  function handleClose() {
    deleteBackupPopupStore.set({
      visible: false,
      backupId: "",
      backupName: "",
    });
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "Escape" && !isProcessing) {
      handleClose();
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

{#if $deleteBackupPopupStore.visible}
  <div class="dialog-overlay" transition:fade={{ duration: 160 }}>
    <div
      class="dialog-content"
      transition:scale={{ duration: 160, start: 0.96 }}
      role="dialog"
      aria-modal="true"
    >
      <h3>Delete Backup?</h3>
      <p class="dialog-text">
        Are you sure you want to delete
        <strong>"{$deleteBackupPopupStore.backupName}"</strong>?
      </p>
      <p class="dialog-warning">This action cannot be undone.</p>
      <div class="dialog-actions">
        <button
          class="delete-button"
          onclick={handleDelete}
          disabled={isProcessing}
        >
          {isProcessing ? "Deleting..." : "Delete"}
        </button>
        <button
          class="cancel-button"
          onclick={handleClose}
          disabled={isProcessing}
        >
          Cancel
        </button>
      </div>
    </div>
  </div>
{/if}

<style>
  .dialog-overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.8);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 2000;
    backdrop-filter: blur(2px);
  }

  :global([data-platform="linux"]) .dialog-overlay {
    backdrop-filter: none;
    background: rgba(0, 0, 0, 0.92);
  }

  .dialog-content {
    width: 480px;
    max-width: 90vw;
    background: var(--ui-surface);
    color: var(--ui-text);
    border: 1px solid var(--ui-border);
    border-radius: 12px;
    padding: 20px;
    box-shadow: 0 10px 28px rgba(0, 0, 0, 0.45);
  }

  h3 {
    color: var(--ui-accent);
    font-size: 22px;
    margin: 0 0 1rem;
    font-family: var(--ui-font);
    text-align: center;
  }

  .dialog-text {
    color: var(--ui-text);
    font-size: 14px;
    margin: 0 0 0.75rem;
    line-height: 1.5;
  }

  .dialog-text strong {
    color: var(--ui-accent);
  }

  .dialog-warning {
    color: #e1a3a8;
    font-size: 14px;
    margin: 0 0 1.5rem;
    line-height: 1.4;
  }

  .dialog-actions {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 1rem;
  }

  button {
    border: none;
    border-radius: 6px;
    padding: 0.9rem 1.25rem;
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

  .delete-button {
    background: var(--ui-danger);
    color: var(--ui-text);
    border: 1px solid var(--ui-border);
  }

  .delete-button:hover:not(:disabled) {
    background: var(--ui-danger-hover);
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
    .dialog-content {
      padding: 16px;
    }

    h3 {
      font-size: 18px;
    }

    .dialog-text {
      font-size: 1rem;
    }

    .dialog-actions {
      grid-template-columns: 1fr;
    }
  }
  button:focus-visible {
    outline: 2px solid var(--ui-accent);
    outline-offset: 2px;
  }
</style>
