# Changelog

## 0.2.0 - 2026-10-22

### Added
- **Local REST API for the Claude CLI.** `drui --api` (or Actions > Toggle REST API, or the main menu) starts an HTTP server on `127.0.0.1` with a bearer token, so Claude Code and scripts can read and edit the open file while you watch. Every edit is one undo step. `GET /guide` explains how an agent should work; the same text is in `docs/agent.md`, and `tools/dr.py` is a small client.
- Edits are guarded: the API answers `403` for any layer outside a `claude_edit` copy of a design (made with `POST /claude_edit`), and agents leave notes beside what they changed (`POST /notes`).
- Coverage of the app's functions (`docs/api-coverage.md`): layers and batch patches, selection, every Actions palette command, align and distribute, undo and redo, pages, save / open / import, export, pictures and instances, copy and paste, variables, styles, comments, version history, branches and reviews, libraries, view and preferences, the Motion timeline, the cut / eraser / shape builder / paint tools by coordinates, and Present.
- Destructive requests need an explicit say-so: restoring a version, editing or deleting a style, opening or importing over unsaved work. Approving a branch review stays with the person.
- The API limits itself to 30 requests a second.

### Changed
- The REST API is no longer reserved for a paid build; it needs no account.

## 0.1.0 - 2026-10-21

First preview: the editor, `.fig` and Sketch import, prototyping, Motion, Dev Mode measurements, and the Windows installer.
