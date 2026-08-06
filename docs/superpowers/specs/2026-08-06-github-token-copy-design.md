# GitHub Token Input Copy Design

## Goal

Clarify that underscores in `github_pat_` and `ghp_` are literal parts of recognized
GitHub token prefixes, not characters users must add separately.

## Copy

For an unconfigured GitHub credential:

- English placeholder: `Paste GitHub personal access token`
- Chinese placeholder: `粘贴 GitHub Personal Access Token`
- English helper: `Supports tokens beginning with github_pat_ or ghp_. Stored securely in the system credential store.`
- Chinese helper: `支持以 github_pat_ 或 ghp_ 开头的令牌。凭据将安全存储于系统凭据库中。`

The configured-state replacement placeholder remains unchanged because its purpose is to
explain replacement behavior. DeepSeek and GitLab copy, credential handling, validation,
and storage behavior are out of scope.

## Verification

Update the localized SettingsPanel tests to assert the new placeholder and helper copy in
English and Chinese. Existing secret-handling and accessibility tests must remain green.
