# Graph Report - tramuri  (2026-10-28)

## Corpus Check
- 16 files · ~4,768 words
- Verdict: corpus is large enough that graph structure adds value.
- Unclassified: 4 file(s) not represented in the graph (top: (none) 2, .typed 1, .lock 1)

## Summary
- 156 nodes · 280 edges · 15 communities (14 shown, 1 thin omitted)
- Extraction: 96% EXTRACTED · 4% INFERRED · 0% AMBIGUOUS · INFERRED: 11 edges (avg confidence: 0.87)
- Token cost: 0 input · 0 output

## Graph Freshness
- Built from commit: `1590f562`
- Run `git rev-parse HEAD` and compare to check if the graph is stale.
- Run `graphify update .` after code changes (no API cost).

## Community Hubs (Navigation)
- scan
- discovery.rs
- python/src/lib.rs
- storage.rs
- tramuri-core/src/lib.rs
- scan
- TramuriError
- main.rs
- commands.rs
- init
- ADR-0002 — Project Discovery vertical slice
- FileComparison
- Tramuri
- tramuri-core
- tramuri

## God Nodes (most connected - your core abstractions)
1. `scan()` - 20 edges
2. `TramuriError` - 12 edges
3. `initialized_project()` - 11 edges
4. `scan()` - 10 edges
5. `init()` - 9 edges
6. `FileRecord` - 8 edges
7. `init()` - 8 edges
8. `HashStatus` - 7 edges
9. `collect_files()` - 7 edges
10. `StoredFile` - 7 edges

## Surprising Connections (you probably didn't know these)
- `Decision` --references--> `init()`  [INFERRED]
  docs/architecture/decisions/0002-project-discovery.md → python/tramuri/__init__.py
- `Decision` --references--> `scan()`  [INFERRED]
  docs/architecture/decisions/0002-project-discovery.md → python/tramuri/__init__.py
- `_scan_json()` --calls--> `scan()`  [INFERRED]
  bindings/python/src/lib.rs → crates/tramuri-core/src/discovery.rs
- `tramuri-python` --depends_on--> `tramuri-core`  [EXTRACTED]
  bindings/python/Cargo.toml → crates/tramuri-core/Cargo.toml
- `run()` --calls--> `scan()`  [INFERRED]
  crates/tramuri-cli/src/main.rs → crates/tramuri-core/src/discovery.rs

## Import Cycles
- 2-file cycle: `crates/tramuri-core/src/discovery.rs -> crates/tramuri-core/src/lib.rs -> crates/tramuri-core/src/discovery.rs`
- 3-file cycle: `crates/tramuri-core/src/discovery.rs -> crates/tramuri-core/src/storage.rs -> crates/tramuri-core/src/lib.rs -> crates/tramuri-core/src/discovery.rs`

## Communities (15 total, 1 thin omitted)

### Community 0 - "scan"
Cohesion: 0.18
Nodes (6): Decision, init(), scan(), test_cli_and_sdk_scan_json_have_normalized_parity(), test_init_is_idempotent_and_scan_returns_inventory(), test_scan_requires_init()

### Community 1 - "discovery.rs"
Cohesion: 0.10
Nodes (3): filter_entry(), hash_file(), MAX_HASH_BYTES

### Community 2 - "python/src/lib.rs"
Cohesion: 0.15
Nodes (5): _init_json(), map_core_error(), map_serialization_error(), _native(), _scan_json()

### Community 3 - "storage.rs"
Cohesion: 0.22
Nodes (12): read_inventory(), to_stored(), SCHEMA_VERSION, APP_DIR, DATABASE_FILE, database_path(), initialize_database(), load_inventory() (+4 more)

### Community 4 - "tramuri-core/src/lib.rs"
Cohesion: 0.21
Nodes (13): collect_files(), FileRecord, HashStatus, Hashed, ReadError, TooLarge, inventory_file(), relative_path() (+5 more)

### Community 5 - "scan"
Cohesion: 0.27
Nodes (12): compare_file(), empty_project_scans_as_complete(), incomplete_scan_does_not_infer_deletions_or_prune_rows(), initialized_project(), large_file_is_inventoried_without_hash(), scan(), scan_honors_gitignore_and_secret_denylists(), scan_persists_added_changed_unchanged_and_deleted_files() (+4 more)

### Community 6 - "TramuriError"
Cohesion: 0.18
Nodes (9): Result, TramuriError, InvalidProjectRoot, InvalidStatePath, Io, NonUtf8Path, NotInitialized, Sqlite (+1 more)

### Community 7 - "main.rs"
Cohesion: 0.27
Nodes (6): Cli, Command, Init, Scan, main(), run()

### Community 8 - "commands.rs"
Cohesion: 0.33
Nodes (4): help_is_available(), init_and_scan_emit_machine_readable_json(), operational_error_is_nonzero_and_written_to_stderr(), tramuri()

### Community 9 - "init"
Cohesion: 0.40
Nodes (5): canonical_project_root(), init(), init_creates_database_and_is_idempotent(), InitReport, path_to_string()

### Community 10 - "ADR-0002 — Project Discovery vertical slice"
Cohesion: 0.40
Nodes (4): ADR-0002 — Project Discovery vertical slice, Consequences, Context, Options considered

### Community 11 - "FileComparison"
Cohesion: 0.50
Nodes (4): FileComparison, Changed, Unchanged, Unverified

### Community 12 - "Tramuri"
Cohesion: 0.50
Nodes (3): Development, License, Tramuri

### Community 13 - "tramuri-core"
Cohesion: 0.67
Nodes (3): tramuri-cli, tramuri-core, tramuri-python

## Knowledge Gaps
- **18 isolated node(s):** `tramuri-python`, `tramuri-cli`, `Init`, `Scan`, `Hashed` (+13 more)
  These have ≤1 connection - possible missing edges or undocumented components. (Counts symbols only; 64 node(s) total have ≤1 connection when file, concept and rationale nodes are included.)
- **1 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `scan()` connect `scan` to `discovery.rs`, `python/src/lib.rs`, `storage.rs`, `tramuri-core/src/lib.rs`, `main.rs`, `init`?**
  _High betweenness centrality (0.167) - this node is a cross-community bridge._
- **Are the 5 inferred relationships involving `scan()` (e.g. with `_scan_json()` and `run()`) actually correct?**
  _`scan()` has 5 INFERRED edges - model-reasoned connections that need verification._
- **What connects `tramuri-python`, `tramuri-cli`, `Init` to the rest of the system?**
  _18 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `discovery.rs` be split into smaller, more focused modules?**
  _Cohesion score 0.1038961038961039 - nodes in this community are weakly interconnected._
- **Why does `_scan_json()` connect `python/src/lib.rs` to `scan`?**
  _High betweenness centrality (0.125) - this node is a cross-community bridge._