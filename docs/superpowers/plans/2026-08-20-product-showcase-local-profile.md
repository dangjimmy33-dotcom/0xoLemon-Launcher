# Product Showcase + Local Profile Customization Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn What's New into a reusable product-showcase page and let the signed-in user customize a local Discord-style profile cover/bio/status with automatic local backup.

**Architecture:** Keep the social prototype frontend-only. Persist editable profile metadata and cover bytes under Tauri AppLocalData with a browser localStorage fallback, expose that state through the existing SocialPrototype provider, and render it in profile surfaces. Replace the current changelog-only What's New page with a data-driven Motion showcase while preserving release history and compact modal behavior.

**Tech Stack:** React 19, TypeScript 6, Motion 12, Tauri 2 dialog/fs plugins, CSS custom properties.

**Spec:** Approved in chat on 2026-08-20.

## Global Constraints

- No backend/social-server writes.
- No Discord token is persisted in profile metadata.
- Cover image stays on the user's device.
- Preserve existing changelog modal behavior.
- Respect current Color Studio tokens and reduced-motion preferences.
- Do not hotlink or bundle Sushi Launcher artwork; only reuse interaction/layout ideas.

---

### Task 1: Contract tests for local profile customization

**Files:**
- Create: `src/lib/profileCustomization.contract.test.mjs`
- Create: `src/social/socialProfileStorage.ts`
- Modify: `src/social/SocialPrototype.tsx`
- Modify: `src/social/SocialPrototype.css`
- Modify: `src-tauri/capabilities/default.json`

- [ ] Write a failing contract asserting local AppLocalData persistence, rolling backup, image picker, self-cover rendering, profile editor, and required fs permissions.
- [ ] Run it and verify failure.
- [ ] Implement minimal storage + UI integration.
- [ ] Run contract and existing social contracts.

### Task 2: Product showcase page

**Files:**
- Create: `src/lib/productShowcase.contract.test.mjs`
- Create: `src/components/WhatsNewView.css`
- Modify: `src/components/WhatsNewView.tsx`

- [ ] Write a failing contract asserting data-driven feature showcase, Motion scroll hooks, release history anchor, and compact modal path.
- [ ] Run it and verify failure.
- [ ] Implement product hero, animated feature stages, and release history.
- [ ] Run showcase + theme contracts.

### Task 3: Verification and packaging

- [ ] Transpile all changed TS/TSX with the installed TypeScript parser.
- [ ] Parse JSON capability file.
- [ ] Run all relevant `.contract.test.mjs` files.
- [ ] Create ZIP and verify archive integrity.
