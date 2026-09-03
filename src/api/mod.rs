//! HTTP client for Plane's REST API (`/api/v1/`).
//!
//! Every request is workspace-scoped and authenticated with a personal access
//! token sent as `X-API-Key`. Plane Cloud rate-limits to 60 requests per minute
//! per token; a 429 surfaces as [`Error::RateLimited`] carrying the server's
//! `Retry-After` when it sends one.
//!
//! All paths below hang off `{base_url}/api/v1/workspaces/{workspace_slug}`,
//! both of which come from [`Config`]. Path segments are percent-encoded here,
//! and the trailing slash Plane's router insists on is added automatically —
//! without it Django issues a redirect that drops `POST`/`PATCH` bodies.
//!
//! | Method   | Path                                                        | Rust                                    | Returns                  |
//! |----------|-------------------------------------------------------------|-----------------------------------------|--------------------------|
//! | `GET`    | `/projects/`                                                | [`Client::list_projects`]               | [`Paginated<Project>`]   |
//! | `POST`   | `/projects/`                                                | [`Client::create_project`]              | [`Project`]              |
//! | `GET`    | `/projects/{project_id}/`                                   | [`Client::get_project`]                 | [`Project`]              |
//! | `PATCH`  | `/projects/{project_id}/`                                   | [`Client::update_project`]              | [`Project`]              |
//! | `DELETE` | `/projects/{project_id}/`                                   | [`Client::delete_project`]              | `204 No Content`         |
//! | `POST`   | `/projects/{project_id}/archive/`                           | [`Client::archive_project`]             | `204 No Content`         |
//! | `DELETE` | `/projects/{project_id}/archive/`                           | [`Client::unarchive_project`]           | `204 No Content`         |
//! | `GET`    | `/projects/{project_id}/work-items/`                        | [`Client::list_work_items`]             | [`Paginated<WorkItem>`]  |
//! | `POST`   | `/projects/{project_id}/work-items/`                        | [`Client::create_work_item`]            | [`WorkItem`]             |
//! | `GET`    | `/projects/{project_id}/work-items/{id}/`                   | [`Client::get_work_item`]               | [`WorkItem`]             |
//! | `PATCH`  | `/projects/{project_id}/work-items/{id}/`                   | [`Client::update_work_item`]            | [`WorkItem`]             |
//! | `DELETE` | `/projects/{project_id}/work-items/{id}/`                   | [`Client::delete_work_item`]            | `204 No Content`         |
//! | `GET`    | `/work-items/{IDENT}-{seq}/`                                | [`Client::get_work_item_by_key`]        | [`WorkItem`]             |
//! | `GET`    | `/work-items/search/`                                       | [`Client::search_work_items`]           | [`SearchWorkItems`]      |
//! | `GET`    | `/projects/{project_id}/states/`                            | [`Client::list_states`]                 | [`Paginated<StateLite>`] |
//! | `GET`    | `/projects/{project_id}/summary/`                           | [`Client::project_summary`]             | [`ProjectSummary`]       |
//!
//! The states row is the one addition to the set the types module described:
//! "completed" is a property of a work item's *state*, not of the work item, so
//! there is no way to close something without first discovering which state
//! means done.

pub mod types;

use std::{error::Error as StdError, fmt, time::Duration};

use reqwest::{
    RequestBuilder, Response, StatusCode, Url,
    header::{HeaderMap, HeaderValue, RETRY_AFTER},
};
use serde::de::DeserializeOwned;
use uuid::Uuid;

use crate::api::types::{
    ApiError, CreateProjectRequest, CreateWorkItemRequest, DetailParams, ListProjectsParams,
    ListWorkItemsParams, PageParams, Paginated, Project, SearchWorkItems, SearchWorkItemsParams,
    ProjectSummary, StateLite, UpdateProjectRequest, UpdateWorkItemRequest, WorkItem,
};
use crate::config::Config;

/// Result alias for every call on [`Client`].
pub type Result<T> = std::result::Result<T, Error>;

/// How much of a response body to keep when reporting an error. Enough to
/// identify the problem, short enough not to dump a page of HTML into a log.
const BODY_SNIPPET_LEN: usize = 500;

/// A workspace-scoped handle on one Plane instance.
///
/// Cloning is cheap — the underlying connection pool is shared — so hand clones
/// to whatever needs to make requests rather than wrapping this in an `Arc`.
#[derive(Debug, Clone)]
pub struct Client {
    http: reqwest::Client,
    base_url: Url,
    workspace: String,
}

impl Client {
    /// Build a client from resolved configuration.
    pub fn new(config: &Config) -> Result<Self> {
        let mut api_key = HeaderValue::from_str(&config.api.api_key)
            .map_err(|_| Error::InvalidApiKey)?;
        // Keeps the token out of any `Debug` rendering of the header map.
        api_key.set_sensitive(true);

        let mut headers = HeaderMap::new();
        headers.insert("X-API-Key", api_key);

        let http = reqwest::Client::builder()
            .user_agent(concat!("sailboat/", env!("CARGO_PKG_VERSION")))
            .default_headers(headers)
            .build()
            .map_err(Error::Transport)?;

        let base_url = Url::parse(&config.api.base_url).map_err(|_| Error::InvalidBaseUrl {
            url: config.api.base_url.clone(),
        })?;

        Ok(Self {
            http,
            base_url,
            workspace: config.workspace.slug.clone(),
        })
    }

    // -- Projects ----------------------------------------------------------

    /// `GET /projects/` — projects the token can see, newest-sorted by default.
    pub async fn list_projects(&self, params: &ListProjectsParams) -> Result<Paginated<Project>> {
        let url = self.endpoint(&["projects"])?;
        self.json(self.http.get(url).query(params)).await
    }

    /// `POST /projects/` — 409 if the identifier is already taken.
    pub async fn create_project(&self, project: &CreateProjectRequest) -> Result<Project> {
        let url = self.endpoint(&["projects"])?;
        self.json(self.http.post(url).json(project)).await
    }

    /// `GET /projects/{project_id}/`
    pub async fn get_project(&self, project_id: Uuid, params: &DetailParams) -> Result<Project> {
        let url = self.endpoint(&["projects", &project_id.to_string()])?;
        self.json(self.http.get(url).query(params)).await
    }

    /// `PATCH /projects/{project_id}/` — only the fields you set are changed.
    pub async fn update_project(
        &self,
        project_id: Uuid,
        changes: &UpdateProjectRequest,
    ) -> Result<Project> {
        let url = self.endpoint(&["projects", &project_id.to_string()])?;
        self.json(self.http.patch(url).json(changes)).await
    }

    /// `DELETE /projects/{project_id}/` — permanent, and takes every work item
    /// in the project with it.
    ///
    /// Reading the project back afterwards returns 403, not 404: the permission
    /// check runs ahead of the lookup and the caller's membership is gone too.
    pub async fn delete_project(&self, project_id: Uuid) -> Result<()> {
        let url = self.endpoint(&["projects", &project_id.to_string()])?;
        self.discard(self.http.delete(url)).await
    }

    /// `POST /projects/{project_id}/archive/` — hides the project from active
    /// listings and drops it from everyone's favourites. Reversible.
    pub async fn archive_project(&self, project_id: Uuid) -> Result<()> {
        let url = self.endpoint(&["projects", &project_id.to_string(), "archive"])?;
        self.discard(self.http.post(url).json(&serde_json::json!({})))
            .await
    }

    /// `DELETE /projects/{project_id}/archive/` — restores an archived project.
    pub async fn unarchive_project(&self, project_id: Uuid) -> Result<()> {
        let url = self.endpoint(&["projects", &project_id.to_string(), "archive"])?;
        self.discard(self.http.delete(url)).await
    }

    // -- Work items --------------------------------------------------------

    /// `GET /projects/{project_id}/work-items/`
    ///
    /// Archived work items, drafts, and anything sitting in a triage state are
    /// excluded. Setting both `external_id` and `external_source` on the params
    /// looks up a single imported item instead of listing.
    pub async fn list_work_items(
        &self,
        project_id: Uuid,
        params: &ListWorkItemsParams,
    ) -> Result<Paginated<WorkItem>> {
        let url = self.endpoint(&["projects", &project_id.to_string(), "work-items"])?;
        self.json(self.http.get(url).query(params)).await
    }

    /// `POST /projects/{project_id}/work-items/`
    ///
    /// Omitting `assignees` falls back to the project's default assignee; an
    /// empty vector suppresses that.
    pub async fn create_work_item(
        &self,
        project_id: Uuid,
        item: &CreateWorkItemRequest,
    ) -> Result<WorkItem> {
        let url = self.endpoint(&["projects", &project_id.to_string(), "work-items"])?;
        self.json(self.http.post(url).json(item)).await
    }

    /// `GET /projects/{project_id}/work-items/{work_item_id}/`
    pub async fn get_work_item(
        &self,
        project_id: Uuid,
        work_item_id: Uuid,
        params: &DetailParams,
    ) -> Result<WorkItem> {
        let url = self.endpoint(&[
            "projects",
            &project_id.to_string(),
            "work-items",
            &work_item_id.to_string(),
        ])?;
        self.json(self.http.get(url).query(params)).await
    }

    /// `PATCH /projects/{project_id}/work-items/{work_item_id}/`
    ///
    /// `assignees` and `labels` replace the existing set rather than adding to
    /// it. Moving to a state in the completed group makes Plane stamp
    /// `completed_at` itself.
    pub async fn update_work_item(
        &self,
        project_id: Uuid,
        work_item_id: Uuid,
        changes: &UpdateWorkItemRequest,
    ) -> Result<WorkItem> {
        let url = self.endpoint(&[
            "projects",
            &project_id.to_string(),
            "work-items",
            &work_item_id.to_string(),
        ])?;
        self.json(self.http.patch(url).json(changes)).await
    }

    /// `DELETE /projects/{project_id}/work-items/{work_item_id}/`
    ///
    /// Sub-items cascade: deleting a parent deletes its children too.
    pub async fn delete_work_item(&self, project_id: Uuid, work_item_id: Uuid) -> Result<()> {
        let url = self.endpoint(&[
            "projects",
            &project_id.to_string(),
            "work-items",
            &work_item_id.to_string(),
        ])?;
        self.discard(self.http.delete(url)).await
    }

    /// `GET /work-items/{project_identifier}-{sequence_id}/`
    ///
    /// Looks a work item up by the key people actually type, e.g. `PROJ-12`.
    /// Workspace-scoped, so no project id is needed.
    pub async fn get_work_item_by_key(
        &self,
        project_identifier: &str,
        sequence_id: i32,
    ) -> Result<WorkItem> {
        // The hyphen is part of Plane's route pattern, not a path separator, so
        // this stays a single segment.
        let key = format!("{project_identifier}-{sequence_id}");
        let url = self.endpoint(&["work-items", &key])?;
        self.json(self.http.get(url)).await
    }

    /// `GET /work-items/search/` — substring match over name, sequence id and
    /// project identifier. Not paginated.
    pub async fn search_work_items(
        &self,
        params: &SearchWorkItemsParams,
    ) -> Result<SearchWorkItems> {
        let url = self.endpoint(&["work-items", "search"])?;
        self.json(self.http.get(url).query(params)).await
    }

    // -- States ------------------------------------------------------------

    /// `GET /projects/{project_id}/states/`
    ///
    /// Returns the reduced view; the full state payload carries more, but
    /// nothing a board needs. New projects are seeded with a default set, so
    /// this is never empty.
    pub async fn list_states(
        &self,
        project_id: Uuid,
        params: &PageParams,
    ) -> Result<Paginated<StateLite>> {
        let url = self.endpoint(&["projects", &project_id.to_string(), "states"])?;
        self.json(self.http.get(url).query(params)).await
    }

    /// `GET /projects/{project_id}/summary/`
    ///
    /// Counts of the project's contents. The only source of a work item count
    /// that does not involve listing the work items themselves -- but it is one
    /// request per project, so cache the answer rather than calling it per frame.
    pub async fn project_summary(&self, project_id: Uuid) -> Result<ProjectSummary> {
        let url = self.endpoint(&["projects", &project_id.to_string(), "summary"])?;
        self.json(self.http.get(url)).await
    }

    // -- Plumbing ----------------------------------------------------------

    /// Builds a workspace-scoped URL, percent-encoding each segment and adding
    /// the trailing slash Plane's router requires.
    fn endpoint(&self, segments: &[&str]) -> Result<Url> {
        let mut url = self.base_url.clone();
        {
            let mut path = url
                .path_segments_mut()
                .map_err(|()| Error::InvalidBaseUrl {
                    url: self.base_url.to_string(),
                })?;
            // Drop the empty segment a bare origin like `https://api.plane.so/`
            // leaves behind, so we don't emit `//api/v1/...`.
            path.pop_if_empty();
            path.extend(["api", "v1", "workspaces", self.workspace.as_str()]);
            path.extend(segments);
            path.push("");
        }
        Ok(url)
    }

    /// Sends a request and turns any non-2xx into a typed [`Error`].
    async fn execute(&self, request: RequestBuilder) -> Result<Response> {
        let response = request.send().await.map_err(Error::Transport)?;
        let status = response.status();
        if status.is_success() {
            return Ok(response);
        }

        let retry_after = parse_retry_after(response.headers());
        // A failed read here shouldn't mask the status we already know about.
        let body = response.text().await.unwrap_or_default();
        let error = parse_api_error(&body);

        if status == StatusCode::TOO_MANY_REQUESTS {
            Err(Error::RateLimited { retry_after, error })
        } else {
            Err(Error::Status { status, error })
        }
    }

    /// Sends a request and decodes the body.
    async fn json<T: DeserializeOwned>(&self, request: RequestBuilder) -> Result<T> {
        let response = self.execute(request).await?;
        let url = response.url().to_string();
        let body = response.text().await.map_err(Error::Transport)?;

        serde_json::from_str(&body).map_err(|source| Error::Decode {
            url,
            source,
            body: snippet(&body),
        })
    }

    /// Sends a request whose success case has no body (`204 No Content`).
    async fn discard(&self, request: RequestBuilder) -> Result<()> {
        self.execute(request).await?;
        Ok(())
    }
}

/// Everything that can go wrong talking to Plane.
#[derive(Debug)]
pub enum Error {
    /// `base_url` isn't a URL that can root an API path.
    InvalidBaseUrl { url: String },
    /// The token contains bytes that can't go in an HTTP header.
    InvalidApiKey,
    /// The request never completed: DNS, TLS, timeout, connection reset.
    Transport(reqwest::Error),
    /// Plane answered, with a non-2xx status.
    Status { status: StatusCode, error: ApiError },
    /// Rate limited. Plane Cloud allows 60 requests per minute per token.
    RateLimited {
        retry_after: Option<Duration>,
        error: ApiError,
    },
    /// The response arrived but didn't match the expected shape.
    Decode {
        url: String,
        source: serde_json::Error,
        body: String,
    },
}

impl Error {
    /// The HTTP status, when the failure got that far.
    pub fn status(&self) -> Option<StatusCode> {
        match self {
            Error::Status { status, .. } => Some(*status),
            Error::RateLimited { .. } => Some(StatusCode::TOO_MANY_REQUESTS),
            _ => None,
        }
    }

    /// Whether the resource is missing — the usual "already deleted" case.
    pub fn is_not_found(&self) -> bool {
        self.status() == Some(StatusCode::NOT_FOUND)
    }

    /// Whether this collided with something that already exists: a taken
    /// project identifier, or a reused `external_id`/`external_source` pair.
    pub fn is_conflict(&self) -> bool {
        self.status() == Some(StatusCode::CONFLICT)
    }

    /// For a 409 raised by a duplicate external id, the id of the record that
    /// already exists — which is usually the one you wanted.
    pub fn conflicting_id(&self) -> Option<&str> {
        match self {
            Error::Status { error, .. } => error.id.as_deref(),
            _ => None,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::InvalidBaseUrl { url } => {
                write!(f, "{url} is not a usable Plane base URL")
            }
            Error::InvalidApiKey => {
                f.write_str("the Plane API token is not valid as an HTTP header value")
            }
            Error::Transport(source) => write!(f, "request to Plane failed: {source}"),
            Error::Status { status, error } => {
                write!(f, "Plane returned {status}: {}", describe(error))
            }
            Error::RateLimited { retry_after, error } => match retry_after {
                Some(delay) => write!(
                    f,
                    "rate limited by Plane, retry in {}s: {}",
                    delay.as_secs(),
                    describe(error)
                ),
                None => write!(f, "rate limited by Plane: {}", describe(error)),
            },
            Error::Decode { url, source, body } => {
                write!(f, "could not decode the response from {url}: {source}\nbody: {body}")
            }
        }
    }
}

impl StdError for Error {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Error::Transport(source) => Some(source),
            Error::Decode { source, .. } => Some(source),
            _ => None,
        }
    }
}

/// Renders whichever of Plane's two error shapes came back.
fn describe(error: &ApiError) -> String {
    // Plane uses three different keys depending on which layer rejected the
    // request: `error` from its own views, `detail` from DRF's auth and
    // permission checks, `error_message` from the rate limiter.
    if let Some(message) = error
        .error
        .as_ref()
        .or(error.detail.as_ref())
        .or(error.error_message.as_ref())
    {
        return message.clone();
    }
    if error.fields.is_empty() {
        return "no detail given".to_string();
    }
    // Validation failures arrive as {"field": ["message", ...]}.
    error
        .fields
        .iter()
        .map(|(field, detail)| format!("{field}: {detail}"))
        .collect::<Vec<_>>()
        .join("; ")
}

/// Plane returns JSON for its own errors, but a proxy or gateway in front of it
/// may not — fall back to keeping the raw text.
fn parse_api_error(body: &str) -> ApiError {
    serde_json::from_str(body).unwrap_or_else(|_| ApiError {
        error: (!body.trim().is_empty()).then(|| snippet(body)),
        ..ApiError::default()
    })
}

fn parse_retry_after(headers: &HeaderMap) -> Option<Duration> {
    let seconds = headers.get(RETRY_AFTER)?.to_str().ok()?.trim().parse().ok()?;
    Some(Duration::from_secs(seconds))
}

fn snippet(body: &str) -> String {
    let trimmed = body.trim();
    match trimmed.char_indices().nth(BODY_SNIPPET_LEN) {
        Some((cut, _)) => format!("{}…", &trimmed[..cut]),
        None => trimmed.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::types::{Priority, StateGroup};
    use crate::config::{Api, Workspace};
    use chrono::{Days, Utc};
    use color_eyre::eyre::{ensure, eyre};

    fn client_for(base_url: &str, workspace: &str) -> Client {
        Client::new(&Config {
            api: Api {
                base_url: base_url.to_string(),
                api_key: "plane_api_testtoken".to_string(),
            },
            workspace: Workspace {
                slug: workspace.to_string(),
            },
        })
        .expect("test client should build")
    }

    #[test]
    fn endpoints_match_planes_routes() {
        let client = client_for("https://api.plane.so", "my-team");
        let project = "550e8400-e29b-41d4-a716-446655440000";

        assert_eq!(
            client.endpoint(&["projects"]).unwrap().as_str(),
            "https://api.plane.so/api/v1/workspaces/my-team/projects/"
        );
        assert_eq!(
            client
                .endpoint(&["projects", project, "work-items"])
                .unwrap()
                .as_str(),
            format!("https://api.plane.so/api/v1/workspaces/my-team/projects/{project}/work-items/")
        );
        assert_eq!(
            client.endpoint(&["work-items", "search"]).unwrap().as_str(),
            "https://api.plane.so/api/v1/workspaces/my-team/work-items/search/"
        );
    }

    #[test]
    fn a_base_url_with_a_trailing_slash_does_not_double_up() {
        let bare = client_for("https://api.plane.so", "my-team");
        let slashed = client_for("https://api.plane.so/", "my-team");
        assert_eq!(
            bare.endpoint(&["projects"]).unwrap(),
            slashed.endpoint(&["projects"]).unwrap()
        );
    }

    #[test]
    fn self_hosted_instances_may_sit_under_a_path_prefix() {
        let client = client_for("https://plane.example.com/plane", "my-team");
        assert_eq!(
            client.endpoint(&["projects"]).unwrap().as_str(),
            "https://plane.example.com/plane/api/v1/workspaces/my-team/projects/"
        );
    }

    #[test]
    fn path_segments_are_percent_encoded() {
        let client = client_for("https://api.plane.so", "a team/with slash");
        let url = client.endpoint(&["projects"]).unwrap();
        assert!(
            url.as_str().contains("a%20team%2Fwith%20slash"),
            "slug should not be able to escape its segment: {url}"
        );
    }

    #[test]
    fn work_item_keys_stay_one_segment() {
        let client = client_for("https://api.plane.so", "my-team");
        assert_eq!(
            client.endpoint(&["work-items", "PROJ-12"]).unwrap().as_str(),
            "https://api.plane.so/api/v1/workspaces/my-team/work-items/PROJ-12/"
        );
    }

    #[test]
    fn errors_expose_the_conflicting_id_from_a_409() {
        let error = Error::Status {
            status: StatusCode::CONFLICT,
            error: parse_api_error(
                r#"{"error":"Issue with the same external id and external source already exists",
                    "id":"550e8400-e29b-41d4-a716-446655440000"}"#,
            ),
        };
        assert!(error.is_conflict());
        assert!(!error.is_not_found());
        assert_eq!(
            error.conflicting_id(),
            Some("550e8400-e29b-41d4-a716-446655440000")
        );
    }

    #[test]
    fn validation_failures_render_every_field() {
        let error = Error::Status {
            status: StatusCode::BAD_REQUEST,
            error: parse_api_error(r#"{"name":["This field is required."]}"#),
        };
        assert!(error.to_string().contains("name"), "{error}");
    }

    #[test]
    fn an_auth_failure_reports_drfs_detail_key() {
        let error = Error::Status {
            status: StatusCode::FORBIDDEN,
            error: parse_api_error(r#"{"detail":"Given API token is not valid"}"#),
        };
        assert_eq!(
            error.to_string(),
            "Plane returned 403 Forbidden: Given API token is not valid"
        );
    }

    #[test]
    fn a_rate_limit_reports_the_limiters_own_key() {
        let error = Error::RateLimited {
            retry_after: Some(Duration::from_secs(13)),
            error: parse_api_error(r#"{"error_code":5900,"error_message":"RATE_LIMIT_EXCEEDED"}"#),
        };
        assert_eq!(
            error.to_string(),
            "rate limited by Plane, retry in 13s: RATE_LIMIT_EXCEEDED"
        );
    }

    #[test]
    fn a_non_json_error_body_is_kept_rather_than_swallowed() {
        let error = parse_api_error("<html>502 Bad Gateway</html>");
        assert_eq!(error.error.as_deref(), Some("<html>502 Bad Gateway</html>"));
    }

    // -- Live integration test --------------------------------------------

    /// Walks a project through its whole life against a real Plane workspace:
    /// create it, fill it with work items, reorganise and edit them, close
    /// every one, then delete the lot so nothing is left behind.
    ///
    /// Ignored by default because it writes to a real workspace. Run it with:
    ///
    /// ```text
    /// cargo test --  --ignored --nocapture project_and_work_item_lifecycle
    /// ```
    ///
    /// It needs `PLANE_API_KEY` and `PLANE_WORKSPACE` (a `.env` file is fine),
    /// and the token must belong to a workspace admin, since it creates and
    /// deletes projects. It makes roughly 30 calls against a 60/minute budget,
    /// so don't run it in a tight loop.
    #[tokio::test]
    #[ignore = "writes to a real Plane workspace; run with --ignored"]
    async fn project_and_work_item_lifecycle() -> color_eyre::Result<()> {
        let config = Config::load()?;
        let client = Client::new(&config)?;

        // Plane rejects most punctuation in both project names and identifiers
        // (including hyphens), so the run marker is plain uppercase alphanumerics.
        let marker = Uuid::new_v4().simple().to_string()[..6].to_uppercase();

        let project = client
            .create_project(&CreateProjectRequest {
                name: format!("Sailboat lifecycle {marker}"),
                identifier: format!("SBT{marker}"),
                description: Some("Temporary. Created and removed by sailboat's test suite.".into()),
                ..Default::default()
            })
            .await?;

        // From here on every failure still has to reach teardown, so the body
        // returns a Result rather than asserting, and both outcomes are
        // reported — the scenario's first, since it explains the teardown's.
        let outcome = exercise(&client, &project, &marker).await;
        let cleanup = teardown(&client, project.id).await;

        // A failed teardown leaves a real project sitting in the workspace, so
        // say so even when the scenario also failed and its error is the one
        // that gets returned.
        if let Err(error) = &cleanup {
            eprintln!(
                "teardown failed; project {} ({}) may still exist: {error:?}",
                project.identifier, project.id
            );
        }

        outcome?;
        cleanup?;
        Ok(())
    }

    async fn exercise(client: &Client, project: &Project, marker: &str) -> color_eyre::Result<()> {
        // The new project is visible in the workspace listing...
        let listed = client
            .list_projects(&ListProjectsParams {
                per_page: Some(100),
                ..Default::default()
            })
            .await?;
        ensure!(
            listed.results.iter().any(|found| found.id == project.id),
            "the new project is missing from the project listing"
        );

        // ...and on its own.
        let fetched = client.get_project(project.id, &DetailParams::default()).await?;
        ensure!(
            fetched.identifier == project.identifier,
            "get_project returned a different project"
        );

        // Edit it: turn on a feature and rewrite the description.
        let updated = client
            .update_project(
                project.id,
                &UpdateProjectRequest {
                    description: Some("Reorganised mid-test.".into()),
                    cycle_view: Some(true),
                    ..Default::default()
                },
            )
            .await?;
        ensure!(updated.cycle_view, "update_project did not enable cycle_view");

        // Archive and restore.
        client.archive_project(project.id).await?;
        let archived = client.get_project(project.id, &DetailParams::default()).await?;
        ensure!(
            archived.archived_at.is_some(),
            "archive_project left archived_at unset"
        );
        client.unarchive_project(project.id).await?;
        let restored = client.get_project(project.id, &DetailParams::default()).await?;
        ensure!(
            restored.archived_at.is_none(),
            "unarchive_project left archived_at set"
        );

        // A new project is seeded with default states; find the ones we need.
        let states = client
            .list_states(
                project.id,
                &PageParams {
                    per_page: Some(100),
                    ..Default::default()
                },
            )
            .await?;
        let in_progress = states
            .results
            .iter()
            .find(|state| state.group == StateGroup::Started)
            .ok_or_else(|| eyre!("project has no state in the started group"))?;
        let done = states
            .results
            .iter()
            .find(|state| state.group == StateGroup::Completed)
            .ok_or_else(|| eyre!("project has no state in the completed group"))?;

        // Build a small tree: one parent with two sub-items.
        let parent = client
            .create_work_item(
                project.id,
                &CreateWorkItemRequest {
                    name: format!("Rig the mainsail {marker}"),
                    description_html: Some("<p>Umbrella item for the rigging work.</p>".into()),
                    priority: Some(Priority::High),
                    ..Default::default()
                },
            )
            .await?;
        ensure!(
            parent.sequence_id > 0,
            "Plane did not assign a sequence id to the new work item"
        );

        let mut children = Vec::new();
        for (offset, (name, priority)) in [
            ("Hoist the halyard", Priority::Medium),
            ("Tie the bowline", Priority::Low),
        ]
        .into_iter()
        .enumerate()
        {
            let child = client
                .create_work_item(
                    project.id,
                    &CreateWorkItemRequest {
                        name: format!("{name} {marker}"),
                        parent: Some(parent.id),
                        priority: Some(priority),
                        start_date: Some(Utc::now().date_naive()),
                        target_date: Some(Utc::now().date_naive() + Days::new(offset as u64 + 1)),
                        ..Default::default()
                    },
                )
                .await?;
            ensure!(child.parent.is_some(), "sub-item was created without a parent");
            children.push(child);
        }

        let everything: Vec<&WorkItem> = std::iter::once(&parent).chain(children.iter()).collect();

        // All three come back from the listing.
        let listed = client
            .list_work_items(
                project.id,
                &ListWorkItemsParams {
                    per_page: Some(100),
                    ..Default::default()
                },
            )
            .await?;
        ensure!(
            listed.total_count >= 3,
            "expected at least 3 work items, listing reported {}",
            listed.total_count
        );

        // The human-facing key resolves to the same record.
        let by_key = client
            .get_work_item_by_key(&project.identifier, parent.sequence_id)
            .await?;
        ensure!(
            by_key.id == parent.id,
            "{}-{} resolved to a different work item",
            project.identifier,
            parent.sequence_id
        );

        // Search finds them by the run marker.
        let hits = client
            .search_work_items(&SearchWorkItemsParams {
                search: marker.to_string(),
                project_id: Some(project.id),
                ..Default::default()
            })
            .await?;
        ensure!(
            hits.issues.iter().any(|hit| hit.id == parent.id),
            "search for {marker} did not turn up the parent item"
        );

        // Reorganise: everything moves into progress, and the parent gets
        // re-prioritised and renamed on the way.
        for item in &everything {
            client
                .update_work_item(
                    project.id,
                    item.id,
                    &UpdateWorkItemRequest {
                        state: Some(in_progress.id),
                        ..Default::default()
                    },
                )
                .await?;
        }
        let renamed = client
            .update_work_item(
                project.id,
                parent.id,
                &UpdateWorkItemRequest {
                    name: Some(format!("Rig the mainsail (revised) {marker}")),
                    priority: Some(Priority::Urgent),
                    ..Default::default()
                },
            )
            .await?;
        ensure!(
            renamed.priority == Priority::Urgent,
            "priority did not stick, got {:?}",
            renamed.priority
        );
        ensure!(
            renamed.name.contains("revised"),
            "rename did not stick, got {:?}",
            renamed.name
        );

        // Expanding state should inline the object rather than the bare id.
        let expanded = client
            .get_work_item(
                project.id,
                parent.id,
                &DetailParams {
                    expand: Some("state".into()),
                    ..Default::default()
                },
            )
            .await?;
        let state = expanded
            .state
            .as_ref()
            .ok_or_else(|| eyre!("work item came back with no state"))?;
        ensure!(
            state.expanded().is_some(),
            "expand=state returned a bare id instead of the state object"
        );

        // Close everything.
        for item in &everything {
            let closed = client
                .update_work_item(
                    project.id,
                    item.id,
                    &UpdateWorkItemRequest {
                        state: Some(done.id),
                        ..Default::default()
                    },
                )
                .await?;
            ensure!(
                closed.completed_at.is_some(),
                "moving {} to the done state did not stamp completed_at",
                closed.name
            );
        }

        Ok(())
    }

    /// Removes every work item, then the project, and confirms it is gone.
    /// Runs even when the scenario above failed, so a bad run leaves nothing
    /// behind in the workspace.
    async fn teardown(client: &Client, project_id: Uuid) -> color_eyre::Result<()> {
        let items = client
            .list_work_items(
                project_id,
                &ListWorkItemsParams {
                    per_page: Some(100),
                    ..Default::default()
                },
            )
            .await?;

        for item in &items.results {
            match client.delete_work_item(project_id, item.id).await {
                Ok(()) => {}
                // Deleting a parent cascades to its children, so by the time we
                // reach one it may already be gone.
                Err(error) if error.is_not_found() => {}
                Err(error) => return Err(error.into()),
            }
        }

        let remaining = client
            .list_work_items(
                project_id,
                &ListWorkItemsParams {
                    per_page: Some(100),
                    ..Default::default()
                },
            )
            .await?;
        ensure!(
            remaining.results.is_empty(),
            "{} work items survived teardown",
            remaining.results.len()
        );

        client.delete_project(project_id).await?;

        match client.get_project(project_id, &DetailParams::default()).await {
            // Plane answers a deleted project with 403 rather than 404: the
            // permission check runs before the lookup, and our membership went
            // with the project. Either status confirms it is gone.
            Err(error)
                if matches!(
                    error.status(),
                    Some(StatusCode::NOT_FOUND | StatusCode::FORBIDDEN)
                ) =>
            {
                Ok(())
            }
            Err(error) => Err(error.into()),
            Ok(_) => Err(eyre!("the project is still readable after being deleted")),
        }
    }
}
