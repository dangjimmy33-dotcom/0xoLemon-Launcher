# Multiple Hugging Face depot repositories

The launcher now reads the ordered repository table in:

```text
src-tauri/huggingface-repos.json
```

Default order:

1. `Penaldo-CR7/PenaldoCR7`
2. `CatManga/Cat-Manga`

The launcher checks `catalog.json` in the first repository. If the game folder or file is not available there, it automatically falls back to the next repository. After a repository succeeds, that repository is kept as the preferred source for the remaining manifest and pack requests of the same job.

Example table:

```json
{
  "repositories": [
    {
      "repoId": "Penaldo-CR7/PenaldoCR7",
      "repoType": "dataset",
      "revision": "main",
      "enabled": true
    },
    {
      "repoId": "CatManga/Cat-Manga",
      "repoType": "dataset",
      "revision": "main",
      "enabled": true
    }
  ]
}
```

The game folder inside each repository is still controlled by:

```text
src-tauri/src/remote_paths.rs
```

For example, `geometry-dash` maps to the case-sensitive Hugging Face folder `Geometry-Dash`.

Optional runtime overrides:

```text
OXO_DEPOT_REPOS=Penaldo-CR7/PenaldoCR7,CatManga/Cat-Manga
```

or:

```text
OXO_DEPOT_REPO_BASES=https://huggingface.co/datasets/Penaldo-CR7/PenaldoCR7/resolve/main;https://huggingface.co/datasets/CatManga/Cat-Manga/resolve/main
```

`OXO_DEPOT_REPO_BASE` and `FIRST_LIGHT_DEPOT_BASE` remain supported for compatibility.

For a private repository, set `HF_TOKEN` or `FIRST_LIGHT_HF_TOKEN` before starting the launcher. Do not commit the token into source code.
