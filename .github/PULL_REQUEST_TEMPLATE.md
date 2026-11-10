## What this changes

<!-- One or two sentences. What is different after this PR? -->

## Why

<!-- The problem this solves. Link the issue: Closes #123 -->

Closes #

## Type of change

- [ ] Renderer (TypeScript / React)
- [ ] Rust backend (`src-tauri`)
- [ ] Build, packaging or release pipeline
- [ ] Documentation only

## How it was verified

```bash
# Renderer
pnpm typecheck
pnpm format:check
pnpm test:unit

# Rust
cd src-tauri
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

- [ ] Renderer typecheck passes
- [ ] Prettier check passes
- [ ] Vitest tests pass
- [ ] `cargo fmt --check` passes
- [ ] `cargo clippy` passes with no warnings
- [ ] `cargo test` passes
- [ ] No secrets, keys or tokens in the diff
- [ ] Cross-platform impact considered (Windows / macOS / Linux)
- [ ] Docs updated if user-visible behaviour changed

## Notes for the reviewer

<!-- Trade-offs, follow-ups, areas you want scrutinised. -->
