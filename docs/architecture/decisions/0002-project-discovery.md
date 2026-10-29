# ADR-0002 — Project Discovery vertical slice

- **Status:** Accepted
- **Date:** 2026-10-28

## Context

The first product capability must provide `tramuri init` and `tramuri scan` through both a native Rust CLI and the Python SDK. The repository currently contains no product source code. The behavior must remain local, deterministic, safe for untrusted repositories, and independent of an LLM or harness.

## Options considered

- **Separate CLI and Python implementations:** faster to prototype each surface, but duplicates discovery rules and risks behavioral drift.
- **Shared Rust core:** one implementation serves the native CLI and PyO3 SDK; it requires a Rust build for development and CI, while end users receive prebuilt wheels.
- **Follow symlinks during discovery:** finds linked content but expands the scan boundary and risks traversing outside the requested project.
- **Do not follow symlinks:** bounded and predictable; report skipped links so the inventory is visibly incomplete.

## Decision

- Create one Rust workspace at the Git root with `tramuri-core`, `tramuri-cli`, and a PyO3 binding crate built by maturin.
- Keep all discovery rules and persistence in `tramuri-core`. The CLI and SDK are thin interfaces to that API.
- `init` creates `.tramuri/tramuri.sqlite3` and is idempotent. The initial SQLite schema is version 1, recorded with `PRAGMA user_version`.
- `scan` inventories regular files in deterministic relative-path order, honors repository `.gitignore` rules, and independently excludes sensitive names. It does not parse or execute repository files.
- Do not follow symlinks. Include a warning and mark the scan incomplete when a symlink is encountered.
- Record relative path, size, modification time where available, and SHA-256 for files up to 32 MiB. Larger files remain in the inventory without a hash and carry an explicit warning.
- A file whose fingerprint cannot be verified is listed as `unverified`; metadata equality alone does not prove content identity. Detectable path or file changes during hashing produce a partial scan.
- A complete scan may report deleted paths and remove their prior inventory rows. An incomplete scan must not infer deletions or erase unseen rows.
- The CLI provides human-readable output by default and JSON with `--json`; a partial scan is a successful result with warnings, while operational errors go to stderr and return nonzero.
- The Python SDK exposes `tramuri.init(path)` and `tramuri.scan(path)` from the same Rust core and returns normalized data matching CLI JSON.
- This slice does not define or stabilize Context Pack or Evidence Pack schemas; those require their own versioned contracts before implementation.

## Consequences

- Python users on supported platforms install prebuilt wheels and do not need Rust. Developers and CI need Rust and maturin.
- Hashing is bounded; files larger than the limit have no content fingerprint until a later design decision changes the limit or hashing strategy.
- Large files without content hashes remain unverified across scans, even if size and modification time are unchanged.
- Symlinked content is deliberately omitted. The report makes this visible rather than silently treating the inventory as complete.
- SQLite migrations must be added before changing the initial schema. Contract versions for Context Packs and Evidence Packs remain a later, separately reviewed decision.
- Windows, Linux, and macOS wheel coverage will be implemented and verified in the distribution phase, not claimed from this local vertical slice.
