<script lang="ts">
  import { fade } from "svelte/transition";
  import { invoke } from "@tauri-apps/api/core";
  import { X } from "lucide-svelte";

  export let show: boolean = false;
  export let onClose: () => void;

  let disableLaunchButton = false;
  let disableCheckButton = false;

  let isError: boolean = false;

  async function handleLaunch() {
    disableLaunchButton = true;
    disableCheckButton = true;
    try {
      await invoke("launch_balatro");
      onClose();
      return;
    } catch (error) {
      console.error("Failed to launch game:", error);
      return;
    } finally {
      disableLaunchButton = false;
      disableCheckButton = false;
    }
  }

  async function handleCheckAgain() {
    disableCheckButton = true;
    try {
      let steam_running: boolean = await invoke("check_steam_running");
      if (!steam_running) {
        isError = true;
        setTimeout(() => {
          isError = false;
        }, 2000);
      } else {
        await invoke("launch_balatro");
        onClose();
        return;
      }
    } catch (error) {
      console.error("Failed to check if Steam is running:", error);
    } finally {
      disableCheckButton = false;
    }
  }

  $: if (!show) {
    disableLaunchButton = false;
    disableCheckButton = false;
  }
</script>

{#if show}
  <div class="overlay" transition:fade={{ duration: 100 }}>
    <div class="alert-box">
      <button class="close-button" on:click={onClose} class:hidden={isError}>
        <div class="close-icon-container">
          <X size={13} />
        </div>
      </button>
      <div class="content" class:hidden={isError}>
        <h2 id="alert-title-first">Steam is not running. Are you sure?</h2>
        <div class="button-container">
          <button
            class="launch-button"
            on:click={handleLaunch}
            disabled={disableLaunchButton}
          >
            {#if disableLaunchButton}
              Launching...
            {:else}
              Yes, launch without Steam
            {/if}
          </button>
          <button
            class="check-button"
            on:click={handleCheckAgain}
            disabled={disableCheckButton}
          >
            {#if disableCheckButton}
              Checking...
            {:else}
              Check again
            {/if}
          </button>
        </div>
      </div>
      {#if isError}
        <div class="error-message" transition:fade={{ duration: 200 }}>
          <h2 id="alert-title" class:shake={isError}>
            Steam is still not running!
          </h2>
        </div>
      {/if}
    </div>
  </div>
{/if}

<style>
  .overlay {
    position: fixed;
    top: 0;
    left: 0;
    width: 100%;
    height: 100%;
    background: rgba(0, 0, 0, 0.7);
    display: flex;
    justify-content: center;
    align-items: center;
    z-index: 1000;
  }

  .alert-box {
    box-sizing: border-box;
    max-height: 90vh;
    overflow-y: auto;
    background: var(--ui-surface);
    border: 1px solid var(--ui-border);
    border-radius: 8px;
    padding: 20px;
    width: 400px;
    text-align: center;
    position: relative;
    overflow: hidden;
  }

  .content {
    transition: opacity 0.2s;
    margin-bottom: 1rem;
  }

  .content.hidden {
    opacity: 0;
  }

  #alert-title {
    color: var(--ui-text);
    margin-bottom: 20px;
    font-family: var(--ui-font);
    font-size: 22px;
  }

  #alert-title.shake {
    animation: shake 0.5s cubic-bezier(0.36, 0.07, 0.19, 0.97) both;
  }

  #alert-title-first {
    color: var(--ui-text);
    font-family: var(--ui-font);
    font-size: 18px;
  }

  .error-message {
    position: absolute;
    top: 0;
    left: 0;
    width: 100%;
    height: 100%;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(193, 65, 57, 0.9);
  }

  .close-button {
    position: absolute;
    top: 0.5rem;
    right: 0.5rem;
    background: transparent;
    border: none;
    cursor: pointer;
    padding: 0;
    z-index: 1000;
  }

  .close-button.hidden {
    display: none;
  }

  .close-icon-container {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 25px;
    height: 25px;
    border-radius: 4px; /* Changed from 50% to 4px for rounded corners */
    background: var(--ui-surface);
    /* border: 1px solid var(--ui-border); */
    color: var(--ui-text);
    transition: all 0.2s ease;
  }

  .close-button:hover .close-icon-container {
    background: var(--ui-danger);
    transform: scale(1.1);
    color: var(--ui-text);
  }

  .close-button:active .close-icon-container {
    transform: scale(0.9);
  }

  @keyframes shake {
    10%,
    90% {
      transform: translate3d(-1px, 0, 0);
    }
    20%,
    80% {
      transform: translate3d(2px, 0, 0);
    }
    30%,
    50%,
    70% {
      transform: translate3d(-4px, 0, 0);
    }
    40%,
    60% {
      transform: translate3d(4px, 0, 0);
    }
  }

  .launch-button {
    background: var(--ui-danger);
    border: 1px solid var(--ui-border);
    color: var(--ui-text);
    padding: 0.75rem 16px;
    border-radius: 4px;
    cursor: pointer;
    font-family: var(--ui-font);
    font-size: 14px;
    transition: all 0.2s ease;
  }
  .launch-button:hover {
    background: var(--ui-danger);
    border: 1px solid var(--ui-border);
  }
  .launch-button:active {
    transform: none;
  }

  .check-button {
    background: var(--ui-success);
    border: 1px solid var(--ui-border);
    color: var(--ui-text);
    padding: 0.75rem 16px;
    border-radius: 4px;
    cursor: pointer;
    font-family: var(--ui-font);
    font-size: 14px;
    transition: all 0.2s ease;
  }

  .check-button:hover {
    background: var(--ui-success);
    border: 1px solid var(--ui-border);
  }
  .check-button:active {
    transform: none;
  }
  button:focus-visible {
    outline: 2px solid var(--ui-accent);
    outline-offset: 2px;
  }
</style>
