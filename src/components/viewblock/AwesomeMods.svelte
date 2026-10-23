<script lang="ts">
  import {
    BookOpen,
    ExternalLink,
    PackageCheck,
    RefreshCw,
    Search,
    ShieldAlert,
    Wrench,
  } from "lucide-svelte";
  import { onMount } from "svelte";
  import type { Mod } from "../../stores/modStore";
  import { installationStatus, modsStore } from "../../stores/modStore";
  import {
    AWESOME_BALATRO_README_URL,
    bundledAwesomeCatalog,
    findCatalogMatch,
    loadAwesomeCatalog,
    type AwesomeCatalog,
    type AwesomeEntry,
    type AwesomeLoader,
  } from "$lib/awesomeCatalog";
  import { openExternal } from "$lib/opener";
  import { getAwesomeInstallSource } from "$lib/awesomeInstall";
  import AwesomeInstallDialog from "../AwesomeInstallDialog.svelte";
  import {
    installedModsCache,
    fetchCachedMods,
    forceRefreshCache,
  } from "../../stores/modCache";

  let {
    onViewCatalog = () => {},
  }: {
    onViewCatalog?: (mod: Mod) => void;
  } = $props();

  let catalog = $state<AwesomeCatalog>(bundledAwesomeCatalog);
  let origin = $state<"live" | "cache" | "snapshot">("snapshot");
  let refreshWarning = $state("");
  let refreshing = $state(false);
  let query = $state("");
  let loaderFilter = $state<AwesomeLoader | "All">("All");
  let categoryFilter = $state("All");
  let view = $state<"mods" | "loaders">("mods");
  let installing = $state<AwesomeEntry | null>(null);
  let installedMessage = $state("");
  let displayRevision = $derived(
    /^[a-f0-9]{40}$/i.test(catalog.source.revision)
      ? catalog.source.revision.slice(0, 12)
      : catalog.source.revision,
  );

  const normalize = (value: string) =>
    value
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, " ")
      .trim();

  let modEntries = $derived(
    catalog.entries.filter((entry) => entry.kind === "mod"),
  );
  let loaderEntries = $derived(
    catalog.entries.filter((entry) => entry.kind === "loader"),
  );
  let matches = $derived(
    new Map(
      modEntries.map((entry) => [
        entry.id,
        findCatalogMatch(entry, $modsStore),
      ]),
    ),
  );
  let installedCount = $derived(
    modEntries.filter((entry) => isInstalled(entry)).length,
  );
  let availableCount = $derived(
    modEntries.filter(
      (entry) => matches.has(entry.id) && !!matches.get(entry.id),
    ).length,
  );
  let downloadCount = $derived(
    modEntries.filter(
      (entry) =>
        getAwesomeInstallSource(entry, matches.get(entry.id)).kind ===
        "download",
    ).length,
  );
  let categories = $derived(
    Array.from(
      new Set(
        modEntries
          .filter(
            (entry) => loaderFilter === "All" || entry.loader === loaderFilter,
          )
          .flatMap((entry) => entry.categories),
      ),
    ).sort((left, right) => left.localeCompare(right)),
  );
  let filteredEntries = $derived.by(() => {
    const source = view === "mods" ? modEntries : loaderEntries;
    const needle = normalize(query);
    return source.filter((entry) => {
      if (
        view === "mods" &&
        loaderFilter !== "All" &&
        entry.loader !== loaderFilter
      ) {
        return false;
      }
      if (
        view === "mods" &&
        categoryFilter !== "All" &&
        !entry.categories.includes(categoryFilter)
      ) {
        return false;
      }
      if (!needle) return true;
      return normalize(
        [
          entry.title,
          entry.description,
          entry.author,
          entry.loader,
          entry.categories.join(" "),
          entry.parentTitle || "",
        ].join(" "),
      ).includes(needle);
    });
  });

  onMount(() => {
    void fetchCachedMods();
    loadAwesomeCatalog().then((result) => {
      catalog = result.catalog;
      origin = result.origin;
    });
  });

  async function refreshFromGitHub(): Promise<void> {
    if (refreshing) return;
    refreshing = true;
    refreshWarning = "";
    const result = await loadAwesomeCatalog({ refresh: true });
    catalog = result.catalog;
    origin = result.origin;
    refreshWarning = result.warning || "";
    refreshing = false;
  }

  function switchView(next: "mods" | "loaders"): void {
    view = next;
    categoryFilter = "All";
    loaderFilter = "All";
  }

  function open(url: string): void {
    void openExternal(url);
  }

  function viewMatch(entry: AwesomeEntry): void {
    const match = matches.get(entry.id);
    if (match) onViewCatalog(match);
  }

  function isInstalled(entry: AwesomeEntry): boolean {
    const match = matches.get(entry.id);
    return (
      !!(match && (match.installed || $installationStatus[match.title])) ||
      $installedModsCache.some(
        (mod) => mod.name.toLowerCase() === entry.title.toLowerCase(),
      )
    );
  }

  function installed(path: string): void {
    installedMessage = `${installing?.title} installed into ${path}`;
    installing = null;
    void forceRefreshCache();
  }
</script>

{#if installing}
  <AwesomeInstallDialog
    entry={installing}
    source={getAwesomeInstallSource(installing, matches.get(installing.id))}
    folderName={matches.get(installing.id)?.folderName || installing.title}
    onClose={() => {
      installing = null;
    }}
    onInstalled={installed}
  />
{/if}

<div class="container default-scrollbar">
  <div class="awesome-view">
    <header>
      <div>
        <div class="eyebrow"><BookOpen size={16} /> Community directory</div>
        <h2>Awesome Balatro</h2>
        <p>
          Browse every mod listed by the community-maintained Awesome Balatro
          README. A catalog match means BMM currently lists that exact title and
          GitHub repository; it is not a compatibility guarantee.
        </p>
      </div>
      <button
        class="refresh-button"
        type="button"
        disabled={refreshing}
        onclick={refreshFromGitHub}
      >
        <span class:spinning={refreshing}><RefreshCw size={18} /></span>
        {refreshing ? "Refreshing…" : "Refresh from GitHub"}
      </button>
    </header>

    <div class="summary" aria-label="Awesome Balatro summary">
      <div><strong>{modEntries.length}</strong><span>Mods listed</span></div>
      <div><strong>{availableCount}</strong><span>In BMM catalog</span></div>
      <div><strong>{installedCount}</strong><span>Installed</span></div>
      <div><strong>{downloadCount}</strong><span>Direct downloads</span></div>
    </div>

    <div class="notice">
      <ShieldAlert size={20} />
      <span>
        Install archives and individual Lua mods directly, or import downloaded
        files from Discord. BMM checks the package and warns about conflicts
        before installation. Special loader and game-patching setups include
        source instructions.
      </span>
    </div>

    {#if installedMessage}<div class="notice" role="status">
        {installedMessage}
      </div>{/if}

    {#if refreshWarning}
      <div class="refresh-warning" role="status">
        GitHub refresh failed ({refreshWarning}). Showing the
        {origin === "cache" ? "last cached copy" : "bundled offline snapshot"}.
      </div>
    {/if}

    <div class="toolbar">
      <div class="tabs" role="tablist" aria-label="Directory type">
        <button
          type="button"
          role="tab"
          class:active={view === "mods"}
          aria-selected={view === "mods"}
          onclick={() => switchView("mods")}
        >
          Mods ({modEntries.length})
        </button>
        <button
          type="button"
          role="tab"
          class:active={view === "loaders"}
          aria-selected={view === "loaders"}
          onclick={() => switchView("loaders")}
        >
          Loaders ({loaderEntries.length})
        </button>
      </div>

      <label class="search">
        <Search size={18} />
        <span class="sr-only">Search Awesome Balatro</span>
        <input
          type="search"
          bind:value={query}
          placeholder="Search title, author, description…"
        />
      </label>

      {#if view === "mods"}
        <label>
          <span class="sr-only">Filter by loader</span>
          <select bind:value={loaderFilter}>
            <option value="All">All loaders</option>
            <option value="Steamodded">Steamodded</option>
            <option value="Lovely">Lovely</option>
            <option value="Balamod">Balamod</option>
            <option value="None">No listed loader</option>
          </select>
        </label>
        <label>
          <span class="sr-only">Filter by category</span>
          <select bind:value={categoryFilter}>
            <option value="All">All categories</option>
            {#each categories as category}
              <option value={category}>{category}</option>
            {/each}
          </select>
        </label>
      {/if}
    </div>

    <div class="results-line">
      <span
        >{filteredEntries.length} result{filteredEntries.length === 1
          ? ""
          : "s"}</span
      >
      <button
        type="button"
        class="source-link"
        onclick={() => open(AWESOME_BALATRO_README_URL)}
      >
        Upstream README <ExternalLink size={14} />
      </button>
    </div>

    {#if filteredEntries.length === 0}
      <div class="empty-state">No entries match these filters.</div>
    {:else}
      <div class="card-grid">
        {#each filteredEntries as entry (entry.id)}
          {@const match = matches.get(entry.id)}
          {@const installed = isInstalled(entry)}
          {@const source = getAwesomeInstallSource(entry, match)}
          <article class:safety={!!entry.safetyWarning}>
            <div class="card-heading">
              <div>
                <h3>{entry.title}</h3>
                {#if entry.parentTitle}
                  <p class="parent">Part of {entry.parentTitle}</p>
                {/if}
              </div>
              {#if installed}
                <span class="status installed"
                  ><PackageCheck size={14} /> Installed</span
                >
              {:else if match}
                <span class="status available">In BMM catalog</span>
              {:else}
                <span class="status manual"
                  >{entry.kind === "loader"
                    ? "Reference"
                    : source.kind === "download"
                      ? "Direct download"
                      : source.kind === "local"
                        ? "Import download"
                        : "Source instructions"}</span
                >
              {/if}
            </div>

            {#if entry.description}
              <p class="description">{entry.description}</p>
            {/if}
            {#if entry.author}
              <p class="author">by {entry.author}</p>
            {/if}

            <div class="chips">
              {#if entry.kind === "loader"}
                <span class="chip loader"><Wrench size={13} /> Mod loader</span>
              {:else}
                <span class="chip loader">{entry.loader}</span>
              {/if}
              {#each entry.categories as category}
                <span class="chip">{category}</span>
              {/each}
              {#each entry.badges as badge}
                <span class="chip badge" title={badge.url}>{badge.label}</span>
              {/each}
            </div>

            {#if entry.safetyWarning}
              <div class="safety-warning">
                <ShieldAlert size={17} />
                <span>{entry.safetyWarning}</span>
              </div>
            {/if}

            {#if source.kind === "unsupported" && !entry.safetyWarning && entry.kind !== "loader"}
              <p class="author">{source.reason}</p>
            {/if}

            <div class="actions">
              {#if source.kind !== "unsupported" && !installed}
                <button
                  type="button"
                  class="catalog-button"
                  onclick={() => {
                    installedMessage = "";
                    installing = entry;
                  }}
                >
                  {source.kind === "download"
                    ? "Install"
                    : source.discordUrl
                      ? "Download from Discord"
                      : "Import download"}
                </button>
              {/if}
              {#if match}
                <button
                  type="button"
                  class="catalog-button"
                  onclick={() => viewMatch(entry)}
                >
                  View in catalog
                </button>
              {/if}
              <button
                type="button"
                class="open-button"
                onclick={() => open(entry.sourceUrl)}
              >
                {match ? "Open source" : "Open source / Manual installation"}
                <ExternalLink size={15} />
              </button>
              {#each entry.links.slice(1) as link}
                <button
                  type="button"
                  class="secondary-link"
                  onclick={() => open(link.url)}
                >
                  {link.label}
                  <ExternalLink size={13} />
                </button>
              {/each}
            </div>
          </article>
        {/each}
      </div>
    {/if}

    <footer>
      Source: {catalog.source.repository} · revision {displayRevision}
      · {origin === "live"
        ? "live refresh"
        : origin === "cache"
          ? "cached copy"
          : "bundled snapshot"}
    </footer>
  </div>
</div>

<style>
  .awesome-view {
    padding: 1.5rem 2rem 2.5rem;
    color: var(--ui-text);
  }

  header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 2rem;
    margin-bottom: 1rem;
  }

  header > div {
    max-width: 780px;
  }

  h2 {
    color: var(--ui-heading);
    font-size: 2.4rem;
    margin: 0.15rem 0 0.45rem;
  }

  header p {
    color: var(--ui-text-soft);
    line-height: 1.45;
    margin: 0;
  }

  .eyebrow {
    align-items: center;
    color: var(--ui-heading);
    display: flex;
    font-size: 0.9rem;
    gap: 0.4rem;
    text-transform: uppercase;
  }

  button,
  input,
  select {
    font: inherit;
  }

  button {
    cursor: pointer;
  }

  button:disabled {
    cursor: wait;
    opacity: 0.7;
  }

  .refresh-button,
  .catalog-button,
  .open-button {
    align-items: center;
    border: 2px solid var(--ui-glass-border);
    border-radius: 7px;
    color: var(--ui-text);
    display: inline-flex;
    gap: 0.45rem;
    justify-content: center;
    padding: 0.55rem 0.8rem;
  }

  .refresh-button {
    background: var(--ui-button-blue);
    white-space: nowrap;
  }

  .refresh-button:hover:not(:disabled) {
    background: var(--ui-button-blue-hover);
  }

  .summary {
    display: grid;
    gap: 0.65rem;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    margin: 1rem 0;
  }

  .summary > div {
    background: var(--ui-glass);
    border: 1px solid var(--ui-glass-border);
    border-radius: 8px;
    display: flex;
    flex-direction: column;
    padding: 0.7rem 0.9rem;
  }

  .summary strong {
    color: var(--ui-heading);
    font-size: 1.45rem;
  }

  .summary span {
    color: var(--ui-text-muted);
    font-size: 0.82rem;
  }

  .notice,
  .refresh-warning {
    align-items: flex-start;
    background: var(--ui-note-bg);
    border: 1px solid var(--ui-note-border);
    border-radius: 7px;
    display: flex;
    gap: 0.55rem;
    line-height: 1.35;
    margin: 0.75rem 0;
    padding: 0.65rem 0.8rem;
  }

  .notice :global(svg) {
    color: var(--ui-note-icon);
    flex: 0 0 auto;
  }

  .refresh-warning {
    display: block;
  }

  .toolbar {
    align-items: center;
    display: flex;
    flex-wrap: wrap;
    gap: 0.65rem;
    margin-top: 1rem;
  }

  .tabs {
    display: flex;
    gap: 0.35rem;
  }

  .tabs button,
  .source-link,
  .secondary-link {
    align-items: center;
    background: var(--ui-glass);
    border: 1px solid var(--ui-glass-border);
    border-radius: 6px;
    color: var(--ui-text);
    display: inline-flex;
    gap: 0.3rem;
    padding: 0.5rem 0.65rem;
  }

  .tabs button.active {
    background: var(--ui-mod-chip-active-bg);
    color: var(--ui-mod-chip-active-text);
  }

  .search {
    align-items: center;
    background: var(--ui-input-bg);
    border: 2px solid var(--ui-input-border);
    border-radius: 7px;
    display: flex;
    flex: 1 1 260px;
    gap: 0.5rem;
    min-width: 210px;
    padding: 0 0.7rem;
  }

  .search input {
    background: transparent;
    border: 0;
    color: var(--ui-text);
    min-width: 0;
    outline: 0;
    padding: 0.62rem 0;
    width: 100%;
  }

  select {
    background: var(--ui-input-bg);
    border: 2px solid var(--ui-input-border);
    border-radius: 7px;
    color: var(--ui-text);
    padding: 0.56rem 2rem 0.56rem 0.65rem;
  }

  .results-line {
    align-items: center;
    color: var(--ui-text-muted);
    display: flex;
    justify-content: space-between;
    margin: 0.9rem 0 0.6rem;
  }

  .source-link {
    background: transparent;
    border: 0;
    color: var(--ui-heading);
    padding: 0.3rem;
  }

  .card-grid {
    display: grid;
    gap: 0.8rem;
    grid-template-columns: repeat(auto-fill, minmax(290px, 1fr));
  }

  article {
    background: var(--ui-panel-muted);
    border: 1px solid var(--ui-panel-muted-border);
    border-radius: 9px;
    box-shadow: 0 4px 12px var(--ui-glass-shadow);
    display: flex;
    flex-direction: column;
    min-height: 210px;
    padding: 0.9rem;
  }

  article.safety {
    border-color: var(--ui-warning-border);
  }

  .card-heading {
    align-items: flex-start;
    display: flex;
    gap: 0.65rem;
    justify-content: space-between;
  }

  h3 {
    color: var(--ui-heading);
    font-size: 1.2rem;
    line-height: 1.15;
    margin: 0;
  }

  .parent,
  .author {
    color: var(--ui-text-muted);
    font-size: 0.8rem;
    margin: 0.25rem 0 0;
  }

  .description {
    color: var(--ui-text-soft);
    font-size: 0.91rem;
    line-height: 1.38;
    margin: 0.7rem 0 0;
  }

  .status {
    border-radius: 999px;
    flex: 0 0 auto;
    font-size: 0.72rem;
    padding: 0.25rem 0.45rem;
  }

  .status.installed {
    align-items: center;
    background: var(--ui-success);
    color: white;
    display: inline-flex;
    gap: 0.25rem;
  }

  .status.available {
    background: var(--ui-info-strong);
    color: white;
  }

  .status.manual {
    background: var(--ui-neutral);
    color: white;
  }

  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 0.35rem;
    margin: 0.75rem 0;
  }

  .chip {
    background: var(--ui-glass);
    border: 1px solid var(--ui-glass-border);
    border-radius: 999px;
    color: var(--ui-text-soft);
    font-size: 0.72rem;
    padding: 0.2rem 0.42rem;
  }

  .chip.loader {
    align-items: center;
    background: var(--ui-mod-chip-bg);
    color: var(--ui-mod-chip-text);
    display: inline-flex;
    gap: 0.25rem;
  }

  .chip.badge {
    background: var(--ui-note-bg);
  }

  .safety-warning {
    align-items: flex-start;
    background: var(--ui-danger-overlay);
    border: 1px solid var(--ui-warning-border);
    border-radius: 6px;
    display: flex;
    font-size: 0.82rem;
    gap: 0.4rem;
    line-height: 1.3;
    margin-bottom: 0.7rem;
    padding: 0.5rem;
  }

  .safety-warning :global(svg) {
    flex: 0 0 auto;
  }

  .actions {
    align-items: center;
    display: flex;
    flex-wrap: wrap;
    gap: 0.4rem;
    margin-top: auto;
  }

  .catalog-button {
    background: var(--ui-success);
  }

  .catalog-button:hover {
    background: var(--ui-success-hover);
  }

  .open-button {
    background: var(--ui-button-blue);
  }

  .open-button:hover {
    background: var(--ui-button-blue-hover);
  }

  .secondary-link {
    font-size: 0.8rem;
    padding: 0.42rem 0.55rem;
  }

  .empty-state {
    background: var(--ui-glass);
    border: 1px solid var(--ui-glass-border);
    border-radius: 8px;
    color: var(--ui-text-muted);
    padding: 2rem;
    text-align: center;
  }

  footer {
    color: var(--ui-text-muted);
    font-size: 0.78rem;
    margin-top: 1.2rem;
    text-align: right;
  }

  .spinning {
    animation: spin 0.9s linear infinite;
  }

  .sr-only {
    height: 1px;
    margin: -1px;
    overflow: hidden;
    padding: 0;
    position: absolute;
    width: 1px;
    clip: rect(0, 0, 0, 0);
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  @media (max-width: 780px) {
    .awesome-view {
      padding: 1rem;
    }

    header {
      align-items: stretch;
      flex-direction: column;
      gap: 0.8rem;
    }

    .summary {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }

    .toolbar > label,
    .toolbar select {
      width: 100%;
    }
  }
</style>
