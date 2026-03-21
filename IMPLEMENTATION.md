This is a massive and exciting architectural pivot. Based on the new JSON structure you provided and the new local ZIP contents, we have a clear path. 

**The biggest architectural discovery here:** Snapchat has completely removed the `Overlay Link` from the JSON. Overlays are no longer explicitly defined in the data export file. Instead, they are implicitly tied together in the ZIP files using the `mid` (Media ID) from the URL.

Because of this, your Rust backend must parse the `mid` out of the `Download Link` URL parameters, reconstruct the expected filename (`<timestamp>_<mid>-main` and `<timestamp>_<mid>-overlay`), and actively hunt for them inside the 2GB ZIP archives. If the ZIP search comes up empty, it falls back to a network download.

Here is the comprehensive, strictly formatted `IMPLEMENTATION.md` blueprint to hand over to your Engineer Agent.

***

```markdown
# IMPLEMENTATION.md

## 1. Project Context & Architecture
- **Goal:** Overhaul the Local Media Vault to process the modern Snapchat Export format. Implement Job-based session tracking, BLAKE3 content deduplication, dynamic ZIP content searching (using `mid`), minimal-footprint staging, pause/resume state machines, and structured `YYYY/MM` outputs.
- **Tech Stack & Dependencies:**
  - **Rust:** `cargo add uuid blake3 url chrono reqwest zip tokio`
  - **Frontend:** React, Tailwind, Shadcn UI (`lucide-react` for icons).
- **File Structure Updates:**
  ```text
  ├── src-tauri/src/
  │   ├── core/
  │   │   ├── parser.rs      # JSON parsing and URL 'mid' extraction
  │   │   ├── zip_hunter.rs  # NEW: Scans ZIPs for specific MIDs
  │   │   ├── processor.rs   # FFmpeg, BLAKE3, and Staging
  │   │   └── state.rs       # NEW: Pause/Resume Atomic Flags
  │   └── db/schema.rs       # Job and Content Hash tables
  ```
- **Attention Points:** - Overlays are no longer in the JSON; they must be found locally via `mid` matching (`<date>_<mid>-overlay.png`).
  - Strict Sliding Window extraction: Extract ONLY the files currently being processed into `.staging` to minimize disk space.

## 2. Execution Phases

### Phase 1: Database Schema & Job State (SQLite)
- [x] **Step 1.1:** In `src-tauri/src/db/schema.rs`, create the `ExportJobs` table: `id TEXT PRIMARY KEY, created_at DATETIME, status TEXT`.
- [x] **Step 1.2:** Update the `Memories` table to include `job_id TEXT`, `mid TEXT`, `content_hash TEXT`, `relative_path TEXT`, `thumbnail_path TEXT`, and `status TEXT`. Add a `UNIQUE` constraint on `content_hash`.
- [x] **Step 1.3:** Create a `ProcessedZips` table: `job_id TEXT, filename TEXT, status TEXT, PRIMARY KEY (job_id, filename)`.
- [x] **Step 1.4:** In `src-tauri/src/db/mod.rs`, expose Tauri commands to read Job state, Pause/Resume flags, and ZIP status for the frontend.
- [ ] **Verification:** Run `npm run tauri dev`, open the console, and execute the DB initialization. Verify using a local SQLite viewer that the `ExportJobs` and updated `Memories` tables exist.

### Phase 2: JSON Parsing & MID Extraction
- [x] **Step 2.1:** In `src-tauri/src/core/parser.rs`, define the Serde structs to match the new JSON format (`Date`, `Media Type`, `Location`, `Download Link`, `Media Download Url`).
- [x] **Step 2.2:** Implement logic using the `url` crate to parse the `Download Link` and extract the `mid` query parameter (e.g., `9a5a9ce7...`).
- [x] **Step 2.3:** Parse the `Date` string using `chrono` to extract the `YYYY-MM-DD` component. Store the `mid` and parsed Date in the `Memories` table under the active `job_id`.
- [ ] **Verification:** Create a mock JSON with 2 entries. Run a test Rust function to parse it. Verify the DB has 2 rows with the correctly extracted `mid` strings.

### Phase 3: The "Zip Hunter" & Sliding Window Staging
- [x] **Step 3.1:** In `src-tauri/src/core/zip_hunter.rs`, implement `find_and_extract_memory(zip_paths, date, mid)`. It must use the `zip` crate to iterate through the provided ZIPs without extracting them entirely.
- [x] **Step 3.2:** Inside the loop, check if filenames contain `<date>_<mid>-main` or `<date>_<mid>-overlay`.
- [x] **Step 3.3:** If found, extract ONLY those specific files to `.staging/`. 
- [x] **Step 3.4:** If the `main` file is NOT found in any ZIP, fallback to `reqwest` to download it via the `Media Download Url` into `.staging/`. If the download fails, update DB status to `FAILED_NETWORK`.
- [x] **Verification:** Place a mock ZIP containing `2026-02-20_9a5a...-main.mp4` in the directory. Run the hunter function with the corresponding `mid`. Verify ONLY that file appears in `.staging/`.

### Phase 4: BLAKE3 Deduplication & FFmpeg Processing
- [x] **Step 4.1:** In `src-tauri/src/core/processor.rs`, before running FFmpeg, read the staged `main` file into a `blake3::Hasher`. 
- [x] **Step 4.2:** Query the DB for `content_hash`. If a match exists, mark memory as `DUPLICATE`, delete from `.staging/`, and skip.
- [x] **Step 4.3:** If unique, check if an `overlay` exists in `.staging/`. If yes, run the FFmpeg burn-in command. If no, just copy the file.
- [x] **Step 4.4:** Format the final output path using `chrono`: `Export_Folder/YYYY/MM_MonthName/`. Generate a 300x300 thumbnail into `Export_Folder/.thumbnails/`.
- [x] **Step 4.5:** Update DB with `status = PROCESSED`, `relative_path`, and `thumbnail_path`. Clean the `.staging/` folder.
- [ ] **Verification:** Stage an image and a transparent PNG overlay. Run the processor. Verify a merged image appears in `2026/02_February/` and a thumbnail in `.thumbnails/`.

### Phase 5: State Machine Control (Pause/Stop)
- [x] **Step 5.1:** In `src-tauri/src/core/state.rs`, utilize `std::sync::atomic::AtomicBool` or `tokio::sync::watch` to represent `is_paused` and `is_stopped`.
- [x] **Step 5.2:** Wrap the main processing loop in `src-tauri/src/core/processor.rs` with checks for these flags. If `is_paused`, await a signal. If `is_stopped`, gracefully break the loop, leaving unfinished items as `PENDING` in the DB.
- [ ] **Verification:** Trigger the processing loop. Call the "Pause" Tauri command from the React frontend. Verify console logs show the loop pausing without crashing.

### Phase 6: Frontend Progress & Viewer UI
- [x] **Step 6.1:** In `src/features/downloader/components/Workflow.tsx`, update UI to accept Snapchat ZIP exports only (`mydata~<uuid>` main + optional numbered parts).
- [x] **Step 6.2:** Create a Live Console component that listens to Tauri events. Display structured status strings during download/processing.
- [x] **Step 6.3:** Show global progress: `Files Processed: X / Y`, `Duplicates Skipped: Z`, `Active ZIP: <name>`. Add Pause and Stop buttons bound to Tauri state commands.
- [ ] **Verification:**
  - Select Snapchat ZIP files only; confirm the main ZIP (`mydata~<uuid>`) must contain `json/memories_history.json` and `memories/`.
  - Start a mock session and verify live console logs update with timestamps/status, and that ZIP completion rows appear.
  - Verify `Files Processed`, `Duplicates Skipped`, and `Active ZIP` update during runtime.
  - Click Pause and confirm progress stops advancing until Resume.
  - Close and reopen app, click `Reload Session State`, and confirm current session state and finished ZIP statuses are restored.

## 3. Global Testing Strategy
1. **The Missing File Fallback Test:** Provide a JSON with a `mid`, but DO NOT put the file in the provided ZIPs. Verify the app recognizes it is missing, falls back to the HTTP `reqwest` download, successfully stages it, processes it, and marks it `PROCESSED`.
2. **The 6-Month Drunk Duplicate Test:** Upload a JSON containing a `mid` that maps to a video already existing in the `Memories` DB (simulating a duplicate save). Verify the `blake3` hash catches it instantly, flags it as `DUPLICATE`, and the final folder does not contain a duplicate file.
3. **The Pause & Force Quit Test:** Start a large batch. Click "Pause". Verify CPU usage drops to 0. Force quit the app. Reopen. Verify the app detects the unfinished job in SQLite and safely resumes exactly at the uncompleted file.
```