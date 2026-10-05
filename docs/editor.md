# Monaco editor

GravityForge uses a local Monaco distribution with Vite workers; it does not depend on a CDN or iframe. `editorStore` owns multi-tab state, active file, original/current content, dirty flags, language, encoding, modification timestamp, read-only state, external conflicts, and diff mode.

All reads and mutations cross typed Tauri commands. Saves use a workspace-confined temporary file, flush it, and atomically rename it over the destination. Path canonicalization is repeated immediately before operations to prevent traversal and symlink escape.

The editor supports native Monaco search, replace, go-to-line, undo/redo, format action, minimap, breadcrumbs, a persisted open-file session, optional one-second autosave, Quick Open, command palette, and original-versus-current diff. File timestamps are polled while tabs are open: clean tabs reload; dirty tabs receive a conflict marker and are never overwritten automatically. Files over 5 MiB open read-only.
