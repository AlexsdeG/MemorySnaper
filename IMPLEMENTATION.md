```markdown
# IMPLEMENTATION.md

## 1. Project Context & Architecture
- **Goal:** Refactor the existing React monolithic frontend into a Feature-Sliced Design and implement the Rust backend for a Production-Grade Local Media Vault. Features include ZIP/JSON extraction, SQLite-based deduplication (handling multi-part videos), a two-state download/process pipeline, a virtualized local media viewer, and configurable rate limiting.
- **Tech Stack & Dependencies:**
  - **Frontend:** React, Tailwind CSS, Shadcn UI (`npm install @tanstack/react-virtual lucide-react react-router-dom`)
  - **Backend:** Tauri v2 (Rust 1.80+)
  - **Rust Crates:** `cargo add serde_json zip tokio reqwest tauri-plugin-sql sha2`
- **File Structure:**
  ```text
  ├── src/
  │   ├── components/layout/   # Mobile-responsive Bottom/Top Tabs
  │   ├── features/
  │   │   ├── downloader/      # ZIP/JSON Dropzone, Two-Button Workflow
  │   │   ├── viewer/          # Virtualized Grid
  │   │   └── settings/        # Rate Limits, Paths
  │   ├── lib/                 # DB, API, Utils
  │   └── App.tsx              # Tab Routing
  ├── src-tauri/src/
  │   ├── commands/            # Tauri IPC Endpoints
  │   ├── core/                # ZIP extractor, Downloader, FFmpeg
  │   └── db/                  # SQLite Initialization & Queries
  ```
- **Attention Points:** - Never load full-resolution MP4s/JPEGs into the Viewer; only load generated WebP thumbnails.
  - Rate limiting is critical to avoid IP bans from Snapchat's AWS.
  - Deduplication must handle multi-part videos by grouping multiple media URLs under a single Memory ID.

## 2. Execution Phases

### Phase 1: Frontend Modularity & Routing
- [x] **Step 1.1:** In `src/App.tsx`, remove existing monolithic UI code and implement a Tab-based layout (Downloader, Viewer, Settings) using standard state or a router. Place tabs at the bottom for mobile screens (`max-md:bottom-0 max-md:fixed`).
- [x] **Step 1.2:** Create directories `src/features/downloader`, `src/features/viewer`, and `src/features/settings`.
- [x] **Step 1.3:** Move the existing Shadcn UI elements (Cards, Progress, Buttons) from the monolith into placeholder components within their respective `features/` folders.
- [x] **Verification:** Run `npm run tauri dev`. Verify the app launches, the tab navigation works, and the UI adapts to mobile dimensions without errors.

### Phase 2: Rust Backend - ZIP Extraction & Validation
- [x] **Step 2.1:** In `src-tauri/src/core/parser.rs`, create a function using the `zip` crate to extract `memories_history.json` into a temporary system directory if the input is a `.zip`. If the input is `.json`, read it directly.
- [x] **Step 2.2:** In the same file, use `serde_json` to validate the JSON schema matches the expected Snapchat export format.
- [x] **Step 2.3:** In `src-tauri/src/commands/file.rs`, expose `#[tauri::command] validate_memory_file(path: String)` returning a boolean or error string to React.
- [ ] **Verification:** Call `invoke('validate_memory_file', { path: "test.zip" })` from the React console and verify it successfully extracts and parses a valid mock file.

### Phase 3: SQLite Deduplication & Multi-Part Video Handling
- [x] **Step 3.1:** In `src-tauri/src/db/schema.rs`, initialize `tauri-plugin-sql` and create a `Memories` table (id, hash, date, status). The `hash` is generated via `sha2` using Date + Media Type to ensure deduplication across 6-month updates.
- [x] **Step 3.2:** In the same file, create a `MediaChunks` table (id, memory_id, url, overlay_url, order_index) with a foreign key to `Memories`. This allows multiple 10-second video split URLs to belong to one single Memory.
- [x] **Step 3.3:** In `src-tauri/src/core/parser.rs`, write the logic to iterate through the parsed JSON, generate hashes, and execute `INSERT OR IGNORE` into `Memories`, followed by inserting into `MediaChunks`.
- [ ] **Verification:** Parse a JSON file twice. Query the SQLite DB and verify that `Memories` count remains the same (no duplicates) and multi-part videos have corresponding rows in `MediaChunks`.

### Phase 4: The Two-Button Workflow (Download & Process)
- [x] **Step 4.1:** In `src/features/downloader/components/Workflow.tsx`, implement the UI state: Show "Start Download" if un-downloaded chunks exist. Switch button to "Process Files" when downloads complete.
- [x] **Step 4.2:** In `src-tauri/src/core/downloader.rs`, implement `download_media` using `reqwest`. Download files to a `.raw_cache` folder. Update DB status.
- [x] **Step 4.3:** In `src-tauri/src/core/processor.rs`, implement `process_media`. Read from `.raw_cache`, run FFmpeg `concat` (for multi-part videos) and overlay burns, inject EXIF, generate a 300x300 `webp` thumbnail to `.thumbnails`, and move the final file to the user's export path.
- [x] **Step 4.4:** Emit progress events from Rust to update the Progress UI in React.
- [ ] **Verification:** Click "Start Download" -> observe files in `.raw_cache`. Click "Process Files" -> observe processed files in output and `.webp` files in `.thumbnails`.

### Phase 5: Virtualized Viewer
- [x] **Step 5.1:** In `src/features/viewer/components/Grid.tsx`, implement `@tanstack/react-virtual`.
- [x] **Step 5.2:** Create a Rust command `get_thumbnails(offset, limit)` to query the SQLite DB for processed memories and return local file paths to the `.thumbnails` folder.
- [x] **Step 5.3:** Connect the virtualized grid to the Tauri command, rendering `<img>` tags mapping to the local Tauri asset protocol (`convertFileSrc`).
- [ ] **Verification:** Populate the DB with 5,000 mock entries. Scroll the Viewer rapidly. Verify memory usage in Task Manager remains stable (no memory leaks or DOM node explosion).

### Phase 6: Settings & Rate Limiting
- [x] **Step 6.1:** In `src/features/settings/components/SettingsForm.tsx`, create an input for "Requests per Minute" and "Concurrent Downloads". Show a red warning text if Requests per Minute > 100 or Concurrent > 5.
- [x] **Step 6.2:** Save these settings in local storage or SQLite.
- [x] **Step 6.3:** In `src-tauri/src/core/downloader.rs`, wrap the `tokio` HTTP requests in a `tokio::sync::Semaphore` initialized with the "Concurrent Downloads" setting, and implement a time delay to respect "Requests per Minute".
- [ ] **Verification:** Set rate limit to 10 RPM. Start download. Verify via console logs that Rust waits ~6 seconds between HTTP requests.

## 3. Global Testing Strategy
1. **The Incremental Update (6-Month Scenario):** Load a JSON file with 10 memories. Download and process them. Alter the JSON to include 5 *new* memories and 5 *old* ones. Upload it again. Verify the app skips downloading/processing the 5 old ones and exclusively targets the 5 new ones.
2. **The Multi-Part Video Stitch:** Provide a mocked JSON where a 30-second video is split into three 10-second AWS URLs. Download and process. Verify the output is exactly ONE 30-second `.mp4` file that plays seamlessly.
3. **The Mobile Crash Test:** On Android, set rate limits to maximum (triggering the warning). Attempt to download 500 files while simultaneously scrolling the Virtualized Viewer aggressively. Ensure the UI thread does not freeze.
```