# cargo-shepherd audit — v1.2.1

Date: 2026-07-12  
Source: uploaded `rust_cargo_shepherd-main.zip` (GitHub `aeiouofficial/rust_cargo_shepherd`, commit-ish fa5ce19)

## What this app is

A system-wide **Cargo build coordinator**: daemon + priority queue + CLI + TUI +
Windows tray + optional `cargo.exe` shim + passive herding of unmanaged Rust
build processes. Goal: stop parallel VS Code / rust-analyzer windows from
thrashing the global Cargo lock and maxing CPU/RAM.

## Verdict

**Solid product idea, real Windows depth, working architecture.**  
Not a toy. The daemon/queue/IPC/shim design is correct for the problem.

What was wrong was mostly:
1. missing Rust quality tooling (the ESLint/Prettier stack),
2. a few correctness gaps between docs and scheduler behavior,
3. aggressive defaults documented poorly,
4. AI-flavored file headers / changelog drift / packaging noise.

## Rust “ESLint + Prettier” stack (added)

| JS world | Rust equivalent | How to run |
|----------|-----------------|------------|
| Prettier | **rustfmt** | `cargo fmt` / `make fmt` |
| ESLint | **clippy** | `cargo clippy --all-targets -- -D warnings` / `make lint` |
| typecheck | **cargo check / test** | `make test` / `make check` |
| CI gate | GitHub Actions | `.github/workflows/ci.yml` |

Files added:
- `rustfmt.toml`
- `clippy.toml`
- `[lints]` in `Cargo.toml`
- `Makefile` (`fmt`, `lint`, `test`, `check`, `build`)
- `.github/workflows/ci.yml`
- `.gitignore`

## Findings fixed in 1.2.1

### P0 / real bugs

1. **Background priority was a lie**  
   Docs/comments said “only runs when slots would otherwise be empty”.  
   Scheduler treated it as plain lowest priority and still co-ran with active jobs.  
   **Fix:** peek queue; Background jobs only start when managed + herded-active count is 0.

2. **Attached cargo shim disconnect did not kill running builds**  
   Ctrl+C / hangup on the shim cancelled only queued jobs. Running cargo kept burning CPU.  
   **Fix:** `cancel_queued_attached_job` also signals kill for running jobs.

3. **CLI request/response could hang forever**  
   `send_recv` had no timeout after connect.  
   **Fix:** 10s timeout on request/response only. Streaming `recv()` stays unbounded (builds are long).

### P1 / design & config

4. **Unix socket always in `/tmp`**  
   Multi-user collision risk.  
   **Fix:** prefer `$XDG_RUNTIME_DIR/cargo-shepherd.sock`, fall back to `/tmp`.

5. **sccache auto-enable was hard-coded**  
   Surprising if you already manage `RUSTC_WRAPPER`.  
   **Fix:** `use_sccache` config flag (default true, preserves prior behavior).

6. **Priority parsing duplicated**  
   **Fix:** `FromStr` on `Priority`; CLI uses it.

### P2 / hygiene

7. No rustfmt/clippy/CI/gitignore — **added**.  
8. Version headers that read like release notes inside source — **trimmed to module docs**.  
9. Package metadata incomplete — **repository, categories, rust-version**.  
10. README launcher paths said `bin/build_shepherd.bat` while files live at repo root — **fixed in README**.

## Findings left intentional / follow-ups

| Item | Why left |
|------|----------|
| Branding **shepherd** vs **Sheppard** | Product name; keep consistent in UI strings, not a functional bug |
| Default startup takeover of existing rustc/cargo | Explicit product decision in v1.2.0; dangerous but intentional — document, opt out with `SHEPHERD_TAKEOVER_EXISTING_CARGO=0` |
| `daemon.rs` / `tui.rs` size | Large but cohesive; full split can wait until next feature |
| Unix suspend/resume for herding | Windows-only today; Linux needs SIGSTOP/SIGCONT path |
| No integration tests with real cargo | Unit tests cover queue/daemon helpers; needs a harness with fake cargo |
| CHANGELOG references missing `build_island` / release folders | Historical packaging; not in this zip |

## Architecture (still good)

```
CLI / TUI / cargo-shim
        │  NDJSON
        ▼
   Daemon IPC server
        │
   PriorityQueue ──► runner pool ──► real cargo
        ▲                 │
        └── Notify ◄──────┘ finish/kill
        │
   ResourceMonitor (CPU/RAM gates + external herd)
```

Keep this shape. Do not “rewrite into 20 microcrates” — that *is* AI slop.

## How to verify on your machine

```bash
cargo fmt -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo build --release
# or
make check
```

Windows: use `build_shepherd.ps1` / `start_shepherd.ps1` as before.

## Quality bar for future edits

- Prefer small modules with one job over god-files *only when a seam already exists*.
- Comment **why**, not **what**.
- No fake abstractions, no placeholder TODOs that ship.
- Always run `make check` before tagging a release.
