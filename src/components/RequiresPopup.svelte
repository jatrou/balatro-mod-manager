<script lang="ts">
  import { CircleAlert } from "lucide-svelte";
  import { fade, scale } from "svelte/transition";
  import { invoke } from "@tauri-apps/api/core";
  import { requiresPopupStore } from "../stores/modStore";

  let steamoddedInstalled = $state(false);
  let talismanInstalled = $state(false);

  async function checkInstallations() {
    if ($requiresPopupStore.requiresSteamodded) {
      steamoddedInstalled = await invoke("check_mod_installation", {
        modType: "Steamodded",
      });
    }
    if ($requiresPopupStore.requiresTalisman) {
      talismanInstalled = await invoke("check_mod_installation", {
        modType: "Talisman",
      });
    }
  }

  function handleProceedClick() {
    $requiresPopupStore.onProceed();
    requiresPopupStore.update((s) => ({ ...s, visible: false }));
  }

  function handleClose() {
    requiresPopupStore.update((s) => ({ ...s, visible: false }));
  }

  function handleDependencyClick(dependency: string) {
    $requiresPopupStore.onDependencyClick(dependency);
    requiresPopupStore.update((s) => ({ ...s, visible: false }));
  }

  $effect(() => {
    if ($requiresPopupStore.visible) {
      checkInstallations();
    }
  });
</script>

{#if $requiresPopupStore.visible}
  <div class="popup-overlay" transition:fade={{ duration: 200 }}>
    <div
      class="popup-content"
      transition:scale={{ duration: 200, start: 0.9, opacity: 0 }}
    >
      <div class="popup-header">
        <CircleAlert size={26} color="#fdcf51" />
        <h2>Required Dependencies</h2>
      </div>
      <div class="popup-body">
        <p>This mod requires the following missing dependencies:</p>
        <ul>
          {#if $requiresPopupStore.requiresSteamodded && !steamoddedInstalled}
            <li>
              <!-- Accessible clickable Steamodded link -->
              <span
                class="dependency clickable"
                role="button"
                tabindex="0"
                onclick={(e) => {
                  e.stopPropagation();
                  handleDependencyClick("Steamodded");
                }}
                onkeydown={(e) => {
                  if (e.key === "Enter" || e.key === " ") {
                    e.preventDefault();
                    handleDependencyClick("Steamodded");
                  }
                }}
              >
                Steamodded
              </span>
              - Core modding framework
            </li>
          {/if}
          {#if $requiresPopupStore.requiresTalisman && !talismanInstalled}
            <li>
              <!-- Accessible clickable Talisman link -->
              <span
                class="dependency clickable"
                role="button"
                tabindex="0"
                onclick={(e) => {
                  e.stopPropagation();
                  handleDependencyClick("Talisman");
                }}
                onkeydown={(e) => {
                  if (e.key === "Enter" || e.key === " ") {
                    e.preventDefault();
                    handleDependencyClick("Talisman");
                  }
                }}
              >
                Talisman
              </span>
              - Extended modding API
            </li>
          {/if}
        </ul>

        {#if ($requiresPopupStore.requiresSteamodded && !steamoddedInstalled) || ($requiresPopupStore.requiresTalisman && !talismanInstalled)}
          <p>It's recommended to install these first.</p>
        {:else}
          <p>All required dependencies seem to be installed.</p>
        {/if}

        <div class="button-container">
          <button class="proceed-button" onclick={handleProceedClick}>
            Download Anyway
          </button>
          <button class="cancel-button" onclick={handleClose}> Close </button>
        </div>
      </div>
    </div>
  </div>
{/if}

<style>
  .dependency.clickable {
    cursor: pointer;
    transition: all 0.2s ease;
    position: relative;
    display: inline-block;
    text-decoration: underline; /* Add underline to indicate it's clickable */
    text-underline-offset: 5px; /* Add some space between text and underline */
  }

  .dependency.clickable:hover {
    color: var(--ui-text);
    transform: none;
    text-decoration-thickness: 2px; /* Make underline thicker on hover */
  }

  .dependency.clickable:hover::after {
    content: "Open mod page";
    position: absolute;
    bottom: -25px;
    left: 0;
    background: rgba(0, 0, 0, 0.8);
    color: var(--ui-text);
    padding: 4px 8px;
    border-radius: 4px;
    font-size: 0.8rem;
    white-space: nowrap;
    pointer-events: none;
    z-index: 10;
  }
  /* Styles remain the same */
  .popup-overlay {
    position: fixed;
    top: 0;
    left: 0;
    width: 100%;
    height: 100%;
    background: rgba(0, 0, 0, 0.8);
    display: flex;
    justify-content: center;
    align-items: center;
    z-index: 2000;
  }

  .popup-content {
    background: var(--ui-surface);
    border: 1px solid var(--ui-border);
    border-radius: 10px;
    padding: 16px;
    width: 420px;
    max-width: 90%;
  }

  .popup-header {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    margin-bottom: 1rem;
  }

  .popup-header h2 {
    color: var(--ui-accent);
    font-size: 18px;
    margin: 0;
  }

  .popup-body {
    color: var(--ui-text);
    font-size: 14px;
  }

  .popup-body p {
    margin-bottom: 1rem;
  }

  .popup-body ul {
    list-style: none;
    padding: 0;
    margin-bottom: 1.25rem;
  }

  .popup-body li {
    margin-bottom: 0.75rem;
    display: flex;
    align-items: center;
    gap: 0.5rem;
    font-size: 14px;
  }

  .dependency {
    color: var(--ui-accent);
    font-weight: bold;
    font-size: 14px;
  }

  .button-container {
    display: flex;
    gap: 0.75rem;
    justify-content: flex-end;
  }

  .cancel-button,
  .proceed-button {
    padding: 0.6rem 1.2rem;
    color: var(--ui-text);
    border: none;
    border-radius: 5px;
    font-family: var(--ui-font);
    font-size: 1rem;
    cursor: pointer;
    transition: all 0.2s ease;
  }

  .cancel-button {
    background: var(--ui-danger);
    outline: var(--ui-border) solid 1px;
  }

  .cancel-button:hover {
    background: var(--ui-danger-hover);
    transform: none;
  }

  .proceed-button {
    background: var(--ui-surface);
    outline: var(--ui-border) solid 1px;
  }

  .proceed-button:hover {
    background: var(--ui-glass-strong);
    transform: none;
  }

  @media (max-width: 1160px) {
    .popup-content {
      padding: 16px;
      width: 90%;
      max-width: 400px;
    }
    .popup-header h2 {
      font-size: 18px;
    }
    .popup-body {
      font-size: 1rem;
    }
    .popup-body li {
      font-size: 1rem;
      margin-bottom: 0.75rem;
    }
    .dependency {
      font-size: 14px;
    }
    .cancel-button,
    .proceed-button {
      padding: 0.75rem 1.25rem;
      font-size: 1rem;
      border-radius: 4px;
    }
    .popup-header {
      gap: 0.5rem;
      margin-bottom: 1rem;
    }
  }
  button:focus-visible {
    outline: 2px solid var(--ui-accent);
    outline-offset: 2px;
  }
</style>
