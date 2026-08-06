# GitHub PR Review Agent Design

## Goal

Add a high-precision, user-approved GitHub pull-request review workflow without giving
the model local execution, file mutation, or direct publishing authority.

## Runtime

`crates/review-agent` owns the provider loop, GitHub review source, validation, budgets,
cancellation, and sanitized tracing. The stateless DeepSeek Responses integration sends
the complete message/function-call history each round and accepts only the two declared
read-only tools. The loop is capped at eight rounds and twenty tool calls.

Repository data is pinned to the PR head SHA. Preflight refuses silent truncation beyond
30 files or 200,000 UTF-8 patch bytes. Each file read is capped at 400 lines, and aggregate
tool output is capped at 300,000 bytes. Paths are normalized and checked against traversal.

## Application flow

The PR detail panel opens a modal workspace. After preflight, the first run requires a
versioned disclosure acknowledgement. Oversized reviews require explicit file selection.
Progress events are filtered by run ID, and cancellation propagates to the Rust runtime.

The result contains an authoritative summary, reviewed-file list, usage, and validated
line findings. Users may edit and select findings before submitting them once as a GitHub
`COMMENT` review. The backend rechecks the head SHA before any write and never degrades an
invalid line comment into a general pull-request comment.

## Credentials and privacy

The unified Settings panel manages DeepSeek, GitHub, and GitLab credentials. It exposes
only configured status, never secret values. Existing GitHub/GitLab keyring identifiers
remain compatible. DeepSeek's endpoint and model are fixed and read-only.

Local traces contain only timing, model, token usage, tool names/counts, status, and stable
error codes. They exclude keys, prompts, diffs, file content, full outputs, and reasoning.

## Out of scope

GitLab review, issue triage, local repository access, command execution, code changes,
automatic approval/request-changes, and merge automation remain outside this milestone.
