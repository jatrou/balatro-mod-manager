<script lang="ts">
  import type { Backup } from "../stores/backups";
  import {
    formatBytes,
    formatBackupDate,
    getBackupDisplayName,
  } from "../stores/backups";
  import {
    restoreBackupPopupStore,
    deleteBackupPopupStore,
  } from "../stores/modStore";
  import { Archive, RotateCcw } from "lucide-svelte";

  interface Props {
    backup: Backup;
  }

  let { backup }: Props = $props();

  const isManual = $derived(backup.trigger === "manual");

  function openRestoreConfirm() {
    restoreBackupPopupStore.set({
      visible: true,
      backupId: backup.id,
      backupName: getBackupDisplayName(backup),
    });
  }

  function openDeleteConfirm() {
    deleteBackupPopupStore.set({
      visible: true,
      backupId: backup.id,
      backupName: getBackupDisplayName(backup),
    });
  }
</script>

<div class="backup-card">
  <div class="backup-icon" class:manual={isManual}>
    {#if isManual}
      <Archive size={24} />
    {:else}
      <RotateCcw size={24} />
    {/if}
  </div>
  <div class="backup-info">
    <div class="backup-name">{getBackupDisplayName(backup)}</div>
    <div class="backup-metadata">
      <span class="date">{formatBackupDate(backup.created_at)}</span>
      <span class="separator">•</span>
      <span class="mod-count"
        >{backup.mod_count} mod{backup.mod_count !== 1 ? "s" : ""}</span
      >
      <span class="separator">•</span>
      <span class="size">{formatBytes(backup.size_bytes)}</span>
    </div>
    {#if backup.lovely_version}
      <div class="lovely-version">Lovely: {backup.lovely_version}</div>
    {/if}
  </div>
  <div class="backup-actions">
    <button class="restore-button" onclick={openRestoreConfirm}>
      Restore
    </button>
    <button class="delete-button" onclick={openDeleteConfirm}> Delete </button>
  </div>
</div>

<style>
  .backup-card {
    display: grid;
    grid-template-columns: auto 1fr auto;
    gap: 1rem;
    align-items: center;
    background: rgba(244, 238, 224, 0.05);
    border: 1px solid var(--ui-border);
    border-radius: 8px;
    padding: 12px 16px;
    transition:
      background 0.2s ease,
      border-color 0.2s ease;
  }

  .backup-card:hover {
    background: rgba(244, 238, 224, 0.08);
    border-color: var(--ui-border);
  }

  .backup-icon {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 36px;
    height: 36px;
    border-radius: 8px;
    background: var(--ui-glass);
    color: var(--ui-accent);
  }

  .backup-icon.manual {
    background: var(--ui-glass);
    color: var(--ui-accent);
  }

  .backup-info {
    display: flex;
    flex-direction: column;
    gap: 0.35rem;
    min-width: 0;
  }

  .backup-name {
    font-family: var(--ui-font);
    font-size: 16px;
    color: var(--ui-accent);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .backup-metadata {
    display: flex;
    gap: 0.5rem;
    align-items: center;
    font-size: 14px;
    color: var(--ui-text-muted);
  }

  .separator {
    opacity: 0.5;
  }

  .lovely-version {
    font-size: 1rem;
    color: var(--ui-text-muted);
  }

  .backup-actions {
    display: flex;
    gap: 0.5rem;
  }

  button {
    border: none;
    border-radius: 6px;
    padding: 7px 12px;
    font-family: var(--ui-font);
    font-size: 14px;
    cursor: pointer;
    transition:
      transform 0.2s ease,
      background-color 0.2s ease;
    box-shadow: none;
  }

  button:hover {
    transform: none;
    box-shadow: none;
  }

  button:active {
    transform: translateY(0);
    box-shadow: none;
  }

  .restore-button {
    background: var(--ui-success);
    color: var(--ui-text);
    border: 1px solid var(--ui-border);
  }

  .restore-button:hover {
    background: var(--ui-success-hover);
  }

  .delete-button {
    background: var(--ui-danger);
    color: var(--ui-text);
    border: 1px solid var(--ui-border);
  }

  .delete-button:hover {
    background: var(--ui-danger-hover);
  }

  @media (max-width: 768px) {
    .backup-card {
      grid-template-columns: 1fr;
      gap: 0.75rem;
      padding: 1rem;
    }

    .backup-icon {
      display: none;
    }

    .backup-metadata {
      flex-wrap: wrap;
      font-size: 0.9rem;
    }

    .backup-actions {
      width: 100%;
    }

    .backup-actions button {
      flex: 1;
      padding: 0.75rem;
    }
  }
  button:focus-visible {
    outline: 2px solid var(--ui-accent);
    outline-offset: 2px;
  }
</style>
