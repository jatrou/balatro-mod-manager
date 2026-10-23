<script lang="ts">
  import { fade, scale } from "svelte/transition";
  import { showWarningPopup } from "../stores/modStore";

  export let visible: boolean = false;
  export let message: string = "";
  export let onConfirm: () => void;
  export let onCancel: () => void;

  function handleConfirm() {
    onConfirm();
    showWarningPopup.update((p) => ({ ...p, visible: false }));
  }

  function handleCancel() {
    onCancel();
    showWarningPopup.update((p) => ({ ...p, visible: false }));
  }
</script>

{#if visible}
  <div class="modal-background" transition:fade={{ duration: 160 }}>
    <div
      class="modal"
      transition:scale={{ duration: 160, start: 0.95, opacity: 1 }}
    >
      <h2>Warning</h2>
      <p>{message}</p>
      <div class="buttons">
        <button class="cancel-button" on:click={handleCancel}>Cancel</button>
        <button class="confirm-button" on:click={handleConfirm}>Confirm</button>
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
    max-width: 500px;
    width: 90%;
    text-align: center;
  }
  h2 {
    color: #e1a3a8;
    margin-bottom: 1rem;
    font-family: var(--ui-font);
  }
  p {
    color: var(--ui-text);
    font-size: 14px;
    margin-bottom: 20px;
    font-family: var(--ui-font);
  }
  .buttons {
    display: flex;
    justify-content: center;
    gap: 1rem;
  }
  button {
    padding: 0.8rem 16px;
    border: none;
    border-radius: 4px;
    cursor: pointer;
    font-size: 1rem;
    transition: all 0.2s ease;
    font-family: var(--ui-font);
  }
  .cancel-button {
    background: var(--ui-danger);
    outline: var(--ui-border) solid 1px;
    color: var(--ui-text);
  }
  .confirm-button {
    background: var(--ui-success);
    outline: var(--ui-border) solid 1px;
    color: var(--ui-text);
  }
  button:hover {
    opacity: 0.9;
    scale: 1;
    transition: all 0.2s ease;
  }
  button:focus-visible {
    outline: 2px solid var(--ui-accent);
    outline-offset: 2px;
  }
</style>
