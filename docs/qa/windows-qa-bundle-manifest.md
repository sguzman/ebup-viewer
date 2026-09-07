# Windows QA Bundle Manifest

The Goal 0007 artifact `lanternleaf-windows-qa` is a self-contained extracted directory for the Gate 2 real-desktop checklist.

| Path | Purpose |
|---|---|
| `lanternleaf.exe` | Native Rust/egui executable built by the hosted Windows job |
| `conf/*.toml` | Runtime application, normalizer, abbreviation, and quack-check configuration |
| `conf/pandoc/strip-nontext.lua` | Source conversion filter used by HTML/EPUB ingestion |
| `tools/pandoc/pandoc.exe` | Bundled Pandoc runtime; the maintainer does not install Pandoc |
| `qa-sources/representative.txt` | Accepted Goal 0006 TXT fixture |
| `qa-sources/representative.md` | Accepted Goal 0006 Markdown fixture |
| `qa-sources/representative.html` | Accepted Goal 0006 HTML fixture |
| `qa-sources/representative.epub` | Deterministic two-chapter EPUB generated during staging |
| `run-lanternleaf-qa.ps1` | Bundle-relative launcher and log handoff |
| `BUNDLE-MANIFEST.txt` | Machine-readable artifact summary |

The launcher creates only bundle-local `qa-logs/`, `qa-handoff/`, and `qa-cache/` directories. It sets process-local configuration and `PATH` values, so it does not install software or persist global environment changes. Fonts are resolved from the Windows system by the native egui shell; the application icon is compiled into the executable.

Run from the extracted bundle directory:

```powershell
.\run-lanternleaf-qa.ps1
```
