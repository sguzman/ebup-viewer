# Egui Config Cache And Persistence Roadmap

## Objective
- [ ] Consolidate settings, config, bookmarks, recents, and content artifact persistence under pure Rust ownership for the egui app.
- [ ] Preserve current user data semantics while removing the mixed Tauri/UI presentation layer from persistence flows.
- [ ] Define deterministic migration, compatibility, and invalidation rules for existing data.

## Current-State Grounding In This Repo
- Current persistence and config ownership is already heavily Rust-side:
- `src/cache.rs`
- `src/cache/*`
- `src/config/*`
- source/artifact caching under `.cache/lantern-leaf/<source_hash>/`
- Tauri currently coordinates some persistence lifecycle points through command handlers and shutdown/session flows.
- Existing persistence concerns include:
- base config
- per-book overrides
- bookmarks
- recent books
- thumbnails
- dual-view content artifacts
- PDF sync/OCR artifacts
- browser-tab snapshot artifacts

## Target End State Under Egui
- The egui app uses the same Rust persistence layers, updated as needed for:
- new app crate ownership
- cache versioning during migration
- removal of Tauri-specific assumptions
- deterministic startup/open/close/quit persistence behavior
- User data remains Rust-native and independent of any frontend bridge or web runtime.

## Key Architectural Decisions Already Chosen
- Existing Rust persistence logic should be preserved where possible.
- End state is pure Rust; no persistence behavior should depend on TypeScript/Tauri glue.
- Existing cache/config/bookmark semantics should be preserved unless a deliberate migration rule says otherwise.
- Migration must include explicit compatibility and invalidation behavior rather than silent breakage.

## Phase 1: Persistence Surface Inventory
- [ ] Inventory all persisted data and lifecycle triggers:
- [ ] app config load/save
- [ ] per-book config override load/save
- [ ] bookmarks
- [ ] recent books
- [ ] content artifacts
- [ ] thumbnails
- [ ] PDF artifacts
- [ ] browser-tab artifacts
- [ ] Define which of these remain unchanged vs need schema/version updates for egui cutover.
- Phase exit:
- [ ] there is a complete inventory of persisted data and owning services.

## Phase 2: App Lifecycle Ownership
- [ ] Define persistence behavior for:
- [ ] startup
- [ ] source open
- [ ] live session updates
- [ ] close source
- [ ] safe quit
- [ ] crash/recovery expectations
- [ ] Ensure no lifecycle step depends on Tauri shell semantics after migration.
- Phase exit:
- [ ] egui app lifecycle and persistence interactions are explicit.

## Phase 3: Schema And Compatibility Plan
- [ ] Define compatibility rules for existing config and cache data:
- [ ] read existing config/bookmark formats where possible
- [ ] version cache artifacts that depend on renderer/runtime ownership
- [ ] invalidate/rebuild artifacts whose assumptions change under egui
- [ ] keep user-visible data loss minimal and explicit when unavoidable
- Phase exit:
- [ ] there is a deterministic migration and invalidation strategy for existing users.

## Phase 4: Cache Layout And Artifact Strategy
- [ ] Review cache layout for:
- [ ] content artifacts
- [ ] image assets
- [ ] PDF sync/OCR/geometry artifacts
- [ ] browser-tab snapshots
- [ ] thumbnails
- [ ] Decide whether the current layout remains authoritative or needs versioned substructure for the egui migration.
- Phase exit:
- [ ] cache ownership and migration rules are explicit enough for implementation.

## Phase 5: Deletion, Recovery, And Rebuild Semantics
- [ ] Preserve or refine delete/recent-remove behavior for all source types.
- [ ] Preserve non-destructive rebuild behavior for corrupt/missing artifacts.
- [ ] Define recovery logging and diagnostic surfaces in the egui app.
- Phase exit:
- [ ] delete/recover/rebuild semantics are explicit and parity-safe.

## Risks / Failure Modes
- Data compatibility can silently regress if egui migration changes artifact assumptions without a versioning plan.
- Safe-quit semantics may weaken if app lifecycle handling is rewritten without persistence ownership clarity.
- Source-type-specific artifacts can be orphaned if delete logic is not treated as part of migration scope.

## Test / Parity Requirements
- [ ] Rust tests for config/bookmark/cache round-trips and migrations.
- [ ] Integration tests for reopen, delete, rebuild-after-corruption, and safe-quit persistence.
- [ ] Manual QA for upgrade scenarios from the Tauri app to the egui app using existing data directories.
- [ ] Full implementation-phase build verification excluding AppImage/RPM/DEB packaging outputs.

## Acceptance Criteria
- [ ] Persistence ownership is fully Rust-native and independent of Tauri/UI layers.
- [ ] Existing user data compatibility and invalidation rules are explicit.
- [ ] Delete/recover/rebuild semantics are preserved or deliberately redefined.
- [ ] The roadmap is specific enough to implement persistence cutover without reopening behavior decisions.
