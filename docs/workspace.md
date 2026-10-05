# Project workspace

`WorkspaceManager` is the single owner of the canonical workspace root. The frontend supplies only relative paths after project selection; the backend rejects absolute paths, parent traversal, missing targets, and canonical paths that escape through symlinks.

Directory listings are lazy, sorted with directories first, and filtered using `.gitignore` plus configurable default exclusions. Absolute child paths never cross IPC. File reads distinguish UTF-8 text, invalid text, and binary content.

Project detection reads only known manifests and lockfiles. Project Map V1 performs a bounded, ignore-aware metadata walk (50,000 files maximum), collecting top-level directories, configuration files, extensions, entry points, tests, scripts, and dependency summaries without AI. The active workspace and ten most recent projects are persisted in the application config directory.
