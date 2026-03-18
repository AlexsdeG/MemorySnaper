# MemorySnaper

> **Save and relive your Snapchat memories — 100% on your own machine.**

MemorySnaper is a privacy-first desktop application built with Tauri, React, and Rust. It imports your official Snapchat data export, downloads every media file at full quality, burns overlay text back onto each photo and video, and presents everything in a fast, virtualized media grid. No accounts, no cloud, no third-party servers.

---

## Features

| Feature | Description |
|---|---|
| **100% Local Processing** | Every operation runs on your machine. No data is ever transmitted to external services. |
| **Multi-Part ZIP & JSON Import** | Accepts the full Snapchat export archive (`.zip`) or a standalone `memories_history.json`. Duplicate detection on re-import. |
| **Concurrent Media Download** | Tokio-powered async downloader with configurable rate limiting (requests/min and concurrency) to avoid Snapchat throttling. |
| **Overlay Burn-in** | Reconstructs the original Snapchat overlay — text, captions, and stickers — directly onto each photo and video frame using the embedded metadata. |
| **Virtualized Media Grid** | Browse thousands of memories without lag via a windowed, virtualized thumbnail grid. |
| **System Dark / Light Mode** | Follows your OS theme automatically. Override to Light, System, or Dark at any time from the Settings tab. |
| **Privacy-First Architecture** | All media and metadata are stored in a local SQLite database. No telemetry. No analytics. GDPR-friendly by design. |

---

## Getting Started

### Prerequisites

- [Node.js](https://nodejs.org/) 18 or later
- [Rust](https://rustup.rs/) (stable toolchain, 1.77+)
- [pnpm](https://pnpm.io/) (recommended) or npm

### Install & Run

```bash
# 1. Clone the repository
git clone https://github.com/AlexsdeG/MemorySnaper.git
cd MemorySnaper

# 2. Install JavaScript dependencies
pnpm install        # or: npm install

# 3. Start the development build (Vite frontend + Tauri shell)
pnpm tauri dev      # or: npm run tauri dev
```

### Production Build

```bash
pnpm tauri build    # or: npm run tauri build
```

The signed installer is output to `src-tauri/target/release/bundle/`.

---

## Architecture

MemorySnaper follows a strict local-first, layered architecture:

```
┌─────────────────────────────────────────────┐
│   React UI  (Vite + TypeScript + Tailwind)  │
│   src/features/   ·   src/components/       │
└───────────────────┬─────────────────────────┘
                    │  Tauri IPC (#[tauri::command])
┌───────────────────▼─────────────────────────┐
│   Rust Backend  (src-tauri/src/)            │
│                                             │
│  core/             Business logic           │
│    ├─ parser.rs    JSON → typed structs      │
│    ├─ downloader.rs  Async HTTP via reqwest  │
│    ├─ processor.rs   Overlay burn-in         │
│    └─ media.rs     File I/O utilities        │
│  db/               SQLite layer             │
│    ├─ schema.rs    Table definitions (sqlx)  │
│    └─ mod.rs       Query helpers            │
└───────────────────┬─────────────────────────┘
                    │
┌───────────────────▼─────────────────────────┐
│   SQLite Database  (local file, sqlx 0.8)   │
│   Stores: memory items, download state,     │
│           thumbnail paths, job progress     │
└─────────────────────────────────────────────┘
```

**Data flow:**
1. The user drops a Snapchat `.zip` or `.json` export into the UI.
2. The React frontend calls the `import_memories_json` Tauri command.
3. The Rust `parser` deserializes the JSON and writes records to SQLite via `sqlx`.
4. The user triggers **Start Download**; the `downloader` fetches each CDN URL concurrently using `tokio` worker threads and saves raw files to `.raw_cache/`.
5. The user triggers **Process Files**; the `processor` burns overlays onto each file and writes output alongside a thumbnail.
6. The React Viewer reads thumbnail paths from SQLite and renders them in a virtualized grid.

All state survives app restarts — progress is never lost.

---

## Contributing

1. Fork the repository and create a feature branch.
2. Run `cargo fmt && cargo clippy --all-targets --all-features -D warnings` before committing Rust changes.
3. Run `pnpm typecheck` (or `tsc --noEmit`) before committing TypeScript changes.
4. Open a pull request with a clear description of the change.

---

## License

MIT
