# Copilot Instructions

## Project Stack
- Application framework: Tauri v2
- Frontend: React + TypeScript
- UI: Tailwind CSS (and Shadcn UI when added)
- Backend: Rust

## Architecture Expectations
- Keep the app local-first. Do not introduce external backend services for core processing.
- Use Tauri commands for privileged operations; keep browser-side code focused on UI/state.
- Prefer explicit IPC contracts with serializable request/response types.

## TypeScript Standards (Strict)
- Use strict TypeScript settings and maintain type safety at all boundaries.
- Do not use `any`; prefer precise interfaces, discriminated unions, and generics.
- Validate and narrow unknown input before use.
- Keep React components typed, including props and event handlers.
- Prefer small, pure utility functions and avoid hidden side effects.

## Rust Standards (Clippy + Quality)
- Code must pass `cargo fmt` and `cargo clippy --all-targets --all-features -D warnings`.
- Avoid `unwrap()`/`expect()` in non-test code; return typed errors with context.
- Prefer strongly typed structs/enums over loosely typed maps.
- Keep async code non-blocking; move CPU-heavy work to dedicated threads.
- Structure modules by responsibility (`commands`, `core`, `db`) and keep command handlers thin.

## Security and Privacy
- Treat all user exports and media as sensitive local data.
- Minimize logging of personal identifiers or file contents.
- Request only required filesystem capabilities and permissions.
- Never transmit media or metadata to third-party services by default.

## Change Discipline
- Implement only the requested phase/step scope.
- Keep patches minimal and consistent with existing style.
- Update implementation checklist status when a step is completed and validated.
