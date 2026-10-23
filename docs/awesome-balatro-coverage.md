# Awesome Balatro catalog coverage

The in-app Awesome Balatro directory is generated from
[`jie65535/awesome-balatro`](https://github.com/jie65535/awesome-balatro/blob/main/README.md).
The bundled offline snapshot records README blob
`1a0335a5834f3148cd24076983d65e6e2bd195c0`, retrieved on October 23, 2026.
The directory can refresh from the raw upstream README at runtime; a failed or
unexpectedly small refresh keeps the last known-good cache or bundled snapshot.
The upstream list is published under
[CC0 1.0 Universal](https://github.com/jie65535/awesome-balatro/blob/main/LICENSE);
the snapshot retains the upstream repository, README URL, revision, and source
line for every entry.

## Snapshot coverage

| Group                                               | Entries |
| --------------------------------------------------- | ------: |
| Mods listed under the four upstream Mods sections   |     320 |
| Mod-like Tools entry (Balatro Ultimate Speedup Mod) |       1 |
| Mod loader references, shown on a separate tab      |       4 |
| Total directory entries                             |     325 |

The 321 mod entries cover 322 source bullets. One identical Steamodded entry,
Sylvie's Silliness, appears under both Collections and Decks; it is represented
once with both categories. Nine named children of grouped entries such as
Gimmicky decks pack are retained as individual entries with their parent and
source link. The parser also accepts the upstream malformed
`[Redux-Arcanum] (url)` link.

Mod entries by listed loader:

| Loader in Awesome Balatro | Entries |
| ------------------------- | ------: |
| Steamodded                |     259 |
| Balamod                   |      30 |
| Lovely                    |      25 |
| No loader listed          |       7 |

The “no loader” count includes the six Mods (Other) entries and Balatro
Ultimate Speedup Mod. Calculators, seed searchers, managers, trackers, companion
apps, website templates, and resource links from the Tools and Resources
sections are not presented as game mods. Balamod, Lovely, NANEINF, and
Steamodded remain discoverable on the separate Loaders tab.

## Installation boundary

Awesome Balatro is a discovery source, not an installation manifest. BMM shows
“View in catalog” only when exactly one current primary-index item has both:

1. the same normalized title;
2. the same canonical GitHub owner/repository;
3. a repository-root source link rather than a file, tree, query, or anchor; and
4. a current, non-orphaned primary record carrying a verified `bmi://`
   download reference.

Every other entry is source/manual-only. This includes Discord threads,
Thunderstore pages, standalone Lua files, repository collection anchors,
non-GitHub sites, Balamod and loader-free entries, ambiguous multi-mod
repositories, and any GitHub repository that is absent from the current BMM
index. BMM does not guess a ZIP URL or infer that a repository's default branch
has an installable layout.

JokerDisplay and any source owned by `nh6574`, Joyous Spring, and Balatro
Multiplayer are hard-blocked from the BMM catalog action even if stale local
data contains a matching record. Their source can remain visible with a warning;
installation must follow the owner's current instructions. Multiplayer is
called out separately because its supported layout is not compatible with
BMM's legacy installation flow.

Runtime “in BMM catalog” and “installed” totals intentionally are not fixed in
this report. They are computed against the currently verified primary catalog
and local installation state, so the UI cannot turn an old bundled match into
an installation path.
