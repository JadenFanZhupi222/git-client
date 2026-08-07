use review_agent::{
    map_patch_lines, validate_repository_path, ReviewError, ReviewFile, MAX_AUTO_FILES,
    MAX_ISSUE_PUBLISH_LABELS, MAX_ISSUE_REPLY_BYTES, MAX_PATCH_BYTES, MAX_READ_LINES,
};

#[test]
fn stable_error_codes_are_exposed() {
    assert_eq!(ReviewError::AiKeyMissing.code(), "AI_KEY_MISSING");
    assert_eq!(
        ReviewError::GithubTokenMissing.code(),
        "GITHUB_TOKEN_MISSING"
    );
    assert_eq!(ReviewError::AuthFailed.code(), "AUTH_FAILED");
    assert_eq!(ReviewError::RateLimited.code(), "RATE_LIMITED");
    assert_eq!(
        ReviewError::NetworkError("offline".into()).code(),
        "NETWORK_ERROR"
    );
    assert_eq!(ReviewError::PrUpdated.code(), "PR_UPDATED");
    assert_eq!(
        ReviewError::ReviewBudgetExceeded.code(),
        "REVIEW_BUDGET_EXCEEDED"
    );
    assert_eq!(
        ReviewError::InvalidModelOutput("bad".into()).code(),
        "INVALID_MODEL_OUTPUT"
    );
    assert_eq!(ReviewError::Cancelled.code(), "CANCELLED");
    assert_eq!(
        ReviewError::ReviewPublishFailed("nope".into()).code(),
        "REVIEW_PUBLISH_FAILED"
    );
    assert_eq!(ReviewError::IssueUpdated.code(), "ISSUE_UPDATED");
    assert_eq!(
        ReviewError::IssuePublishFailed("nope".into()).code(),
        "ISSUE_PUBLISH_FAILED"
    );
}

#[test]
fn limits_are_stable() {
    assert_eq!(MAX_AUTO_FILES, 30);
    assert_eq!(MAX_PATCH_BYTES, 200_000);
    assert_eq!(MAX_READ_LINES, 400);
    assert_eq!(MAX_ISSUE_PUBLISH_LABELS, 20);
    assert_eq!(MAX_ISSUE_REPLY_BYTES, 20_000);
}

#[test]
fn repository_paths_reject_absolute_and_traversal() {
    for path in [
        "../secret",
        "a/../../secret",
        "/etc/passwd",
        r"C:\secret",
        r"a\..\secret",
    ] {
        assert!(validate_repository_path(path).is_err(), "accepted {path}");
    }
    assert!(validate_repository_path("src/lib.rs").is_ok());
}

#[test]
fn patch_mapping_tracks_github_sides() {
    let patch = "@@ -10,2 +10,2 @@\n context\n-old\n+new\n";
    let lines = map_patch_lines(patch).unwrap();
    assert!(lines.contains(&("LEFT".into(), 11)));
    assert!(lines.contains(&("RIGHT".into(), 11)));
    assert!(lines.contains(&("LEFT".into(), 10)));
    assert!(lines.contains(&("RIGHT".into(), 10)));
}

#[test]
fn patch_mapping_treats_triple_marker_content_as_changed_lines() {
    let patch = "@@ -1 +1 @@\n---old-looking-content\n+++new-looking-content\n";
    let lines = map_patch_lines(patch).unwrap();
    assert_eq!(
        lines,
        [("LEFT".to_string(), 1), ("RIGHT".to_string(), 1)]
            .into_iter()
            .collect()
    );
}

#[test]
fn review_file_counts_utf8_patch_bytes() {
    let file = ReviewFile::from_patch("src/你好.rs", "@@ -1 +1 @@\n-a\n+你\n").unwrap();
    assert_eq!(file.patch_bytes, file.patch.as_ref().unwrap().len());
    assert!(file.reviewable);
}
