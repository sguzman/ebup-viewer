# Reader Subscription Audit

## Scope

This audit covers direct store subscriptions to `reader` and reader-adjacent hot state.

## Current results

- `App.tsx` is the only screen-level component that subscribes to the full `reader` snapshot.
- `ReaderQuickActionsDock.tsx` does not subscribe to `reader` directly; it uses narrow quick-action selectors.
- Hidden app status, toast, theme, and starter-screen flows do not subscribe to `reader`.
- Components below `ReaderShell.tsx` receive `reader` by props rather than creating their own store subscriptions.

## Current boundary

- The remaining monolithic `reader` subscription is intentionally concentrated at the screen boundary.
- New components should prefer narrow selectors or tuple selectors instead of subscribing directly to `reader`.
- The next store reshape should split that screen-boundary subscription into document, playback, and UI domains.
