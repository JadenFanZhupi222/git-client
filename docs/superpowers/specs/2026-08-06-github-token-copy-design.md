# Token Input Copy Design

## Goal

Prevent ASCII ellipses from visually colliding with the trailing underscore or hyphen in
GitHub and GitLab token prefixes. Prefix examples belong in helper copy rather than input
placeholders.

## Copy

For an unconfigured GitHub credential:

- English placeholder: `Paste GitHub personal access token`
- Chinese placeholder: `粘贴 GitHub Personal Access Token`
- English helper: `Supports tokens beginning with github_pat_ or ghp_. Stored securely in the system credential store.`
- Chinese helper: `支持以 github_pat_ 或 ghp_ 开头的令牌。凭据将安全存储于系统凭据库中。`

For an unconfigured GitLab credential:

- English placeholder: `Paste GitLab personal access token`
- Chinese placeholder: `粘贴 GitLab Personal Access Token`
- English helper: `Supports tokens beginning with glpat-. Stored securely in the system credential store.`
- Chinese helper: `支持以 glpat- 开头的令牌。凭据将安全存储于系统凭据库中。`

Configured-state replacement placeholders remain unchanged because their purpose is to
explain replacement behavior. DeepSeek copy, credential handling, validation, and storage
behavior are out of scope.

## Verification

Update localized SettingsPanel tests to assert the GitHub and GitLab placeholders and helper
copy in English and Chinese. No `_...` or `-...` prefix example may remain in this settings
surface. Existing secret-handling and accessibility tests must remain green.
