# CloudRedirect 2.6.3 Integration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** Replace the partial CloudRedirect reimplementation with a typed adapter over the upstream 2.6.3 engine and a fully integrated bilingual launcher UI.

**Architecture:** Vendor the upstream engine source and build only the Windows native engine targets. A Rust adapter manages extraction, configuration, credentials, CLI invocation, and events. A React view exposes the engine through 0xoLemon components and i18n.

**Tech Stack:** Rust/Tauri 2, React 19, TypeScript 6, CMake/MSVC C++, upstream CloudRedirect 2.6.3.

## Global Constraints
- Do not use the upstream WPF UI or updater.
- Preserve existing local Cloud Save v5 and download/update modules.
- Keep destructive operations serialized and explicitly confirmed by the UI.
- Keep `en-US` and `vi-VN` key sets identical.

---

### Task 1: Vendor and build the upstream engine
- [x] Copy upstream 2.6.3 source to `src-tauri/vendor/cloudredirect`.
- [x] Add `src-tauri/build-cloudredirect.ps1` that builds Release native targets and copies runtime binaries into `src-tauri/resources/cloud_redirect/engine/2.6.3`.
- [x] Add version metadata and source attribution.
- [x] Update Tauri resources and before-build command.

### Task 2: Typed Rust engine adapter
- [x] Add engine models, command runner, config store, credential store, and operation coordinator.
- [x] Implement status, deploy, mode, provider config/test, remote apps/files, sync, migration, cleanup, delete, diagnostics, stats and Cloud760 commands.
- [x] Keep OAuth compatibility entry points, but remove legacy mock sync commands from the exposed Tauri command surface.
- [x] Register every command in `lib.rs`.

### Task 3: Integrated React UI
- [x] Replace the monolithic legacy component with Overview, Provider, Games, Backups, Migration, Maintenance and Diagnostics tabs.
- [x] Add real progress/events and confirmation flows.
- [x] Keep existing visual tokens and responsive behavior.

### Task 4: i18n and contracts
- [x] Add matching English and Vietnamese strings.
- [x] Add contract tests for command names, supported provider values, and i18n parity.
- [x] Run available syntax/static checks and package the result.
