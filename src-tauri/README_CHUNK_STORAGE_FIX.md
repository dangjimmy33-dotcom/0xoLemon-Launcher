# Chunk storage and completed-job cleanup

Depot chunks are staged only inside the selected game library:

```text
<library>/
  common/<game>/                 # assembled game files
  downloading/<game>/chunks/     # temporary verified transport chunks
```

No second chunk store is created in AppData.

While a job is active, the `downloading/<game>` tree is kept for pause/resume and recovery.
After a successful install, update, or repair has written `.0xolemon/state.0xo` and
`.0xolemon/manifest.0xo`, the launcher removes the complete game-specific
`downloading/<game>` directory, including chunks, staged files and temporary leftovers.
Windows cleanup is retried to tolerate short-lived file handles.

Failed or still-active resumable jobs are not removed automatically. Explicit Cancel + Clean
continues to remove the active downloading directory and current job journal.
