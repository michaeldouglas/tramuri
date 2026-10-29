# Tramuri

Tramuri is local-first infrastructure for discovering project facts and carrying verifiable context between people, tools, and work sessions. Its core behavior is implemented in Rust and exposed through a native CLI and a Python SDK.

The project is in early development. The first increment provides project initialization and deterministic file inventory; knowledge records and versioned Context Pack/Evidence Pack exports will follow in later increments.

## Development

Requirements: Rust stable, Python 3.11 or newer, and uv.

```powershell
cargo run -p tramuri-cli -- init .
cargo run -p tramuri-cli -- scan .
cargo test --workspace
```

Build the Python extension locally and run the SDK tests with:

```powershell
uv sync --locked
uv run maturin develop
uv run pytest
```

Repository contents are treated as untrusted data during discovery. Scanning does not execute files or hooks. The Python extension is built with PyO3 and maturin; end users will receive prebuilt wheels for the supported platforms.

Secret-name exclusions are a conservative baseline, not exhaustive secret detection. The initial inventory stores file metadata and bounded hashes, not file contents.

## License

Apache-2.0. See [LICENSE](LICENSE).
