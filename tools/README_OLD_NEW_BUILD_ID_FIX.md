# Old/New Steam Build ID fix

Build-pair mode now has separate fields for:

- Old base version
- Old Steam Build ID
- Old final version string (optional exact override)
- New base version
- New Steam Build ID
- New final version string (optional exact override)

Generated command example:

```text
build-pair --old-version "1.0.0 (Build 21800000)" --new-version "1.0.7.1 (Build 22012444) - Uploaded 2026-06-18"
```

Use `Old final version string` when the previous catalog version included an upload date or another suffix that must match exactly.
