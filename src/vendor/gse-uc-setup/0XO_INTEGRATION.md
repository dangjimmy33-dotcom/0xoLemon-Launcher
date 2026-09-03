# 0xoLemon vendor snapshot

This directory is a source/reference snapshot of the user-supplied GSE_UC_Setup project.
0xoLemon does not execute this Python package at runtime. Native launcher behavior is implemented in Rust/React.
Binary resources are intentionally stored once under `src-tauri/resources/gse-uc/` instead of duplicated here.
The original Google OAuth client-secret payload is intentionally not vendored or bundled; 0xoLemon keeps its own save/cloud credential system canonical.
