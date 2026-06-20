# Integrated frontend features

- Real running state driven by Rust process events instead of a fake timeout.
- PID, launch count, total playtime, and launcher achievement progress on the game page.
- Real-time achievement toast.
- Verified depot chunks are retained directly in the library `downloading/<game>/chunks` directory for resume, repair, and future updates.
- Chunk data is never copied to AppData; AppData is reserved for small launcher state only.
- Existing custom installs are registered with the Rust backend after scanning.
- Game-description HTML is sanitized before it reaches `dangerouslySetInnerHTML`.

Validation performed:

- All TypeScript/TSX files were parsed with esbuild.
- A local esbuild bundle was produced with external runtime dependencies and external font assets.

A full `npm build` was not possible because the uploaded `src` archive did not include `package.json`, lockfile, Vite config, TypeScript config, or installed dependencies.
