# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**lovshot** - A GIF/screenshot capture desktop application built with Tauri 2 + React 19 + TypeScript + Vite. Supports region-based screenshot and GIF recording with global hotkey activation.

## Development Commands

```bash
pnpm tauri dev      # Run desktop app with hot reload (preferred)
pnpm dev            # Frontend only (port 1420)
pnpm tauri build    # Production build
pnpm build          # Type check (tsc && vite build)
```

## Architecture

### Multi-Window Design
- **Main window** (`src/App.tsx`): Control panel showing recording status, stop button
- **Selector window** (`public/selector.html`): Full-screen transparent overlay for region selection

### Core Flow
1. Global hotkey `Shift+Alt+A` triggers selector window
2. User drags to select region, chooses mode (screenshot/GIF)
3. Rust backend captures via `screenshots` crate
4. Screenshots save to clipboard + `~/Pictures/lovshot/`
5. GIF recording runs in background thread, encodes asynchronously

### Rust Backend (`src-tauri/src/lib.rs`)
- `AppState` holds recording state, frames buffer, region, FPS settings
- Commands: `get_screens`, `capture_screenshot`, `open_selector`, `set_region`, `start_recording`, `stop_recording`, `save_screenshot`, `save_gif`, `set_fps`
- Events emitted: `recording-state` (frame count updates), `save-complete` (async save result)
- macOS-specific: Uses `objc` crate to set window level above dock

### Key Dependencies
- **Rust**: `screenshots` (screen capture), `gif` (encoding), `image` (processing), `tauri-plugin-clipboard-manager`
- **Frontend**: `@tauri-apps/api` for IPC, `@tauri-apps/plugin-global-shortcut`

## Key Patterns

- **Tauri Commands**: Define with `#[tauri::command]`, register in `invoke_handler`
- **Frontend-Backend IPC**: `invoke()` for commands, `listen()` for events
- **Coordinate System**: Selector passes logical pixels; Rust uses them directly with `capture_area`
- **Async GIF Save**: `save_gif` returns immediately, emits `save-complete` when done

## Bundle Identifier

`app.lovpen.shot`
