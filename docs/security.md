# Security

The default trust boundary is the opened workspace. Canonical path validation prevents traversal and symlink escapes. External filesystem access, privilege escalation, system configuration, destructive Git operations, and hazardous network actions require grants or explicit approval.

Tool inputs and outputs are validated, secrets are redacted from logs, and provider credentials use operating-system secret storage. Ask, Agent, and Autonomous modes alter approval defaults but never bypass mandatory dangerous-action approval.
