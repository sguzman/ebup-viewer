# PDF Performance Budget Profiles

## Profiles

### low_memory

- Canvas budget: `4`
- Text-layer budget: `2`
- Text-span budget: `900`
- Bitmap artifact budget: `4`
- Overscan: `0`
- Prefetch delay: `64ms`

### balanced

- Canvas budget: `8`
- Text-layer budget: `4`
- Text-span budget: `2200`
- Bitmap artifact budget: `10`
- Overscan: `1`
- Prefetch delay: `32ms`

### high_memory

- Canvas budget: `12`
- Text-layer budget: `6`
- Text-span budget: `4200`
- Bitmap artifact budget: `16`
- Overscan: `2`
- Prefetch delay: `16ms`

## Selection

- Local override: `localStorage["ll.pdfPerformanceProfile"] = "low_memory" | "balanced" | "high_memory"`
- Automatic low-memory fallback:
  - `deviceMemory <= 4`, or
  - `hardwareConcurrency <= 4`
- Automatic high-memory selection:
  - `deviceMemory >= 16`, and
  - `hardwareConcurrency >= 8`

## Intent

- Keep low-memory laptops from stalling on too many live canvases or text layers.
- Let higher-memory desktops use slightly larger caches and overscan windows without reintroducing whole-document rendering.
- Keep the same scheduler model across all devices; only the budgets change.
