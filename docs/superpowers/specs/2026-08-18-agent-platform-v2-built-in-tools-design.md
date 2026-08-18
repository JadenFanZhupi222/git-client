# Agent Platform v2: Sandboxed Built-in Tools

**Status:** Implemented
**Date:** 2026-08-18
**Stage:** S4 of the Agent Platform v2 roadmap

## 1. Decision

S4 supplies the first production tool adapters behind the S3 registry, schema, policy, approval, cancellation, timeout, budget, sanitization, and event boundary. External capabilities live in a new `agent-tools` crate; `agent-runtime` remains provider-neutral and free of filesystem, process, and HTTP policy.

The built-in pack is instantiated per workspace capability. Possession of a `BuiltinToolConfig` grants access only to its canonical workspace root, artifact root, explicitly configured executables, and explicitly allowlisted web domains. No tool accepts an API key, arbitrary environment variables, request headers, absolute output paths, or an implicit shell command string.

## 2. Tool set

| Tool | Risk | Operation | Default policy |
|---|---|---|---|
| `filesystem.read` | read_only | Read a bounded UTF-8 file under the workspace root | allow |
| `filesystem.list` | read_only | List one bounded directory without following escaping symlinks | allow |
| `filesystem.write` | write | Atomically create or replace one bounded UTF-8 file | ask |
| `search.text` | read_only | Bounded literal/regex search under the workspace root | allow |
| `patch.apply` | write | Atomically replace exactly one expected text occurrence | ask |
| `shell.exec` | destructive | Execute one configured executable with an argv array | ask |
| `web.fetch` | external | Bounded read-only GET to an allowlisted public endpoint | ask |
| `artifact.write` | write | Write a bounded artifact and return an opaque ID | ask |

S4 deliberately omits delete, chmod, arbitrary environment mutation, shell pipelines, redirects, uploads, arbitrary HTTP headers, and multi-file patch transactions.

## 3. Capability roots and path safety

Workspace and artifact roots must exist and canonicalize during pack construction. Tool paths are relative and may contain only normal path components. Absolute paths, drive prefixes, `.`/`..`, empty file paths, NULs, and components traversing a symlink outside the configured root are rejected.

Reads canonicalize the final target and verify it remains under the canonical root. Writes walk existing components without following an escaping symlink, canonicalize the parent, create missing directories one level at a time under the root, and persist a same-directory temporary file. The final rename is atomic on supported local filesystems. A local attacker able to swap symlinks concurrently remains an OS-level TOCTOU risk; future hardening may use `openat`/handle-relative APIs per platform.

No `.git` internals are readable or writable through built-ins. Search also skips `.git`, `node_modules`, and `target` by default.

## 4. Filesystem and patch semantics

`filesystem.read` accepts `path` and an optional bounded `max_bytes`. It rejects directories, binary/non-UTF-8 content, and files exceeding the effective limit rather than returning a partial executable input.

`filesystem.list` returns sorted entries with relative path, kind, and byte size for files. It is non-recursive and bounded.

`filesystem.write` accepts `path`, `content`, and `create_only`. It never appends. Existing files are replaced atomically only after approval; `create_only` fails if the target already exists.

`patch.apply` is a structured patch, not a shell-out to `patch` or `git apply`. It requires non-empty `expected` text to occur exactly once in the current UTF-8 file, replaces it with `replacement`, rechecks the size cap, and atomically persists the result. Missing or repeated preimages fail closed, preventing stale or ambiguous edits.

## 5. Search semantics

`search.text` supports literal or Rust-regex matching, case sensitivity, a relative subtree, and a bounded result count. It skips symlinks, ignored heavy directories, binary/non-UTF-8 files, and files over the per-file scan ceiling. Each result contains only relative path, one-based line/column, and a bounded preview. Cancellation is checked during traversal and line scanning.

## 6. Process semantics

`shell.exec` is named for user familiarity but never invokes `cmd.exe`, PowerShell, `/bin/sh`, or `sh -c`. The model supplies a configured program alias plus an argv string array, optional relative cwd, and bounded stdin. The application maps aliases to canonical executable paths before registration.

The child receives no model-provided or inherited host environment; S4 clears the environment and exposes no environment configuration surface. Stdout/stderr are streamed into a combined byte ceiling. Overflow kills the child. The process uses kill-on-drop so S3 cancellation or timeout terminates it. The result contains exit code plus bounded stdout/stderr; non-zero exit is data, while spawn/IO failures are typed handler failures.

Executable allowlisting is not an OS sandbox: a configured program may interpret otherwise literal arguments as paths or external actions. The production pack therefore defaults to no shell programs, every shell call remains `ask`, and product hosts must add program-specific argument policy or an OS sandbox before enabling broadly capable executables.

## 7. Web semantics

`web.fetch` accepts only a URL. Production defaults require HTTPS, reject embedded credentials, disallow redirects, allow only exact configured domains or explicit subdomain rules, and reject unresolved hosts plus loopback, private, link-local, multicast, unspecified, documentation, and other non-public IP ranges.

DNS is resolved before the request, every address is checked, and the validated addresses are pinned into the request client to reduce DNS-rebinding exposure. No cookies, authorization, user headers, request body, proxy configuration, or response headers are exposed. Only status, final URL, content type, and a bounded UTF-8 body enter the sanitized result. Tests may explicitly enable loopback HTTP; production configuration cannot inherit that setting accidentally from model input.

## 8. Artifact semantics

`artifact.write` accepts a display name, media type, and bounded UTF-8 content. It stores data only in the configured artifact root under an opaque run/call-derived ID and returns ID, display name, media type, and byte count. It never returns an absolute path. Artifact lookup/rendering is reserved for a later product surface.

## 9. Registration and policy

`build_builtin_tool_pack` validates all roots, executable mappings, domain rules, and size ceilings, registers the eight definitions, and returns the registry plus an ordered policy:

1. allow exact `filesystem.read`, `filesystem.list`, and `search.text`;
2. ask exact `filesystem.write`, `patch.apply`, `artifact.write`, `shell.exec`, and `web.fetch`;
3. default deny everything else.

Definitions expose only provider-visible descriptions and JSON Schemas plus S3 risk/timeout/result metadata. Application and run policies may further restrict this pack but cannot broaden it.

## 10. Security invariants

- Tool arguments reach handlers only after complete provider termination, JSON parsing, schema validation, budget reservation, and permission approval.
- No API key, prompt, raw provider body, inherited secret environment, raw HTTP headers, absolute host path, or unsanitized result is emitted to frontend events or logs.
- Filesystem and artifacts cannot escape their configured roots.
- Shell metacharacters are ordinary argv characters and are never parsed by a shell.
- Web redirects and private-network access are disabled in production.
- Results remain observational/transcript inputs; workflow structured output remains authoritative.

## 11. Acceptance criteria

S4 is accepted when:

1. The new crate depends inward on `agent-runtime`; no runtime or core crate depends on it.
2. Pack construction rejects missing roots, bad executable aliases/paths, invalid domain rules, and unsafe limits.
3. Path traversal, absolute paths, `.git`, escaping symlinks, oversized/binary reads, and ambiguous patches are rejected before mutation.
4. Writes and patches are approval-gated by the returned policy and are atomically persisted in tests.
5. Search proves bounds, ignore rules, regex/literal modes, cancellation, and no symlink traversal.
6. Shell proves argv is not shell-parsed, cwd remains in root, environment is cleared, output is bounded, and timeout/cancellation kills the child.
7. Web proves scheme/domain/credential/redirect/private-IP denial, pinned resolution, bounded UTF-8 output, and explicit test-only loopback enablement.
8. Artifact results contain no absolute path and stored bytes remain under the artifact root.
9. Registry definitions and policy decisions are contract-tested for all eight tools.
10. Workspace tests, Clippy with warnings denied, rustfmt, dependency boundaries, frontend tests, and production build pass.

## 12. Test matrix

| Area | Cases |
|---|---|
| Pack | valid construction, stable names/risks, exact allow/ask policy, duplicate-free definitions |
| Paths | traversal, absolute/prefix, empty, NUL, `.git`, internal symlink, escaping symlink, missing parent |
| Read/list | UTF-8, binary, size cap, sorted bounded entries, directories |
| Write | create-only, replace, nested parent, cap, atomic persistence, no outside write |
| Patch | exact once, absent, repeated, stale, expansion cap, Unicode |
| Search | literal, regex, case mode, result cap, file cap, ignores, cancellation, symlink |
| Shell | alias allowlist, argv literal metacharacters, cwd, cleared env, exit status, output overflow, timeout/drop |
| Web | HTTPS default, domain matching, embedded credentials, redirect denial, public/private IP, response cap, invalid UTF-8 |
| Artifact | opaque ID, metadata, cap, separate root, no path disclosure |
| Security | no secret-bearing definition fields; errors/events contain stable categories only |
| Regression | S1-S3 provider/event/approval contracts and full repository verification |

## 13. Follow-on

S5 can compose these tools into session-aware agent loops and context compaction. Before enabling broad mutation in product workflows, add per-workspace policy UI, durable approval audit metadata, platform-specific handle-relative filesystem hardening, and transactional multi-file patch support.
