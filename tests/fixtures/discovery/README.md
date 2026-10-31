# Shared Project Discovery Fixture

This directory is copied into temporary project roots by CLI and SDK tests. It contains only
synthetic data; `sentinel-not-a-real-credential` is a harmless marker used to prove sensitive
file names are excluded.

## Cases

| Path | Case | Expected behavior |
|---|---|---|
| `.gitignore` | Ignore rules | The `ignored/` subtree is omitted. |
| `regular/hello.txt` | Ordinary file | Included and content-hashed. |
| `sensitive/.env.test` | Sensitive name | Excluded by Tramuri even though it is not ignored. |
| `unicode/naïve.txt` | Unicode path | Included with its original UTF-8 path. |
| `ignored/not-indexed.txt` | Ignored file | Omitted according to `.gitignore`. |

Symlink behavior is created dynamically by tests because symlinks are not represented portably in
Git archives. Tests should point the link outside the project root, assert that Tramuri does not
follow it, and mark the scan partial. If the host cannot create symlinks, only that scenario may be
skipped with the platform error recorded.
