use super::*;

fn root() -> tempfile::TempDir {
    tempfile::tempdir().unwrap()
}
fn run(root: &Path, action: FsAction) -> FsResult<FsToolResult> {
    execute(&root.canonicalize().unwrap(), action)
}

#[test]
fn rejects_traversal_and_absolute_paths() {
    let dir = root();
    for path in ["../outside", "/etc/passwd"] {
        let error = run(dir.path(), FsAction::Read { path: path.into() }).unwrap_err();
        assert_eq!(error.code, FsErrorCode::OutsideWorkspace);
        assert_eq!(
            run(
                dir.path(),
                FsAction::Create {
                    path: path.into(),
                    content: "x".into()
                }
            )
            .unwrap_err()
            .code,
            FsErrorCode::OutsideWorkspace
        );
    }
}

#[cfg(unix)]
#[test]
fn rejects_symlink_escape_and_mutation() {
    use std::os::unix::fs::symlink;
    let dir = root();
    let outside = root();
    fs::write(outside.path().join("secret"), "private").unwrap();
    symlink(outside.path(), dir.path().join("escape")).unwrap();
    assert_eq!(
        run(
            dir.path(),
            FsAction::Read {
                path: "escape/secret".into()
            }
        )
        .unwrap_err()
        .code,
        FsErrorCode::OutsideWorkspace
    );
    assert_eq!(
        run(
            dir.path(),
            FsAction::Create {
                path: "escape/new".into(),
                content: "x".into()
            }
        )
        .unwrap_err()
        .code,
        FsErrorCode::OutsideWorkspace
    );
    assert_eq!(
        run(
            dir.path(),
            FsAction::Delete {
                path: "escape".into(),
                recursive: true
            }
        )
        .unwrap_err()
        .code,
        FsErrorCode::Unsupported
    );
    assert_eq!(
        fs::read_to_string(outside.path().join("secret")).unwrap(),
        "private"
    );
}

#[test]
fn read_range_handles_large_text_without_full_read() {
    let dir = root();
    fs::write(
        dir.path().join("large.txt"),
        "a".repeat(MAX_READ as usize + 20),
    )
    .unwrap();
    assert_eq!(
        run(
            dir.path(),
            FsAction::Read {
                path: "large.txt".into()
            }
        )
        .unwrap_err()
        .code,
        FsErrorCode::FileTooLarge
    );
    match run(
        dir.path(),
        FsAction::ReadRange {
            path: "large.txt".into(),
            start_byte: MAX_READ,
            length: 10,
        },
    )
    .unwrap()
    {
        FsToolResult::ReadRange {
            content,
            end_byte,
            has_more,
            ..
        } => {
            assert_eq!(content, "a".repeat(10));
            assert_eq!(end_byte, MAX_READ + 10);
            assert!(has_more);
        }
        _ => panic!("unexpected result"),
    }
}

#[test]
fn patch_uses_position_and_expected_lines() {
    let dir = root();
    fs::write(dir.path().join("note.txt"), "one\r\ntwo\r\nthree\r\n").unwrap();
    let hunks = vec![
        PatchHunk {
            start_line: 2,
            old_lines: vec!["two".into()],
            new_lines: vec!["TWO".into(), "inserted".into()],
        },
        PatchHunk {
            start_line: 4,
            old_lines: vec![],
            new_lines: vec!["four".into()],
        },
    ];
    run(
        dir.path(),
        FsAction::Patch {
            path: "note.txt".into(),
            hunks,
        },
    )
    .unwrap();
    assert_eq!(
        fs::read_to_string(dir.path().join("note.txt")).unwrap(),
        "one\r\nTWO\r\ninserted\r\nthree\r\nfour\r\n"
    );
    let overlap = vec![
        PatchHunk {
            start_line: 1,
            old_lines: vec!["one".into()],
            new_lines: vec!["ONE".into()],
        },
        PatchHunk {
            start_line: 1,
            old_lines: vec!["one".into()],
            new_lines: vec!["again".into()],
        },
    ];
    assert_eq!(
        patch_text("one\n", &overlap).unwrap_err().code,
        FsErrorCode::InvalidInput
    );
    let stale = vec![PatchHunk {
        start_line: 1,
        old_lines: vec!["wrong".into()],
        new_lines: vec!["x".into()],
    }];
    assert_eq!(
        run(
            dir.path(),
            FsAction::Patch {
                path: "note.txt".into(),
                hunks: stale
            }
        )
        .unwrap_err()
        .code,
        FsErrorCode::Conflict
    );
}

#[test]
fn create_write_move_copy_and_delete() {
    let dir = root();
    run(
        dir.path(),
        FsAction::Mkdir {
            path: "child".into(),
        },
    )
    .unwrap();
    run(
        dir.path(),
        FsAction::Create {
            path: "child/a.txt".into(),
            content: "old".into(),
        },
    )
    .unwrap();
    assert_eq!(
        run(
            dir.path(),
            FsAction::Create {
                path: "child/a.txt".into(),
                content: "again".into()
            }
        )
        .unwrap_err()
        .code,
        FsErrorCode::AlreadyExists
    );
    assert_eq!(
        run(
            dir.path(),
            FsAction::Write {
                path: "child/a.txt".into(),
                content: "new".into(),
                expected_content: "stale".into()
            }
        )
        .unwrap_err()
        .code,
        FsErrorCode::Conflict
    );
    run(
        dir.path(),
        FsAction::Write {
            path: "child/a.txt".into(),
            content: "new".into(),
            expected_content: "old".into(),
        },
    )
    .unwrap();
    run(
        dir.path(),
        FsAction::Copy {
            from: "child/a.txt".into(),
            to: "child/b.txt".into(),
            recursive: false,
        },
    )
    .unwrap();
    assert_eq!(
        run(
            dir.path(),
            FsAction::Copy {
                from: "child/b.txt".into(),
                to: "child/a.txt".into(),
                recursive: false
            }
        )
        .unwrap_err()
        .code,
        FsErrorCode::AlreadyExists
    );
    run(
        dir.path(),
        FsAction::Move {
            from: "child/b.txt".into(),
            to: "child/c.txt".into(),
        },
    )
    .unwrap();
    assert_eq!(
        fs::read_to_string(dir.path().join("child/c.txt")).unwrap(),
        "new"
    );
    run(
        dir.path(),
        FsAction::Copy {
            from: "child".into(),
            to: "second".into(),
            recursive: true,
        },
    )
    .unwrap();
    assert_eq!(
        fs::read_to_string(dir.path().join("second/a.txt")).unwrap(),
        "new"
    );
    assert!(run(
        dir.path(),
        FsAction::Delete {
            path: "second".into(),
            recursive: false
        }
    )
    .is_err());
    run(
        dir.path(),
        FsAction::Delete {
            path: "second".into(),
            recursive: true,
        },
    )
    .unwrap();
    run(
        dir.path(),
        FsAction::Delete {
            path: "child/c.txt".into(),
            recursive: false,
        },
    )
    .unwrap();
    assert!(!dir.path().join("second").exists());
}

#[test]
fn search_modes_and_glob_honor_ignore_rules() {
    let dir = root();
    fs::write(dir.path().join(".gitignore"), "ignored.txt\n").unwrap();
    fs::create_dir(dir.path().join("src")).unwrap();
    fs::write(dir.path().join("src/main.rs"), "fn needle() {}\n").unwrap();
    fs::write(dir.path().join("ignored.txt"), "needle\n").unwrap();
    for mode in [SearchMode::Filename, SearchMode::Path, SearchMode::Content] {
        let query = if matches!(mode, SearchMode::Content) {
            "needle"
        } else {
            "main"
        };
        match run(
            dir.path(),
            FsAction::Search {
                query: query.into(),
                mode,
                limit: 10,
            },
        )
        .unwrap()
        {
            FsToolResult::Search { matches, .. } => {
                assert_eq!(matches.len(), 1);
                assert_eq!(matches[0].path, "src/main.rs");
            }
            _ => panic!("unexpected result"),
        }
    }
    match run(
        dir.path(),
        FsAction::Glob {
            pattern: "**/*.txt".into(),
            limit: 10,
        },
    )
    .unwrap()
    {
        FsToolResult::Glob { entries, .. } => assert!(entries.is_empty()),
        _ => panic!("unexpected result"),
    }
    match run(
        dir.path(),
        FsAction::Glob {
            pattern: "**/*.rs".into(),
            limit: 10,
        },
    )
    .unwrap()
    {
        FsToolResult::Glob { entries, .. } => assert_eq!(entries[0].path, "src/main.rs"),
        _ => panic!("unexpected result"),
    }
}

#[test]
fn binary_missing_and_metadata() {
    let dir = root();
    fs::write(dir.path().join("binary"), [0, 1, 2, 3]).unwrap();
    assert_eq!(
        run(
            dir.path(),
            FsAction::Read {
                path: "binary".into()
            }
        )
        .unwrap_err()
        .code,
        FsErrorCode::BinaryFile
    );
    assert_eq!(
        run(
            dir.path(),
            FsAction::Read {
                path: "missing".into()
            }
        )
        .unwrap_err()
        .code,
        FsErrorCode::NotFound
    );
    assert!(matches!(
        run(
            dir.path(),
            FsAction::Exists {
                path: "missing".into()
            }
        )
        .unwrap(),
        FsToolResult::Exists { exists: false }
    ));
    assert!(
        matches!(run(dir.path(), FsAction::Stat { path: "binary".into() }).unwrap(), FsToolResult::Stat { metadata } if metadata.size == 4 && metadata.kind == "file")
    );
}

#[cfg(unix)]
#[test]
fn reports_real_permission_denied() {
    use std::os::unix::fs::PermissionsExt;
    if unsafe { libc::geteuid() } == 0 {
        return;
    }
    let dir = root();
    let protected = dir.path().join("protected");
    fs::create_dir(&protected).unwrap();
    fs::write(protected.join("file"), "secret").unwrap();
    fs::set_permissions(&protected, fs::Permissions::from_mode(0o000)).unwrap();
    let result = run(
        dir.path(),
        FsAction::Read {
            path: "protected/file".into(),
        },
    );
    fs::set_permissions(&protected, fs::Permissions::from_mode(0o700)).unwrap();
    assert_eq!(result.unwrap_err().code, FsErrorCode::PermissionDenied);
}
#[test]
fn permission_errors_are_structured() {
    let error = FsError::from(io::Error::new(io::ErrorKind::PermissionDenied, "denied"));
    assert_eq!(error.code, FsErrorCode::PermissionDenied);
}

#[test]
fn ipc_contract_and_actor_risk() {
    let request: FsToolRequest = serde_json::from_value(serde_json::json!({"actor":"USER","action":{"operation":"read_range","path":"a","startByte":0,"length":10}})).unwrap();
    assert!(matches!(request.actor, ToolActor::User));
    assert_eq!(
        authorize(ToolActor::Agent).unwrap_err().code,
        FsErrorCode::PermissionDenied
    );
    assert!(matches!(
        request.action,
        FsAction::ReadRange { length: 10, .. }
    ));
    assert!(matches!(
        FsAction::Delete {
            path: "a".into(),
            recursive: true
        }
        .risk(),
        RiskLevel::Dangerous
    ));
}
