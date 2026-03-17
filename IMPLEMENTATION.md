This is an excellent decision. Transitioning to a local-first architecture with Tauri is the most robust, privacy-compliant, and cost-effective way to handle massive personal data exports. It entirely bypasses the server-cost and GDPR nightmares.

Here is your highly detailed, executable blueprint. You can save this directly as `IMPLEMENTATION.md` in your project root. 

***

```markdown
# IMPLEMENTATION.md

## 1. Project Context & Architecture
- **Goal:** Build a privacy-first, local Desktop & Mobile application (Tauri v2) to parse, download, and process Snapchat Memories. The app will fetch media from Snapchat's CDN, merge visual overlays, inject EXIF metadata, and save files locally without relying on external servers.
- **Tech Stack & Dependencies:**
  - **Framework:** Tauri v2 (`npm create tauri-app@latest`)
  - **Frontend:** React 18+, Tailwind CSS, Shadcn UI (`npm install tailwindcss lucide-react framer-motion`)
  - **Backend:** Rust 1.80+
  - **Rust Crates:** `serde_json` (parsing), `tokio` (async runtime), `reqwest` (HTTP client), `kamadak-exif` (metadata), `rusqlite` or `tauri-plugin-sql` (local state), `ffmpeg-next` (media processing).
- **File Structure:**
  ```text
  ├── .github/
  │   ├── agents/
  │   ├── docs/
  │   └── prompts/
  ├── src/                # React Frontend
  │   ├── components/     # Shadcn UI components
  │   └── lib/            # Frontend utilities
  ├── src-tauri/          # Rust Backend
  │   ├── src/
  │   │   ├── commands/   # Tauri IPC commands
  │   │   ├── core/       # Download & EXIF logic
  │   │   └── db/         # SQLite operations
  │   ├── Cargo.toml
  │   └── tauri.conf.json
  ```
- **Attention Points:** - Snapchat CDN links expire within ~24 hours; state management must handle resuming and requesting new JSON files.
  - Media processing (FFmpeg overlay merging) is CPU-intensive and must run on background threads to prevent UI freezing.
  - Mobile targets (iOS/Android) require strict file-system permission handling and background-task configuration.

## 2. Execution Phases

### Phase 1: Universal AI Workspace Initializer
- [x] **Step 1.1:** Create `.github/copilot-instructions.md` defining the Tauri v2 (Rust/React) stack, strict TypeScript rules, and Rust clippy standards.
- [x] **Step 1.2:** Create `.github/docs/architecture-blueprint.md` mapping the IPC bridge between `src/` (React) and `src-tauri/src/commands/` (Rust).
- [x] **Step 1.3:** Create `.github/agents/architect.agent.md` (Tools: `[read, search]`, Persona: System planner) and `.github/agents/engineer.agent.md` (Tools: `[read, edit, execute]`, Persona: Senior Developer).
- [x] **Step 1.4:** Create `.github/prompts/` containing `plan.prompt.md`, `execute.prompt.md`, `debug.prompt.md`, and `explain.prompt.md` with the required semantic versioning directives.
- [ ] **Verification:** Run `ls -R .github` to verify all agent, docs, and prompt files exist and are correctly populated.

### Phase 2: Scaffolding & Core Architecture
- [x] **Step 2.1:** Initialize the Tauri v2 application using `npm create tauri-app@latest` (Select React, TypeScript, Tailwind).
- [x] **Step 2.2:** Install frontend dependencies for UI (`shadcn/ui` components like Progress, Card, Button).
- [x] **Step 2.3:** In `src-tauri/Cargo.toml`, add `serde`, `serde_json`, `tokio`, `reqwest`, and `tauri-plugin-sql`.
- [x] **Step 2.4:** Configure `tauri.conf.json` to allow file system access to the user's `Downloads` and `Picture` directories.
- [x] **Verification:** Run `npm run tauri dev`. Verify the baseline desktop window opens without errors.

### Phase 3: State Management & Database
- [x] **Step 3.1:** Initialize `tauri-plugin-sql` in `src-tauri/src/main.rs` to create a local `memories.db` SQLite file.
- [x] **Step 3.2:** Create a Rust module `src-tauri/src/db/` and write queries to initialize tables: `MemoryItem` (id, date, location, media_url, overlay_url, status) and `ExportJob` (status, total_files, downloaded_files).
- [x] **Step 3.3:** Expose Tauri commands (`#[tauri::command]`) to read/write job state from the React frontend.
- [ ] **Verification:** Call the `get_job_state` command from the React frontend console and verify it returns valid JSON.

### Phase 4: Parsing & Download Engine
- [x] **Step 4.1:** Write a Rust function in `src-tauri/src/core/parser.rs` to read the user-provided `memories_history.json` and insert the records into the SQLite database.
- [x] **Step 4.2:** Implement a concurrent download manager in `src-tauri/src/core/downloader.rs` using `tokio::spawn` and `reqwest`, limiting concurrency to 10 parallel downloads to avoid local network saturation.
- [x] **Step 4.3:** Emit download progress events from Rust to the React frontend using Tauri's `Window::emit`.
- [x] **Verification:** Feed a mock `memories_history.json` into the parser and verify that 10 files are successfully downloaded to a temporary local folder.

### Phase 5: Media Processing (Overlays & Metadata)
- [ ] **Step 5.1:** Implement FFmpeg logic in `src-tauri/src/core/media.rs` to overlay the transparent PNG (if it exists) onto the base MP4/JPEG file.
- [ ] **Step 5.2:** Use `kamadak-exif` (or `exiv2` bindings) to write the GPS coordinates and "Date Taken" from the database into the EXIF headers of the final merged files.
- [ ] **Step 5.3:** Implement a cleanup function to delete the raw downloaded files and overlays, keeping only the final merged file.
- [ ] **Verification:** Run the processing command on a test image. Use `exiftool test_image.jpg` in the terminal to verify the GPS and Date timestamps are correctly embedded.

### Phase 6: UI/UX & Mobile Adaptation
- [ ] **Step 6.1:** Build the React dashboard in `src/App.tsx` featuring a Dropzone for the JSON file, a location selector for the export directory, and a real-time Progress Bar listening to Tauri events.
- [ ] **Step 6.2:** Implement a "Resume Export" UI state. If links are expired (HTTP 403 from Snapchat), prompt the user to "Upload new JSON to resume".
- [ ] **Step 6.3:** Configure mobile capabilities in `src-tauri/gen/android/` and `src-tauri/gen/apple/` to request native Photos/Storage permissions.
- [ ] **Verification:** Run `npm run tauri android build` and ensure the `.apk` compiles successfully with the correct manifest permissions.

## 3. Global Testing Strategy
Once all phases are complete, validate the following critical path edge cases:
1. **The 24-Hour Expiration:** Start a download, forcefully disconnect the network, simulate a 24-hour wait (by manually replacing the media URLs in SQLite with expired ones), reconnect, and verify the app successfully prompts for a new JSON file and resumes exactly where it left off without duplicating files.
2. **Mobile Backgrounding:** On a physical mobile device, start a 5GB export, minimize the app to the background, open another heavy app (like a game), return after 10 minutes, and verify the download continued or paused gracefully without corrupting the SQLite state.
3. **Storage Quota:** Deliberately fill the target hard drive/device storage, attempt to download a memory, and verify the app catches the `Disk Full` I/O error and surfaces a clear, actionable UI alert to the user instead of silently crashing.
```

***

**Next Step:** Are you ready to initialize the project and execute Phase 1, or do you need to adjust any of the target directories before the Engineer agent takes over?