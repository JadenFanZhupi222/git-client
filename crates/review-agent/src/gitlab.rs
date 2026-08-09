use crate::*;
use async_trait::async_trait;
use reqwest::{Client, Response, StatusCode, Url};
use serde_json::{json, Value};
use std::time::Duration;

const GITLAB_API_BASE: &str = "https://gitlab.com/api/v4";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);
const PAGE_SIZE: u32 = 100;
const MAX_PAGES: u32 = 100;

pub struct GitlabReviewSource {
    client: Client,
    token: String,
    base_url: String,
}

#[derive(Debug, Clone)]
struct GitlabDiff {
    old_path: String,
    new_path: String,
    patch: Option<String>,
}

#[derive(Debug, Clone)]
struct GitlabDiffVersion {
    base_sha: String,
    start_sha: String,
    head_sha: String,
}

impl GitlabReviewSource {
    pub fn new(token: impl Into<String>) -> Result<Self, ReviewError> {
        let token = token.into();
        if token.trim().is_empty() {
            return Err(ReviewError::AuthFailed);
        }
        let client = Client::builder()
            .user_agent("versionarc-review-agent")
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(REQUEST_TIMEOUT)
            .build()
            .map_err(|_| ReviewError::NetworkError("could not initialize HTTP client".into()))?;
        Ok(Self {
            client,
            token,
            base_url: GITLAB_API_BASE.into(),
        })
    }

    #[cfg(test)]
    fn new_with_base_for_test(token: impl Into<String>, base_url: String) -> Self {
        Self {
            client: Client::builder()
                .connect_timeout(Duration::from_millis(50))
                .timeout(Duration::from_millis(100))
                .build()
                .expect("test HTTP client should build"),
            token: token.into(),
            base_url,
        }
    }

    fn project_endpoint<'segment>(
        &self,
        target: &ReviewTarget,
        suffix_segments: impl IntoIterator<Item = &'segment str>,
    ) -> Result<Url, ReviewError> {
        validate_repository_path(&target.owner)?;
        validate_repository_path(&target.repo)?;
        let project = format!("{}/{}", target.owner, target.repo);
        let mut url = Url::parse(&self.base_url)
            .map_err(|_| ReviewError::NetworkError("invalid GitLab API endpoint".into()))?;
        let mut segments = url
            .path_segments_mut()
            .map_err(|_| ReviewError::NetworkError("invalid GitLab API endpoint".into()))?;
        segments.pop_if_empty().push("projects").push(&project);
        for segment in suffix_segments {
            segments.push(segment);
        }
        drop(segments);
        Ok(url)
    }

    fn request(&self, method: reqwest::Method, url: Url) -> reqwest::RequestBuilder {
        self.client
            .request(method, url)
            .header("PRIVATE-TOKEN", &self.token)
    }

    async fn merge_request(&self, target: &ReviewTarget) -> Result<Value, ReviewError> {
        let iid = target.pull_number.to_string();
        let response = self
            .request(
                reqwest::Method::GET,
                self.project_endpoint(target, ["merge_requests", iid.as_str()])?,
            )
            .send()
            .await
            .map_err(network_error)?;
        checked_json(response, false).await
    }

    async fn diffs(&self, target: &ReviewTarget) -> Result<Vec<GitlabDiff>, ReviewError> {
        let iid = target.pull_number.to_string();
        let mut result = Vec::new();
        for page in 1..=MAX_PAGES {
            let response = self
                .request(
                    reqwest::Method::GET,
                    self.project_endpoint(target, ["merge_requests", iid.as_str(), "diffs"])?,
                )
                .query(&[("per_page", PAGE_SIZE), ("page", page)])
                .send()
                .await
                .map_err(network_error)?;
            let body = checked_json(response, false).await?;
            let entries = body.as_array().ok_or_else(|| {
                ReviewError::InvalidModelOutput("GitLab diffs response was invalid".into())
            })?;
            for entry in entries {
                let old_path = required_path(entry, "old_path")?;
                let new_path = required_path(entry, "new_path")?;
                let unavailable = entry.get("collapsed").and_then(Value::as_bool) == Some(true)
                    || entry.get("too_large").and_then(Value::as_bool) == Some(true);
                result.push(GitlabDiff {
                    old_path,
                    new_path,
                    patch: (!unavailable)
                        .then(|| entry.get("diff").and_then(Value::as_str).map(str::to_owned))
                        .flatten(),
                });
            }
            if entries.len() < PAGE_SIZE as usize {
                return Ok(result);
            }
        }
        Err(ReviewError::ReviewBudgetExceeded)
    }

    async fn latest_version(
        &self,
        target: &ReviewTarget,
        expected_head_sha: &str,
    ) -> Result<GitlabDiffVersion, ReviewError> {
        let iid = target.pull_number.to_string();
        let response = self
            .request(
                reqwest::Method::GET,
                self.project_endpoint(target, ["merge_requests", iid.as_str(), "versions"])?,
            )
            .send()
            .await
            .map_err(network_error)?;
        let body = checked_json(response, false).await?;
        body.as_array()
            .and_then(|versions| {
                versions.iter().find(|version| {
                    version.get("head_commit_sha").and_then(Value::as_str)
                        == Some(expected_head_sha)
                })
            })
            .map(|version| {
                Ok(GitlabDiffVersion {
                    base_sha: required_sha(version, "base_commit_sha")?,
                    start_sha: required_sha(version, "start_commit_sha")?,
                    head_sha: required_sha(version, "head_commit_sha")?,
                })
            })
            .transpose()?
            .ok_or(ReviewError::PrUpdated)
    }
}

#[async_trait]
impl ReviewSource for GitlabReviewSource {
    async fn head_sha(&self, target: &ReviewTarget) -> Result<String, ReviewError> {
        let body = self.merge_request(target).await?;
        required_sha(&body, "sha")
    }

    async fn pull_files_at_head(
        &self,
        target: &ReviewTarget,
        expected_head_sha: &str,
    ) -> Result<Vec<ReviewFile>, ReviewError> {
        validate_sha(expected_head_sha)?;
        let files = self
            .diffs(target)
            .await?
            .into_iter()
            .map(|diff| {
                let path = diff.new_path;
                let patch_bytes = diff.patch.as_ref().map_or(0, String::len);
                ReviewFile {
                    path,
                    reviewable: diff.patch.is_some() && patch_bytes <= MAX_PATCH_BYTES,
                    patch: diff.patch,
                    patch_bytes,
                }
            })
            .collect();
        if self.head_sha(target).await? != expected_head_sha {
            return Err(ReviewError::PrUpdated);
        }
        Ok(files)
    }

    async fn list_repository_tree(
        &self,
        target: &ReviewTarget,
        head_sha: &str,
        prefix: Option<&str>,
    ) -> Result<Vec<String>, ReviewError> {
        validate_sha(head_sha)?;
        if let Some(prefix) = prefix {
            if !prefix.is_empty() {
                validate_repository_path(prefix)?;
            }
        }
        let mut paths = Vec::new();
        for page in 1..=MAX_PAGES {
            let response = self
                .request(
                    reqwest::Method::GET,
                    self.project_endpoint(target, ["repository", "tree"])?,
                )
                .query(&[
                    ("ref", head_sha.to_owned()),
                    ("recursive", "true".to_owned()),
                    ("per_page", PAGE_SIZE.to_string()),
                    ("page", page.to_string()),
                ])
                .send()
                .await
                .map_err(network_error)?;
            let body = checked_json(response, false).await?;
            let entries = body.as_array().ok_or_else(|| {
                ReviewError::InvalidModelOutput(
                    "GitLab repository tree response was invalid".into(),
                )
            })?;
            for item in entries {
                if item.get("type").and_then(Value::as_str) != Some("blob") {
                    continue;
                }
                if let Some(path) = item.get("path").and_then(Value::as_str) {
                    validate_repository_path(path)?;
                    if prefix.is_none_or(|value| path.starts_with(value)) {
                        paths.push(path.to_owned());
                    }
                }
            }
            if entries.len() < PAGE_SIZE as usize {
                return Ok(paths);
            }
        }
        Err(ReviewError::ReviewBudgetExceeded)
    }

    async fn read_file(
        &self,
        target: &ReviewTarget,
        head_sha: &str,
        path: &str,
        start_line: u32,
        end_line: u32,
    ) -> Result<String, ReviewError> {
        validate_sha(head_sha)?;
        validate_repository_path(path)?;
        if start_line == 0 || end_line < start_line || end_line - start_line + 1 > MAX_READ_LINES {
            return Err(ReviewError::ReviewBudgetExceeded);
        }
        let response = self
            .request(
                reqwest::Method::GET,
                self.project_endpoint(target, ["repository", "files", path, "raw"])?,
            )
            .query(&[("ref", head_sha)])
            .send()
            .await
            .map_err(network_error)?;
        let response = checked_response(response, false).await?;
        let bytes = response
            .bytes()
            .await
            .map_err(|_| ReviewError::NetworkError("GitLab file response failed".into()))?;
        let text = String::from_utf8(bytes.to_vec()).map_err(|_| {
            ReviewError::InvalidModelOutput("binary or non-UTF-8 file rejected".into())
        })?;
        Ok(text
            .lines()
            .skip((start_line - 1) as usize)
            .take((end_line - start_line + 1) as usize)
            .collect::<Vec<_>>()
            .join("\n"))
    }

    async fn publish(&self, review: &SubmitReview) -> Result<PublishedReview, ReviewError> {
        validate_sha(&review.head_sha)?;
        if review.findings.is_empty() {
            return Err(ReviewError::ReviewPublishFailed(
                "no confirmed findings to publish".into(),
            ));
        }
        if self.head_sha(&review.target).await? != review.head_sha {
            return Err(ReviewError::PrUpdated);
        }
        let version = self
            .latest_version(&review.target, &review.head_sha)
            .await?;
        let diffs = self.diffs(&review.target).await?;
        if self.head_sha(&review.target).await? != review.head_sha {
            return Err(ReviewError::PrUpdated);
        }
        let iid = review.target.pull_number.to_string();
        let mut last_note_id = None;
        for (published, finding) in review.findings.iter().enumerate() {
            let diff = diffs
                .iter()
                .find(|diff| diff.new_path == finding.path || diff.old_path == finding.path)
                .ok_or_else(|| {
                    ReviewError::ReviewPublishFailed(format!(
                        "GitLab diff no longer contains {}",
                        finding.path
                    ))
                })?;
            let mut position = json!({
                "position_type": "text",
                "base_sha": version.base_sha,
                "start_sha": version.start_sha,
                "head_sha": version.head_sha,
                "old_path": diff.old_path,
                "new_path": diff.new_path,
            });
            let line_key = match finding.side {
                ReviewSide::LEFT => "old_line",
                ReviewSide::RIGHT => "new_line",
            };
            position[line_key] = json!(finding.line);
            let response = self
                .request(
                    reqwest::Method::POST,
                    self.project_endpoint(
                        &review.target,
                        ["merge_requests", iid.as_str(), "discussions"],
                    )?,
                )
                .json(&json!({"body": finding.draft_comment, "position": position}))
                .send()
                .await
                .map_err(network_error)?;
            let body = match checked_json(response, true).await {
                Ok(body) => body,
                Err(error) if published > 0 => {
                    return Err(ReviewError::ReviewPublishFailed(format!(
                        "GitLab published {published} discussion(s) before the remaining publication failed: {error}"
                    )))
                }
                Err(error) => return Err(error),
            };
            last_note_id = body
                .pointer("/notes/0/id")
                .and_then(Value::as_u64)
                .or_else(|| body.get("id").and_then(Value::as_u64));
        }

        let review_id = last_note_id.ok_or_else(|| {
            ReviewError::ReviewPublishFailed("GitLab response omitted note id".into())
        })?;
        Ok(PublishedReview {
            review_id,
            html_url: Some(format!(
                "https://gitlab.com/{}/{}/-/merge_requests/{}#note_{}",
                review.target.owner, review.target.repo, review.target.pull_number, review_id
            )),
        })
    }
}

fn required_path(value: &Value, key: &str) -> Result<String, ReviewError> {
    let path = value
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| ReviewError::InvalidModelOutput(format!("GitLab {key} missing")))?
        .to_owned();
    validate_repository_path(&path)?;
    Ok(path)
}

fn required_sha(value: &Value, key: &str) -> Result<String, ReviewError> {
    let sha = value
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| ReviewError::InvalidModelOutput(format!("GitLab {key} missing")))?
        .to_owned();
    validate_sha(&sha)?;
    Ok(sha)
}

fn validate_sha(sha: &str) -> Result<(), ReviewError> {
    if sha.is_empty() || !sha.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Err(ReviewError::InvalidModelOutput("invalid commit SHA".into()))
    } else {
        Ok(())
    }
}

fn network_error(_: reqwest::Error) -> ReviewError {
    ReviewError::NetworkError("request failed".into())
}

async fn checked_response(response: Response, publish: bool) -> Result<Response, ReviewError> {
    match response.status() {
        StatusCode::UNAUTHORIZED => Err(ReviewError::AuthFailed),
        StatusCode::FORBIDDEN
            if response
                .headers()
                .contains_key(reqwest::header::RETRY_AFTER)
                || response
                    .headers()
                    .get("ratelimit-remaining")
                    .and_then(|value| value.to_str().ok())
                    == Some("0") =>
        {
            Err(ReviewError::RateLimited)
        }
        StatusCode::FORBIDDEN => Err(ReviewError::AuthFailed),
        StatusCode::TOO_MANY_REQUESTS => Err(ReviewError::RateLimited),
        status if !status.is_success() => Err(if publish {
            ReviewError::ReviewPublishFailed("GitLab rejected review discussion".into())
        } else {
            ReviewError::NetworkError("GitLab request failed".into())
        }),
        _ => Ok(response),
    }
}

async fn checked_json(response: Response, publish: bool) -> Result<Value, ReviewError> {
    checked_response(response, publish)
        .await?
        .json()
        .await
        .map_err(|_| {
            if publish {
                ReviewError::ReviewPublishFailed("GitLab response was invalid".into())
            } else {
                ReviewError::InvalidModelOutput("GitLab response was invalid".into())
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    use wiremock::matchers::{body_partial_json, header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn target() -> ReviewTarget {
        ReviewTarget {
            owner: "group".into(),
            repo: "project".into(),
            pull_number: 7,
        }
    }

    fn source(server: &MockServer) -> GitlabReviewSource {
        GitlabReviewSource::new_with_base_for_test("token", server.uri())
    }

    async fn mock_head(server: &MockServer, sha: &str) {
        Mock::given(method("GET"))
            .and(path("/projects/group%2Fproject/merge_requests/7"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"sha": sha})))
            .mount(server)
            .await;
    }

    #[tokio::test]
    async fn loads_current_diffs_and_marks_unavailable_patches_unreviewable() {
        let server = MockServer::start().await;
        mock_head(&server, "abc").await;
        Mock::given(method("GET"))
            .and(path("/projects/group%2Fproject/merge_requests/7/diffs"))
            .and(query_param("per_page", "100"))
            .and(query_param("page", "1"))
            .and(header("private-token", "token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([
                {"old_path":"src/a.rs","new_path":"src/a.rs","diff":"@@ -1 +1 @@\n-a\n+b"},
                {"old_path":"big.bin","new_path":"big.bin","diff":"ignored","too_large":true}
            ])))
            .mount(&server)
            .await;
        let files = source(&server)
            .pull_files_at_head(&target(), "abc")
            .await
            .unwrap();
        assert_eq!(files.len(), 2);
        assert!(files[0].reviewable);
        assert!(!files[1].reviewable);
        assert!(files[1].patch.is_none());
    }

    #[tokio::test]
    async fn reads_raw_file_at_exact_sha_and_range() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(
                "/projects/group%2Fproject/repository/files/src%2Flib.rs/raw",
            ))
            .and(query_param("ref", "abc"))
            .respond_with(ResponseTemplate::new(200).set_body_string("one\ntwo\nthree"))
            .mount(&server)
            .await;
        assert_eq!(
            source(&server)
                .read_file(&target(), "abc", "src/lib.rs", 2, 3)
                .await
                .unwrap(),
            "two\nthree"
        );
    }

    #[tokio::test]
    async fn rejects_changed_head_after_loading_diffs() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/projects/group%2Fproject/merge_requests/7/diffs"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
            .mount(&server)
            .await;
        mock_head(&server, "def").await;
        assert_eq!(
            source(&server)
                .pull_files_at_head(&target(), "abc")
                .await
                .unwrap_err(),
            ReviewError::PrUpdated
        );
    }

    #[tokio::test]
    async fn publishes_right_side_finding_as_gitlab_diff_discussion() {
        let server = MockServer::start().await;
        mock_head(&server, "abc").await;
        Mock::given(method("GET"))
            .and(path("/projects/group%2Fproject/merge_requests/7/versions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([{
                "base_commit_sha":"aaa","start_commit_sha":"bbb","head_commit_sha":"abc"
            }])))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/projects/group%2Fproject/merge_requests/7/diffs"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([{
                "old_path":"old.rs","new_path":"new.rs","diff":"@@ -1 +1 @@\n-a\n+b"
            }])))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path(
                "/projects/group%2Fproject/merge_requests/7/discussions",
            ))
            .and(body_partial_json(json!({
                "body":"Please fix this",
                "position":{
                    "position_type":"text","base_sha":"aaa","start_sha":"bbb","head_sha":"abc",
                    "old_path":"old.rs","new_path":"new.rs","new_line":1
                }
            })))
            .respond_with(ResponseTemplate::new(201).set_body_json(json!({"notes":[{"id":42}]})))
            .mount(&server)
            .await;
        let published = source(&server)
            .publish(&SubmitReview {
                target: target(),
                head_sha: "abc".into(),
                findings: vec![ReviewFinding {
                    id: "f1".into(),
                    severity: Severity::High,
                    path: "new.rs".into(),
                    side: ReviewSide::RIGHT,
                    line: 1,
                    title: "Risk".into(),
                    failure_scenario: "Fails".into(),
                    explanation: "Because".into(),
                    draft_comment: "Please fix this".into(),
                }],
            })
            .await
            .unwrap();
        assert_eq!(published.review_id, 42);
        assert!(published.html_url.unwrap().ends_with("#note_42"));
    }

    #[tokio::test]
    async fn reports_partial_publication_without_automatic_duplicate_retry() {
        let server = MockServer::start().await;
        mock_head(&server, "abc").await;
        Mock::given(method("GET"))
            .and(path("/projects/group%2Fproject/merge_requests/7/versions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([{
                "base_commit_sha":"aaa","start_commit_sha":"bbb","head_commit_sha":"abc"
            }])))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/projects/group%2Fproject/merge_requests/7/diffs"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([{
                "old_path":"src/a.rs","new_path":"src/a.rs","diff":"@@ -1,2 +1,2 @@\n-a\n-b\n+c\n+d"
            }])))
            .mount(&server)
            .await;
        let calls = Arc::new(AtomicUsize::new(0));
        let response_calls = calls.clone();
        Mock::given(method("POST"))
            .and(path(
                "/projects/group%2Fproject/merge_requests/7/discussions",
            ))
            .respond_with(move |_: &wiremock::Request| {
                if response_calls.fetch_add(1, Ordering::SeqCst) == 0 {
                    ResponseTemplate::new(201).set_body_json(json!({"notes":[{"id":41}]}))
                } else {
                    ResponseTemplate::new(500)
                }
            })
            .mount(&server)
            .await;
        let finding = |id: &str, line| ReviewFinding {
            id: id.into(),
            severity: Severity::High,
            path: "src/a.rs".into(),
            side: ReviewSide::RIGHT,
            line,
            title: "Risk".into(),
            failure_scenario: "Fails".into(),
            explanation: "Because".into(),
            draft_comment: format!("Comment {id}"),
        };
        let error = source(&server)
            .publish(&SubmitReview {
                target: target(),
                head_sha: "abc".into(),
                findings: vec![finding("one", 1), finding("two", 2)],
            })
            .await
            .unwrap_err();

        assert!(matches!(
            error,
            ReviewError::ReviewPublishFailed(message)
                if message.contains("published 1 discussion")
        ));
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn maps_rate_limit_without_leaking_response_body() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(429).set_body_string("secret"))
            .mount(&server)
            .await;
        assert_eq!(
            source(&server).head_sha(&target()).await.unwrap_err(),
            ReviewError::RateLimited
        );
    }
}
