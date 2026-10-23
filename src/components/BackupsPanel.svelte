<script lang="ts">
  import { onMount } from "svelte";
  import { backupsStore, formatBytes } from "../stores/backups";
  import { invoke } from "@tauri-apps/api/core";
  import BackupCard from "./BackupCard.svelte";
  import { addMessage } from "$lib/stores";
  import { Save, FolderOpen } from "lucide-svelte";
  import { createBackupPopupStore } from "../stores/modStore";

  onMount(() => {
    backupsStore.load();
  });

  async function handleOpenFolder() {
    try {
      const dir = await backupsStore.getBackupsDirectory();
      if (dir) {
        await invoke("open_directory", { path: dir });
      } else {
        addMessage("Failed to get backups directory", "error");
      }
    } catch (e) {
      addMessage(
        `Failed to open backups folder: ${e instanceof Error ? e.message : String(e)}`,
        "error",
      );
    }
  }

  function handleCreateClick() {
    createBackupPopupStore.set({ visible: true });
  }
</script>

<div class="backups-container">
  <h2>Backups</h2>

  <div class="actions-row">
    <button class="action-button primary" onclick={handleCreateClick}>
      <Save size={18} />
      Create Backup
    </button>
    <button class="action-button secondary" onclick={handleOpenFolder}>
      <FolderOpen size={18} />
      Open Backups Folder
    </button>
  </div>

  {#if $backupsStore.totalSize > 0}
    <p class="size-info">
      Total backup size: <span class="highlight"
        >{formatBytes($backupsStore.totalSize)}</span
      >
    </p>
  {/if}

  {#if $backupsStore.loading && $backupsStore.backups.length === 0}
    <div class="status-message">Loading backups...</div>
  {:else if $backupsStore.error && $backupsStore.backups.length === 0}
    <div class="status-message error">{$backupsStore.error}</div>
  {:else if $backupsStore.backups.length === 0}
    <div class="empty-state">
      <p>No backups yet.</p>
      <p class="hint">
        Create a backup to save your current mods configuration.
      </p>
    </div>
  {:else}
    <div class="backups-list">
      {#each $backupsStore.backups as backup (backup.id)}
        <BackupCard {backup} />
      {/each}
    </div>
  {/if}
</div>

<style>
  .backups-container {
    padding: 16px;
  }

  h2 {
    color: var(--ui-text);
    font-size: 20px;
    margin: 0 0 16px;
    font-family: var(--ui-font);
  }

  .actions-row {
    display: flex;
    gap: 8px;
    margin-bottom: 16px;
    flex-wrap: wrap;
  }

  .action-button {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 7px 12px;
    border-radius: 6px;
    font-family: var(--ui-font);
    font-size: 14px;
    cursor: pointer;
    transition:
      transform 0.15s ease,
      background-color 0.15s ease;
    box-shadow: none;
  }

  .action-button:hover {
    transform: none;
    box-shadow: none;
  }

  .action-button:active {
    transform: translateY(0);
    box-shadow: none;
  }

  .action-button.primary {
    background: var(--ui-success);
    color: var(--ui-text);
    border: 1px solid var(--ui-border);
  }

  .action-button.primary:hover {
    background: var(--ui-success-hover);
  }

  .action-button.secondary {
    background: var(--ui-surface);
    color: var(--ui-text);
    border: 1px solid var(--ui-border);
  }

  .action-button.secondary:hover {
    background: var(--ui-glass-strong);
  }

  .size-info {
    color: var(--ui-text);
    font-family: var(--ui-font);
    font-size: 14px;
    margin-bottom: 1rem;
  }

  .highlight {
    color: var(--ui-accent);
  }

  .status-message {
    color: var(--ui-text);
    font-family: var(--ui-font);
    font-size: 14px;
    padding: 20px;
    text-align: center;
    opacity: 0.8;
  }

  .status-message.error {
    color: #e1a3a8;
  }

  .empty-state {
    text-align: center;
    padding: 20px 1rem;
    color: var(--ui-text);
    font-family: var(--ui-font);
  }

  .empty-state p {
    margin: 0.5rem 0;
    font-size: 16px;
  }

  .empty-state .hint {
    font-size: 14px;
    opacity: 0.7;
  }

  .backups-list {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }

  @media (max-width: 768px) {
    .backups-container {
      padding: 1rem;
    }

    h2 {
      font-size: 18px;
    }

    .actions-row {
      flex-direction: column;
    }

    .action-button {
      width: 100%;
      justify-content: center;
    }
  }
  button:focus-visible {
    outline: 2px solid var(--ui-accent);
    outline-offset: 2px;
  }
</style>
