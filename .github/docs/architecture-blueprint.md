# Architecture Blueprint

## Purpose
Define a clear IPC bridge between the React frontend (`src/`) and Rust backend (`src-tauri/src/commands/`) for a local-first Snapchat Memories processor.

## System Boundaries
- `src/` owns user interaction, view state, and progress rendering.
- `src-tauri/src/commands/` exposes typed, serializable command handlers to the frontend.
- `src-tauri/src/core/` contains business logic (parsing, downloading, media processing).
- `src-tauri/src/db/` owns SQLite persistence and state queries.

## Data Flow Overview
1. User selects `memories_history.json` and output directory in React UI.
2. React invokes Tauri commands to parse and persist memory metadata.
3. Rust backend runs download/processing jobs asynchronously.
4. Rust emits progress/status events to the frontend.
5. React updates progress UI and supports resume flow on failure/expiration.

## IPC Contract Design
All command payloads and responses must be serializable and strongly typed.

### Frontend Invocation Layer (`src/lib/ipc.ts`)
- Centralize all `invoke` and event subscription logic in a single typed module.
- Avoid direct `invoke` usage inside React components.
- Convert unknown responses into validated TypeScript domain models.

### Backend Command Layer (`src-tauri/src/commands/`)
- Keep command handlers thin: validate input, call `core`/`db`, map errors.
- Return explicit response structs and typed error variants.
- Avoid long-running CPU work on the command thread.

## Suggested Command Surface
- `parse_memories_json(request) -> ParseMemoriesResponse`
- `start_export(request) -> ExportJobResponse`
- `pause_export(request) -> ExportJobResponse`
- `resume_export(request) -> ExportJobResponse`
- `get_job_state(request) -> JobStateResponse`
- `list_failed_items(request) -> FailedItemsResponse`
- `retry_failed_items(request) -> ExportJobResponse`

## Suggested Event Surface
- `export://job-progress`
- `export://item-status`
- `export://job-status`

## Type Safety Standards
- **Frontend (TypeScript):** no `any`, validate unknown payloads.
- **Backend (Rust):** use serde DTOs and result-based error handling.

## Error and Resume Strategy
- Persist item status (`pending`, `downloading`, `done`, `failed`, `expired`) in SQLite.
- Map HTTP `403` to an `expired` domain error and prompt for refreshed JSON.
- Recover job state from SQLite after app restart.

## Security and Privacy Constraints
- Keep all processing local.
- Limit filesystem scope to user-approved roots.
- Minimize sensitive logs.
