<script lang="ts">
  import { fade, scale } from "svelte/transition";
  import { invoke } from "@tauri-apps/api/core";
  import { securityPopupStore } from "../stores/modStore";

  let isLoading: boolean = false;
  let errorMessage: string | null = null;

  async function handleAcknowledge() {
    try {
      isLoading = true;
      errorMessage = null;

      // Always store acknowledgment when user clicks "I Understand"
      await invoke("set_security_warning_acknowledged", {
        acknowledged: true,
      });

      // Then call the parent component's callback
      $securityPopupStore.onAcknowledge();
      securityPopupStore.update((s) => ({ ...s, visible: false }));
    } catch (error) {
      console.error("Failed to save security acknowledgment:", error);
      errorMessage = "Failed to save your preference. Please try again.";
    } finally {
      isLoading = false;
    }
  }

  function handleQuit() {
    // Close the application
    invoke("exit_application");
    $securityPopupStore.onCancel();
    securityPopupStore.update((s) => ({ ...s, visible: false }));
  }
</script>

{#if $securityPopupStore.visible}
  <div class="modal-background" transition:fade={{ duration: 160 }}>
    <div
      class="modal"
      transition:scale={{ duration: 160, start: 0.95, opacity: 1 }}
    >
      <h2>Security Notice</h2>
      <p>
        Mods are created and maintained by third-party developers. While the
        index is curated, we cannot guarantee their safety. Please install only
        if you trust the source.
      </p>
      {#if errorMessage}
        <p class="error-message">{errorMessage}</p>
      {/if}
      <div class="buttons">
        <button
          class="cancel-button"
          on:click={handleQuit}
          disabled={isLoading}
        >
          Quit
        </button>
        <button
          class="confirm-button"
          on:click={handleAcknowledge}
          disabled={isLoading}
        >
          {#if isLoading}
            Saving...
          {:else}
            I Understand
          {/if}
        </button>
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
    margin-bottom: 16px;
    font-family: var(--ui-font);
  }
  .error-message {
    color: #e1a3a8;
    font-size: 0.9rem;
    margin-bottom: 1rem;
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
  button:disabled {
    opacity: 0.6;
    cursor: not-allowed;
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
  button:hover:not(:disabled) {
    opacity: 0.9;
    scale: 1;
    transition: all 0.2s ease;
  }
  button:focus-visible {
    outline: 2px solid var(--ui-accent);
    outline-offset: 2px;
  }
</style>
