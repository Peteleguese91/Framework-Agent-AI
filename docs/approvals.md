# Agent approvals

| Mode | SAFE | MODERATE | DANGEROUS |
| --- | --- | --- | --- |
| ASK | No tool execution | No tool execution | No tool execution |
| AGENT | Automatic | Automatic inside workspace | Approve once |
| AUTONOMOUS | Automatic | Automatic | Approve once |

Reads and searches are safe. Workspace mutations, dependency changes, formatters, and unknown executables are moderate. Recursive deletion, outside-workspace paths, destructive Git, privilege/system commands, and opaque interpreters are dangerous.

Rejection becomes a failed tool observation. Persistent grants are deliberately unavailable. Rust independently enforces workspace confinement, valid agent mode, and explicit dangerous approval. Approved terminal commands retain the desktop user's privileges; this policy is not an operating-system sandbox.
