use crate::*;
use async_trait::async_trait;
use base64::Engine;
use reqwest::{Client, Response, StatusCode, Url};
use serde_json::{json, Value};

const GITHUB_API_BASE: &str = "https://api.github.com";

pub struct GithubReviewSource {
    client: Client,
    token: String,
    base_url: String,
}

impl GithubReviewSource {
    pub fn new(token: impl Into<String>) -> Result<Self, ReviewError> {
        let token = token.into();
        if token.trim().is_empty() {
            return Err(ReviewError::GithubTokenMissing);
        }
        let client = Client::builder()
            .user_agent("git-client-review-agent")
            .build()
            .map_err(|_| ReviewError::NetworkError("could not initialize HTTP client".into()))?;
        Ok(Self {
            client,
            token,
            base_url: GITHUB_API_BASE.into(),
        })
    }

    #[cfg(test)]
    fn new_with_base_for_test(token: impl Into<String>, base_url: String) -> Self {
        Self {
            client: Client::new(),
            token: token.into(),
            base_url,
        }
    }

    pub async fn preflight(&self, target: &ReviewTarget) -> Result<ReviewPreflight, ReviewError> {
        let head_sha = self.head_sha(target).await?;
        let files = self.pull_files_at_head(target, &head_sha).await?;
        let reviewable: Vec<_> = files.iter().filter(|f| f.reviewable).collect();
        let total_patch_bytes = reviewable.iter().map(|f| f.patch_bytes).sum();
        let requires_selection =
            reviewable.len() > MAX_AUTO_FILES || total_patch_bytes > MAX_PATCH_BYTES;
        Ok(ReviewPreflight {
            head_sha,
            files,
            total_patch_bytes,
            requires_selection,
        })
    }

    fn endpoint<'segment>(
        &self,
        target: &ReviewTarget,
        suffix_segments: impl IntoIterator<Item = &'segment str>,
    ) -> Result<Url, ReviewError> {
        let mut url = Url::parse(&self.base_url)
            .map_err(|_| ReviewError::NetworkError("invalid GitHub API endpoint".into()))?;
        let mut segments = url
            .path_segments_mut()
            .map_err(|_| ReviewError::NetworkError("invalid GitHub API endpoint".into()))?;
        segments.pop_if_empty();
        segments
            .push("repos")
            .push(&target.owner)
            .push(&target.repo);
        for segment in suffix_segments {
            segments.push(segment);
        }
        drop(segments);
        Ok(url)
    }

    fn request(
        &self,
        method: reqwest::Method,
        url: impl reqwest::IntoUrl,
    ) -> reqwest::RequestBuilder {
        self.client
            .request(method, url)
            .bearer_auth(&self.token)
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
    }
}

#[async_trait]
impl ReviewSource for GithubReviewSource {
    async fn head_sha(&self, target: &ReviewTarget) -> Result<String, ReviewError> {
        validate_repository_path(&target.owner)?;
        validate_repository_path(&target.repo)?;
        let pull_number = target.pull_number.to_string();
        let response = self
            .request(
                reqwest::Method::GET,
                self.endpoint(target, ["pulls", pull_number.as_str()])?,
            )
            .send()
            .await
            .map_err(network_error)?;
        let body = checked_json(response, false).await?;
        body.pointer("/head/sha")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| {
                ReviewError::InvalidModelOutput("GitHub response omitted head SHA".into())
            })
    }

    async fn pull_files_at_head(
        &self,
        target: &ReviewTarget,
        expected_head_sha: &str,
    ) -> Result<Vec<ReviewFile>, ReviewError> {
        validate_sha(expected_head_sha)?;
        let mut result = Vec::new();
        let pull_number = target.pull_number.to_string();
        for page in 1..=100u32 {
            let response = self
                .request(
                    reqwest::Method::GET,
                    self.endpoint(target, ["pulls", pull_number.as_str(), "files"])?,
                )
                .query(&[("per_page", 100u32), ("page", page)])
                .send()
                .await
                .map_err(network_error)?;
            let body = checked_json(response, false).await?;
            let entries = body.as_array().ok_or_else(|| {
                ReviewError::InvalidModelOutput("GitHub files response was invalid".into())
            })?;
            for entry in entries {
                let path = entry
                    .get("filename")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        ReviewError::InvalidModelOutput("GitHub file path missing".into())
                    })?
                    .to_owned();
                validate_repository_path(&path)?;
                let patch = entry
                    .get("patch")
                    .and_then(Value::as_str)
                    .map(str::to_owned);
                let patch_bytes = patch.as_ref().map_or(0, |text| text.len());
                result.push(ReviewFile {
                    path,
                    reviewable: patch.is_some() && patch_bytes <= MAX_PATCH_BYTES,
                    patch,
                    patch_bytes,
                });
            }
            if entries.len() < 100 {
                break;
            }
        }
        if self.head_sha(target).await? != expected_head_sha {
            return Err(ReviewError::PrUpdated);
        }
        Ok(result)
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
        let response = self
            .request(
                reqwest::Method::GET,
                self.endpoint(target, ["git", "trees", head_sha])?,
            )
            .query(&[("recursive", "1")])
            .send()
            .await
            .map_err(network_error)?;
        let body = checked_json(response, false).await?;
        let tree = body.get("tree").and_then(Value::as_array).ok_or_else(|| {
            ReviewError::InvalidModelOutput("GitHub tree response was invalid".into())
        })?;
        let mut paths = Vec::new();
        for item in tree {
            if item.get("type").and_then(Value::as_str) != Some("blob") {
                continue;
            }
            if let Some(path) = item.get("path").and_then(Value::as_str) {
                if prefix.is_none_or(|p| path.starts_with(p)) {
                    paths.push(path.to_owned());
                }
            }
        }
        Ok(paths)
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
                self.endpoint(target, std::iter::once("contents").chain(path.split('/')))?,
            )
            .query(&[("ref", head_sha)])
            .send()
            .await
            .map_err(network_error)?;
        let body = checked_json(response, false).await?;
        if body.get("encoding").and_then(Value::as_str) != Some("base64") {
            return Err(ReviewError::InvalidModelOutput(
                "file is not base64 encoded".into(),
            ));
        }
        let encoded = body
            .get("content")
            .and_then(Value::as_str)
            .ok_or_else(|| ReviewError::InvalidModelOutput("file content missing".into()))?
            .replace(['\r', '\n'], "");
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .map_err(|_| ReviewError::InvalidModelOutput("file content encoding invalid".into()))?;
        let text = String::from_utf8(bytes).map_err(|_| {
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
        if self.head_sha(&review.target).await? != review.head_sha {
            return Err(ReviewError::PrUpdated);
        }
        let comments: Vec<_> = review.findings.iter().map(|finding| json!({"path":finding.path,"side":match finding.side { ReviewSide::LEFT => "LEFT", ReviewSide::RIGHT => "RIGHT" },"line":finding.line,"body":finding.draft_comment})).collect();
        let pull_number = review.target.pull_number.to_string();
        let response = self
            .request(
                reqwest::Method::POST,
                self.endpoint(&review.target, ["pulls", pull_number.as_str(), "reviews"])?,
            )
            .json(&json!({"event":"COMMENT","commit_id":review.head_sha,"comments":comments}))
            .send()
            .await
            .map_err(network_error)?;
        let body = checked_json(response, true).await?;
        let review_id = body.get("id").and_then(Value::as_u64).ok_or_else(|| {
            ReviewError::ReviewPublishFailed("GitHub response omitted review id".into())
        })?;
        Ok(PublishedReview {
            review_id,
            html_url: body
                .get("html_url")
                .and_then(Value::as_str)
                .map(str::to_owned),
        })
    }
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

async fn checked_json(response: Response, publish: bool) -> Result<Value, ReviewError> {
    match response.status() {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => return Err(ReviewError::AuthFailed),
        StatusCode::TOO_MANY_REQUESTS => return Err(ReviewError::RateLimited),
        status if !status.is_success() => {
            return Err(if publish {
                ReviewError::ReviewPublishFailed("GitHub rejected review".into())
            } else {
                ReviewError::NetworkError("GitHub request failed".into())
            })
        }
        _ => {}
    }
    response.json().await.map_err(|_| {
        if publish {
            ReviewError::ReviewPublishFailed("GitHub response was invalid".into())
        } else {
            ReviewError::InvalidModelOutput("GitHub response was invalid".into())
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ReviewError, ReviewFinding, ReviewSide, ReviewSource, ReviewTarget, Severity, SubmitReview,
    };
    use serde_json::json;
    use wiremock::matchers::{body_partial_json, header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn target() -> ReviewTarget {
        ReviewTarget {
            owner: "o".into(),
            repo: "r".into(),
            pull_number: 1,
        }
    }
    fn source(server: &MockServer) -> GithubReviewSource {
        GithubReviewSource::new_with_base_for_test("token", server.uri())
    }

    #[tokio::test]
    async fn reads_utf8_file_at_exact_sha_and_range() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/repos/o/r/contents/src/lib.rs"))
            .and(query_param("ref", "abc"))
            .and(header("authorization", "Bearer token"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"encoding":"base64","content":"b25lCnR3bwp0aHJlZQ=="})),
            )
            .mount(&server)
            .await;
        let content = source(&server)
            .read_file(&target(), "abc", "src/lib.rs", 2, 3)
            .await
            .unwrap();
        assert_eq!(content, "two\nthree");
    }

    #[tokio::test]
    async fn encodes_owner_repo_and_reserved_file_path_segments() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(
                "/repos/o%3Fx/r%23y/contents/dir/a%20%3F%23%25%20%E4%BD%A0%E5%A5%BD.rs",
            ))
            .and(query_param("ref", "abc"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"encoding":"base64","content":"b2s="})),
            )
            .mount(&server)
            .await;
        let target = ReviewTarget {
            owner: "o?x".into(),
            repo: "r#y".into(),
            pull_number: 1,
        };
        let content = source(&server)
            .read_file(&target, "abc", "dir/a ?#% 你好.rs", 1, 1)
            .await
            .unwrap();
        assert_eq!(content, "ok");
    }

    #[tokio::test]
    async fn preflight_marks_missing_and_large_patches_unreviewable() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/repos/o/r/pulls/1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"head":{"sha":"abc"}})))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/o/r/pulls/1/files"))
            .and(query_param("page", "1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([
                {"filename":"a.bin","patch":null},
                {"filename":"big.rs","patch":"x".repeat(200_001)},
                {"filename":"ok.rs","patch":"@@ -1 +1 @@\n-a\n+b"}
            ])))
            .mount(&server)
            .await;
        let files = source(&server)
            .pull_files_at_head(&target(), "abc")
            .await
            .unwrap();
        assert_eq!(
            files.iter().map(|f| f.reviewable).collect::<Vec<_>>(),
            vec![false, false, true]
        );
    }

    #[tokio::test]
    async fn lists_tree_at_exact_sha_and_filters_prefix() {
        let server = MockServer::start().await;
        Mock::given(method("GET")).and(path("/repos/o/r/git/trees/abc")).and(query_param("recursive", "1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"tree":[{"path":"src/a.rs","type":"blob"},{"path":"README.md","type":"blob"},{"path":"src","type":"tree"}]}))).mount(&server).await;
        assert_eq!(
            source(&server)
                .list_repository_tree(&target(), "abc", Some("src"))
                .await
                .unwrap(),
            vec!["src/a.rs"]
        );
    }

    #[tokio::test]
    async fn publish_rechecks_head_and_rejects_race() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/repos/o/r/pulls/1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"head":{"sha":"new"}})))
            .mount(&server)
            .await;
        let review = SubmitReview {
            target: target(),
            head_sha: "abc".into(),
            findings: vec![],
        };
        assert_eq!(
            source(&server).publish(&review).await.unwrap_err(),
            ReviewError::PrUpdated
        );
    }

    #[tokio::test]
    async fn preflight_rechecks_after_fetch_and_rejects_race() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/repos/o/r/pulls/1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"head":{"sha":"abc"}})))
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/o/r/pulls/1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"head":{"sha":"def"}})))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/o/r/pulls/1/files"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([
                {"filename":"ok.rs","patch":"@@ -1 +1 @@\n-a\n+b"}
            ])))
            .mount(&server)
            .await;

        let github = source(&server);
        assert_eq!(
            github.preflight(&target()).await.unwrap_err(),
            ReviewError::PrUpdated
        );
    }

    #[tokio::test]
    async fn publishes_one_comment_review_with_inline_comments() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/repos/o/r/pulls/1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"head":{"sha":"abc"}})))
            .mount(&server)
            .await;
        Mock::given(method("POST")).and(path("/repos/o/r/pulls/1/reviews"))
            .and(body_partial_json(json!({"event":"COMMENT","commit_id":"abc","comments":[{"path":"src/lib.rs","side":"RIGHT","line":2,"body":"fix"}]})))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"id":7,"html_url":"https://example/review/7"}))).mount(&server).await;
        let finding = ReviewFinding {
            id: "f".into(),
            severity: Severity::High,
            path: "src/lib.rs".into(),
            side: ReviewSide::RIGHT,
            line: 2,
            title: "t".into(),
            failure_scenario: "s".into(),
            explanation: "e".into(),
            draft_comment: "fix".into(),
        };
        let published = source(&server)
            .publish(&SubmitReview {
                target: target(),
                head_sha: "abc".into(),
                findings: vec![finding],
            })
            .await
            .unwrap();
        assert_eq!(published.review_id, 7);
    }
}
