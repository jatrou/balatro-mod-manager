<script lang="ts">
  import {
    ExternalLink,
    PackageCheck,
    RefreshCw,
    Search,
    ShieldAlert,
    Star,
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
  import { fuzzyFilter } from "../../utils/fuzzy";
  import { modsPerPage, showModPreviews } from "../../stores/ui";
  import {
    getAwesomeRepository,
    getRepositoryStars,
  } from "$lib/repositoryMetadata";
  import AwesomeInstallDialog from "../AwesomeInstallDialog.svelte";
  import {
    installedModsCache,
    fetchCachedMods,
    forceRefreshCache,
  } from "../../stores/modCache";

  let {
    onViewCatalog = () => {},
    category = "All",
    categorySelection = 0,
    onCategoriesChange = () => {},
  }: {
    onViewCatalog?: (mod: Mod) => void;
    category?: string;
    categorySelection?: number;
    onCategoriesChange?: (categories: string[]) => void;
  } = $props();

  let catalog = $state<AwesomeCatalog>(bundledAwesomeCatalog);
  let origin = $state<"live" | "cache" | "snapshot">("snapshot");
  let refreshWarning = $state("");
  let refreshing = $state(false);
  let query = $state("");
  let loaderFilter = $state<AwesomeLoader | "All">("All");
  let sort = $state<"relevance" | "name" | "stars">("relevance");
  let page = $state(1);
  let failedPreviews = $state<string[]>([]);
  let view = $state<"mods" | "loaders">("mods");
  let installing = $state<AwesomeEntry | null>(null);
  let installedMessage = $state("");
  let displayRevision = $derived(
    /^[a-f0-9]{40}$/i.test(catalog.source.revision)
      ? catalog.source.revision.slice(0, 12)
      : catalog.source.revision,
  );

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
  let categories = $derived(
    [...new Set(modEntries.flatMap((entry) => entry.categories))].sort(),
  );
  let filteredEntries = $derived.by(() => {
    const source = (view === "mods" ? modEntries : loaderEntries).filter(
      (entry) =>
        (view !== "mods" ||
          loaderFilter === "All" ||
          entry.loader === loaderFilter) &&
        (view !== "mods" ||
          category === "All" ||
          entry.categories.includes(category)),
    );
    const found = fuzzyFilter(source, query, (entry) =>
      [
        entry.title,
        entry.description,
        entry.author,
        entry.loader,
        entry.categories.join(" "),
        entry.parentTitle || "",
      ].join(" "),
    );
    if (sort === "name")
      return [...found].sort((a, b) => a.title.localeCompare(b.title));
    if (sort === "stars")
      return [...found].sort(
        (a, b) =>
          (getRepositoryStars(getAwesomeRepository(b) || "") ?? -1) -
            (getRepositoryStars(getAwesomeRepository(a) || "") ?? -1) ||
          a.title.localeCompare(b.title),
      );
    return found;
  });
  let pageCount = $derived(
    $modsPerPage === "all"
      ? 1
      : Math.max(1, Math.ceil(filteredEntries.length / $modsPerPage)),
  );
  let currentPage = $derived(Math.min(page, pageCount));
  let pageStart = $derived(
    $modsPerPage === "all" ? 0 : (currentPage - 1) * $modsPerPage,
  );
  let visibleEntries = $derived(
    $modsPerPage === "all"
      ? filteredEntries
      : filteredEntries.slice(pageStart, pageStart + $modsPerPage),
  );
  let hasPreviews = $derived(
    filteredEntries.some((entry) => !!previewFor(entry)),
  );

  $effect(() => {
    onCategoriesChange(categories);
  });
  $effect(() => {
    void [category, categorySelection];
    view = "mods";
  });
  $effect(() => {
    // Changing the result set or page size starts at the first page.
    void [query, category, loaderFilter, view, sort, $modsPerPage];
    page = 1;
  });

  function previewFor(entry: AwesomeEntry): string | null {
    const match = matches.get(entry.id);
    return match?.image &&
      match._hasThumbnail !== false &&
      !failedPreviews.includes(entry.id)
      ? match.image
      : null;
  }

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
        <h2>{category === "All" ? "Awesome Balatro" : category}</h2>
        <p>Community mods and mod loaders</p>
      </div>
      <button
        class="refresh-button"
        type="button"
        disabled={refreshing}
        onclick={refreshFromGitHub}
      >
        <span class:spinning={refreshing}><RefreshCw size={15} /></span>
        {refreshing ? "Refreshing..." : "Refresh"}
      </button>
    </header>

    <label class="search">
      <Search size={17} />
      <span class="sr-only">Search Awesome Balatro</span>
      <input
        type="search"
        bind:value={query}
        placeholder="Search mods, authors, descriptions..."
      />
    </label>

    <div class="toolbar">
      <div class="tabs" role="tablist" aria-label="Directory type">
        <button
          type="button"
          role="tab"
          class:active={view === "mods"}
          aria-selected={view === "mods"}
          onclick={() => switchView("mods")}>Mods</button
        >
        <button
          type="button"
          role="tab"
          class:active={view === "loaders"}
          aria-selected={view === "loaders"}
          onclick={() => switchView("loaders")}>Loaders</button
        >
      </div>
      {#if view === "mods"}
        <label
          ><span class="sr-only">Filter by loader</span><select
            bind:value={loaderFilter}
          >
            <option value="All">All loaders</option><option>Steamodded</option
            ><option>Lovely</option><option>Balamod</option><option value="None"
              >No listed loader</option
            >
          </select></label
        >
      {/if}
      <label
        ><span class="sr-only">Sort mods</span><select bind:value={sort}>
          <option value="relevance"
            >{query.trim() ? "Best match" : "Directory order"}</option
          ><option value="name">Name</option><option value="stars"
            >GitHub stars</option
          >
        </select></label
      >
      {#if hasPreviews}
        <label class="preview-toggle"
          ><input
            type="checkbox"
            bind:checked={$showModPreviews}
          />Previews</label
        >
      {/if}
      <label class="page-size"
        >Per page<select aria-label="Mods per page" bind:value={$modsPerPage}>
          <option value={10}>10</option><option value={25}>25</option><option
            value={50}>50</option
          ><option value={100}>100</option><option value="all">All</option>
        </select></label
      >
    </div>

    {#if installedMessage}<div class="notice" role="status">
        {installedMessage}
      </div>{/if}
    {#if refreshWarning}<div class="notice" role="status">
        Refresh failed ({refreshWarning}). Showing the {origin === "cache"
          ? "cached copy"
          : "offline snapshot"}.
      </div>{/if}

    <div class="results-line">
      <span
        >{filteredEntries.length} result{filteredEntries.length === 1
          ? ""
          : "s"}{#if filteredEntries.length > 0}
          {" | "}{pageStart + 1}-{pageStart + visibleEntries.length}{/if}</span
      >
      <button
        type="button"
        class="source-link"
        onclick={() => open(AWESOME_BALATRO_README_URL)}
        >Upstream README <ExternalLink size={13} /></button
      >
    </div>

    {#if filteredEntries.length === 0}
      <div class="empty-state">No entries match these filters.</div>
    {:else}
      <div class="card-grid">
        {#each visibleEntries as entry (entry.id)}
          {@const match = matches.get(entry.id)}
          {@const installed = isInstalled(entry)}
          {@const source = getAwesomeInstallSource(entry, match)}
          {@const repository = getAwesomeRepository(entry)}
          {@const stars = repository ? getRepositoryStars(repository) : null}
          {@const preview = previewFor(entry)}
          <article class:safety={!!entry.safetyWarning}>
            {#if $showModPreviews && preview}
              <img
                class="preview"
                src={preview}
                alt="{entry.title} preview"
                loading="lazy"
                onerror={() => {
                  failedPreviews = [...failedPreviews, entry.id];
                }}
              />
            {/if}
            <div class="card-content">
              <div class="card-heading">
                <div class="card-title">
                  <h3>
                    {#if repository}<a
                        class="card-link"
                        href={repository}
                        onclick={(event) => {
                          event.preventDefault();
                          open(repository);
                        }}>{entry.title}</a
                      >{:else}{entry.title}{/if}
                  </h3>
                  {#if entry.author}<p class="author">by {entry.author}</p>{/if}
                </div>
                {#if repository}<span
                    class="stars"
                    title={stars === null
                      ? "GitHub star count unavailable"
                      : "GitHub stars"}
                    ><Star size={13} />{stars === null
                      ? "Unavailable"
                      : stars.toLocaleString()}</span
                  >{/if}
                {#if installed}<span class="status installed"
                    ><PackageCheck size={13} />Installed</span
                  >{/if}
              </div>
              {#if entry.description}<p class="description">
                  {entry.description}
                </p>{/if}
              <div class="metadata">
                {entry.kind === "loader"
                  ? "Mod loader"
                  : entry.loader}{#if entry.categories.length}
                  {" | "}{entry.categories.join(" / ")}{/if}
              </div>
              {#if entry.safetyWarning}<div class="safety-warning">
                  <ShieldAlert size={15} /><span>{entry.safetyWarning}</span>
                </div>{/if}
              <div class="actions">
                {#if source.kind !== "unsupported" && !installed}
                  <button
                    type="button"
                    class="install-button"
                    onclick={() => {
                      installedMessage = "";
                      installing = entry;
                    }}
                    >{source.kind === "download"
                      ? "Install"
                      : source.discordUrl
                        ? "Download from Discord"
                        : "Import download"}</button
                  >
                {/if}
                {#if match}<button
                    type="button"
                    onclick={() => viewMatch(entry)}>View in catalog</button
                  >{/if}
                <button
                  type="button"
                  class="source-link"
                  onclick={() => open(entry.sourceUrl)}
                  >{source.kind === "unsupported"
                    ? "Source instructions"
                    : "Open source"}<ExternalLink size={13} /></button
                >
                {#if entry.parentTitle || entry.badges.length || entry.links.length > 1 || source.kind === "unsupported"}
                  <details>
                    <summary>Details</summary>
                    {#if entry.parentTitle}<p>
                        Part of {entry.parentTitle}
                      </p>{/if}
                    {#if source.kind === "unsupported" && !entry.safetyWarning}<p
                      >
                        {source.reason}
                      </p>{/if}
                    {#each entry.badges as badge}<span
                        class="badge"
                        title={badge.url}>{badge.label}</span
                      >{/each}
                    {#each entry.links.slice(1) as link}<button
                        type="button"
                        class="source-link"
                        onclick={() => open(link.url)}
                        >{link.label}<ExternalLink size={12} /></button
                      >{/each}
                  </details>
                {/if}
              </div>
            </div>
          </article>
        {/each}
      </div>
      {#if pageCount > 1}
        <nav class="pagination" aria-label="Mod pages">
          <button
            type="button"
            disabled={currentPage === 1}
            onclick={() => {
              page = currentPage - 1;
            }}>Previous</button
          >
          <span>Page {currentPage} of {pageCount}</span>
          <button
            type="button"
            disabled={currentPage === pageCount}
            onclick={() => {
              page = currentPage + 1;
            }}>Next</button
          >
        </nav>
      {/if}
    {/if}
    <details class="directory-details">
      <summary>About this directory</summary>
      <p>
        Install archives and individual Lua mods directly, or import downloaded
        files from Discord. Packages are checked and conflicts are shown before
        installation. A catalog match is not a compatibility guarantee.
      </p>
      <p>
        Source: {catalog.source.repository} | revision {displayRevision} | {origin ===
        "live"
          ? "live refresh"
          : origin === "cache"
            ? "cached copy"
            : "bundled snapshot"}. Stars are from the most recent repository
        check; unavailable counts are shown as Unavailable.
      </p>
    </details>
  </div>
</div>

<style>
  .awesome-view {
    padding: 1.25rem 1.5rem 2rem;
    color: var(--ui-text);
    font-size: 0.875rem;
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
    margin-bottom: 1rem;
  }
  h2 {
    margin: 0;
    font-size: 1.35rem;
    font-weight: 600;
    color: var(--ui-heading);
  }
  header p {
    margin: 0.3rem 0 0;
    color: var(--ui-text-muted);
  }
  button,
  input,
  select {
    font: inherit;
  }
  button,
  select,
  summary {
    cursor: pointer;
  }
  button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 0.35rem;
    padding: 0.4rem 0.65rem;
    border: 1px solid var(--ui-input-border);
    border-radius: 5px;
    background: var(--ui-panel-muted);
    color: var(--ui-text);
  }
  button:hover:not(:disabled) {
    background: var(--ui-glass-hover);
    border-color: var(--ui-text-muted);
  }
  button:disabled {
    opacity: 0.4;
    cursor: default;
  }
  :is(button, a, select, input, summary):focus-visible {
    outline: 2px solid var(--ui-heading);
    outline-offset: 3px;
  }
  .refresh-button {
    white-space: nowrap;
  }
  .search {
    display: flex;
    align-items: center;
    gap: 0.55rem;
    border: 1px solid var(--ui-input-border);
    border-radius: 5px;
    padding: 0 0.7rem;
    background: var(--ui-input-bg);
  }
  .search:focus-within {
    border-color: var(--ui-text-muted);
  }
  .search input {
    background: transparent;
    border: 0;
    outline: 0;
    color: var(--ui-text);
    min-width: 0;
    width: 100%;
    padding: 0.65rem 0;
  }
  .toolbar {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 0.5rem;
    margin: 0.75rem 0;
  }
  .tabs {
    display: flex;
    gap: 0.25rem;
  }
  .tabs button.active {
    background: var(--ui-glass-hover);
    border-color: var(--ui-text-muted);
  }
  select {
    background: var(--ui-input-bg);
    border: 1px solid var(--ui-input-border);
    border-radius: 5px;
    padding: 0.4rem 0.6rem;
    color: var(--ui-text);
  }
  .preview-toggle,
  .page-size {
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
    color: var(--ui-text-soft);
    font-size: 0.8rem;
  }
  .preview-toggle input {
    accent-color: var(--ui-success);
  }
  .page-size {
    margin-left: auto;
  }
  .notice,
  .safety-warning {
    background: var(--ui-note-bg);
    border: 1px solid var(--ui-note-border);
    border-radius: 5px;
    padding: 0.6rem;
    margin: 0.6rem 0;
    line-height: 1.4;
  }
  .results-line {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin: 0.5rem 0;
    color: var(--ui-text-muted);
    font-size: 0.8rem;
  }
  button.source-link {
    background: transparent;
    border: 0;
    padding: 0.3rem 0.2rem;
    color: var(--ui-text-soft);
  }
  .card-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(240px, 1fr));
    gap: 0.65rem;
  }
  article {
    position: relative;
    display: flex;
    flex-direction: column;
    min-width: 0;
    background: var(--ui-panel-muted);
    border: 1px solid var(--ui-panel-muted-border);
    border-radius: 6px;
  }
  article:hover {
    border-color: var(--ui-input-border);
  }
  article.safety {
    border-color: var(--ui-warning-border);
  }
  .preview {
    width: 100%;
    height: 130px;
    object-fit: contain;
    background: var(--ui-input-bg);
    border-radius: 5px 5px 0 0;
  }
  .card-content {
    display: flex;
    flex-direction: column;
    flex: 1;
    padding: 0.8rem;
  }
  .card-heading {
    display: flex;
    align-items: flex-start;
    gap: 0.6rem;
  }
  .card-title {
    flex: 1;
    min-width: 0;
  }
  h3 {
    margin: 0;
    font-size: 0.95rem;
    font-weight: 600;
    line-height: 1.3;
    overflow-wrap: anywhere;
  }
  .card-link {
    color: var(--ui-heading);
    text-decoration: none;
  }
  .card-link::after {
    content: "";
    position: absolute;
    inset: 0;
    border-radius: 6px;
  }
  .card-link:hover {
    text-decoration: underline;
  }
  .author,
  .metadata {
    color: var(--ui-text-muted);
    font-size: 0.75rem;
    line-height: 1.4;
  }
  .author {
    margin: 0.2rem 0 0;
  }
  .stars {
    display: inline-flex;
    align-items: center;
    gap: 0.25rem;
    color: var(--ui-text-muted);
    font-size: 0.75rem;
    white-space: nowrap;
  }
  .description {
    margin: 0.6rem 0;
    line-height: 1.45;
    color: var(--ui-text-soft);
    font-size: 0.82rem;
  }
  .metadata {
    margin-bottom: 0.7rem;
  }
  .status {
    display: inline-flex;
    align-items: center;
    gap: 0.25rem;
    color: var(--ui-success);
    font-size: 0.7rem;
  }
  .safety-warning {
    display: flex;
    align-items: flex-start;
    gap: 0.35rem;
    border-color: var(--ui-warning-border);
    background: var(--ui-danger-overlay);
    font-size: 0.8rem;
  }
  .safety-warning :global(svg) {
    flex: 0 0 auto;
  }
  .actions {
    position: relative;
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 0.4rem;
    margin-top: auto;
    font-size: 0.75rem;
  }
  .install-button {
    background: var(--ui-success);
    color: white;
    border-color: transparent;
  }
  button.install-button:hover {
    background: var(--ui-success-hover);
  }
  details p {
    margin: 0.5rem 0;
    line-height: 1.45;
    color: var(--ui-text-muted);
  }
  .badge {
    display: inline-block;
    padding: 0.25rem;
    font-size: 0.75rem;
    color: var(--ui-text-muted);
  }
  .directory-details {
    margin-top: 1rem;
    color: var(--ui-text-muted);
    font-size: 0.75rem;
  }
  .pagination {
    display: flex;
    justify-content: center;
    align-items: center;
    gap: 1rem;
    margin-top: 1rem;
    font-size: 0.8rem;
    color: var(--ui-text-muted);
  }
  .empty-state {
    padding: 2rem;
    text-align: center;
    color: var(--ui-text-muted);
  }
  .spinning {
    display: flex;
    animation: spin 0.9s linear infinite;
  }
  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    padding: 0;
    margin: -1px;
    overflow: hidden;
    clip: rect(0, 0, 0, 0);
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
  @media (max-width: 650px) {
    .awesome-view {
      padding: 1rem;
    }
    .card-grid {
      grid-template-columns: 1fr;
    }
    .page-size {
      margin-left: 0;
    }
  }
</style>
