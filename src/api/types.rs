//! Request and response types for the Plane REST API (`/api/v1/`).
//!
//! Covers the project and work-item endpoints. Shapes are taken from
//! <https://developers.plane.so/api-reference> cross-checked against Plane's own
//! DRF serializers and models (`apps/api/plane/api/serializers/{project,issue}.py`,
//! `apps/api/plane/db/models/{project,issue}.py`).
//!
//! Conventions used throughout:
//!
//! * Request structs skip `None` fields when serialising, so the same struct works
//!   for a full `POST` body and a sparse `PATCH` body. Build them with
//!   `..Default::default()`.
//! * Response structs mark nearly everything `#[serde(default)]`. Plane trims the
//!   payload when you pass `?fields=`, and self-hosted instances lag Cloud by a
//!   release or two, so a missing key should never fail a decode.
//! * Unknown fields are ignored rather than rejected, for the same reason.
//!
//! The endpoint each type belongs to, and the client that calls it, live in
//! [`crate::api`] — see the route table on that module.
//!
//! Authentication is a `X-API-Key: <personal access token>` header, or
//! `Authorization: Bearer <oauth token>`. Cloud rate-limits to 60 requests per
//! minute per key and reports headroom in `X-RateLimit-Remaining` /
//! `X-RateLimit-Reset`.

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Shared scalars
// ---------------------------------------------------------------------------

/// Work item priority. Plane defaults new work items to [`Priority::None`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Priority {
    Urgent,
    High,
    Medium,
    Low,
    #[default]
    None,
}

/// Visibility of a project inside its workspace.
///
/// Serialised as the integer Plane stores it as: `0` for secret, `2` for public.
/// Note the gap — `1` is not a valid value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(try_from = "u8", into = "u8")]
pub enum ProjectNetwork {
    /// Visible only to members of the project.
    Secret,
    /// Visible to everyone in the workspace.
    #[default]
    Public,
}

impl From<ProjectNetwork> for u8 {
    fn from(network: ProjectNetwork) -> u8 {
        match network {
            ProjectNetwork::Secret => 0,
            ProjectNetwork::Public => 2,
        }
    }
}

impl TryFrom<u8> for ProjectNetwork {
    type Error = String;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(ProjectNetwork::Secret),
            2 => Ok(ProjectNetwork::Public),
            other => Err(format!("unknown project network {other}, expected 0 or 2")),
        }
    }
}

/// The bucket a state belongs to. Drives board columns and "is this done?" logic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StateGroup {
    Backlog,
    Unstarted,
    Started,
    Completed,
    Cancelled,
    /// Intake/triage. Work items in a triage state are hidden from the normal
    /// work-item list endpoint.
    Triage,
}

/// A project member's role, as the integer Plane stores it as.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "u8", into = "u8")]
pub enum Role {
    Guest,
    Member,
    Admin,
}

impl From<Role> for u8 {
    fn from(role: Role) -> u8 {
        match role {
            Role::Guest => 5,
            Role::Member => 15,
            Role::Admin => 20,
        }
    }
}

impl TryFrom<u8> for Role {
    type Error = String;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            5 => Ok(Role::Guest),
            15 => Ok(Role::Member),
            20 => Ok(Role::Admin),
            other => Err(format!("unknown role {other}, expected 5, 15 or 20")),
        }
    }
}

// ---------------------------------------------------------------------------
// Expansion
// ---------------------------------------------------------------------------

/// Anything the API returns with a stable `id`.
pub trait Identified {
    fn id(&self) -> Uuid;
}

/// A related field that arrives as a bare UUID, or as a nested object when the
/// field is named in the request's `expand` parameter.
///
/// ```ignore
/// // ?expand=state
/// match &work_item.state {
///     Some(state) => println!("{}", state.expanded().map_or("…", |s| &s.name)),
///     None => println!("no state"),
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Expandable<T> {
    /// The default: just the primary key.
    Id(Uuid),
    /// The related object, because it was listed in `expand`.
    Expanded(Box<T>),
}

impl<T> Expandable<T> {
    /// The nested object, or `None` if this field was not expanded.
    pub fn expanded(&self) -> Option<&T> {
        match self {
            Expandable::Id(_) => None,
            Expandable::Expanded(value) => Some(value),
        }
    }
}

impl<T: Identified> Expandable<T> {
    /// The primary key, whether or not the field was expanded.
    pub fn id(&self) -> Uuid {
        match self {
            Expandable::Id(id) => *id,
            Expandable::Expanded(value) => value.id(),
        }
    }
}

// ---------------------------------------------------------------------------
// Lite objects returned by `expand`
// ---------------------------------------------------------------------------

/// Reduced user, returned for expanded `created_by`, `updated_by`, `project_lead`,
/// `default_assignee` and `assignees`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct UserLite {
    pub id: Uuid,
    #[serde(default)]
    pub first_name: Option<String>,
    #[serde(default)]
    pub last_name: Option<String>,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
    /// Raw avatar path as stored. Prefer [`UserLite::avatar_url`].
    #[serde(default)]
    pub avatar: Option<String>,
    /// Resolved, fetchable avatar URL.
    #[serde(default)]
    pub avatar_url: Option<String>,
}

impl Identified for UserLite {
    fn id(&self) -> Uuid {
        self.id
    }
}

/// Reduced workspace, returned for an expanded `workspace`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct WorkspaceLite {
    pub id: Uuid,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub slug: String,
}

impl Identified for WorkspaceLite {
    fn id(&self) -> Uuid {
        self.id
    }
}

/// Reduced project, returned for an expanded `project` and by the project-picker
/// listing.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ProjectLite {
    pub id: Uuid,
    #[serde(default)]
    pub name: String,
    /// Short key prefixed to work item numbers, e.g. `PROJ` in `PROJ-12`.
    #[serde(default)]
    pub identifier: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub emoji: Option<String>,
    #[serde(default)]
    pub icon_prop: Option<Value>,
    #[serde(default)]
    pub cover_image: Option<String>,
    #[serde(default)]
    pub cover_image_url: Option<String>,
    #[serde(default)]
    pub archived_at: Option<DateTime<Utc>>,
}

impl Identified for ProjectLite {
    fn id(&self) -> Uuid {
        self.id
    }
}

/// Reduced state, returned for an expanded `state`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StateLite {
    pub id: Uuid,
    #[serde(default)]
    pub name: String,
    /// Hex colour, e.g. `#60646C`.
    #[serde(default)]
    pub color: String,
    pub group: StateGroup,
}

impl Identified for StateLite {
    fn id(&self) -> Uuid {
        self.id
    }
}

/// Reduced label, returned for an expanded `labels`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct LabelLite {
    pub id: Uuid,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub color: Option<String>,
}

impl Identified for LabelLite {
    fn id(&self) -> Uuid {
        self.id
    }
}

/// Reduced work item, returned for an expanded `parent`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct WorkItemLite {
    pub id: Uuid,
    #[serde(default)]
    pub sequence_id: i32,
    #[serde(default)]
    pub project_id: Option<Uuid>,
}

impl Identified for WorkItemLite {
    fn id(&self) -> Uuid {
        self.id
    }
}

/// A single point on an estimate scale, returned for an expanded `estimate_point`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct EstimatePoint {
    pub id: Uuid,
    /// The estimate scale this point belongs to.
    #[serde(default)]
    pub estimate: Option<Uuid>,
    /// Position within the scale.
    #[serde(default)]
    pub key: i32,
    /// Displayed label, e.g. `8` or `Large`.
    #[serde(default)]
    pub value: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub project: Option<Uuid>,
    #[serde(default)]
    pub workspace: Option<Uuid>,
    #[serde(default)]
    pub created_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub updated_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub created_by: Option<Uuid>,
    #[serde(default)]
    pub updated_by: Option<Uuid>,
    #[serde(default)]
    pub deleted_at: Option<DateTime<Utc>>,
}

impl Identified for EstimatePoint {
    fn id(&self) -> Uuid {
        self.id
    }
}

// ---------------------------------------------------------------------------
// Pagination
// ---------------------------------------------------------------------------

/// The envelope every list endpoint wraps its results in.
///
/// Paging is cursor-based: feed [`Paginated::next_cursor`] back as the `cursor`
/// query parameter while [`Paginated::next_page_results`] is true. Cursors look
/// like `"20:1:0"` (`per_page:page:offset`) and are always present as strings,
/// even on the last page.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Paginated<T> {
    /// Field the results are grouped by. Null on the project and work-item lists.
    #[serde(default)]
    pub grouped_by: Option<String>,
    #[serde(default)]
    pub sub_grouped_by: Option<String>,
    /// Rows matching the query across every page.
    #[serde(default)]
    pub total_count: i64,
    /// Number of rows in `results`, i.e. on this page.
    #[serde(default)]
    pub count: i64,
    /// Same value as `total_count`.
    #[serde(default)]
    pub total_results: i64,
    #[serde(default)]
    pub total_pages: i64,
    pub next_cursor: String,
    pub prev_cursor: String,
    /// Whether a further page exists after this one.
    #[serde(default)]
    pub next_page_results: bool,
    #[serde(default)]
    pub prev_page_results: bool,
    /// Extra aggregate stats, when the endpoint computes any.
    #[serde(default)]
    pub extra_stats: Option<Value>,
    #[serde(default = "Vec::new")]
    pub results: Vec<T>,
}

impl<T> Paginated<T> {
    /// The cursor to pass as `cursor` for the next page, or `None` at the end.
    pub fn next(&self) -> Option<&str> {
        self.next_page_results.then_some(self.next_cursor.as_str())
    }
}

/// Query parameters shared by every list endpoint.
///
/// `expand` and `fields` are comma-separated field names, e.g.
/// `expand: Some("state,project_lead".into())`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PageParams {
    /// Cursor from a previous response's `next_cursor`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    /// Rows per page. Defaults to 20, capped at 100.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub per_page: Option<u32>,
    /// Field to sort by; prefix with `-` to reverse, e.g. `-created_at`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order_by: Option<String>,
    /// Comma-separated related fields to inline instead of returning as UUIDs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expand: Option<String>,
    /// Comma-separated allowlist of fields to return. Everything else is omitted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fields: Option<String>,
}

/// Query parameters accepted by the single-resource `GET` endpoints.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DetailParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expand: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fields: Option<String>,
}

// ---------------------------------------------------------------------------
// Projects
// ---------------------------------------------------------------------------

/// A project as returned by the list, retrieve, create and update endpoints.
///
/// The counts and membership fields ([`Project::total_members`] onwards) are
/// annotations the list and retrieve queries add; they are absent from the
/// `POST`/`PATCH` responses, hence the `Option`s.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Project {
    pub id: Uuid,
    #[serde(default)]
    pub name: String,
    /// Short key prefixed to work item numbers, e.g. `PROJ` in `PROJ-12`.
    /// Uppercased server-side, max 12 characters, unique per workspace.
    #[serde(default)]
    pub identifier: String,
    /// Plain-text description. Empty rather than null when unset.
    #[serde(default)]
    pub description: String,
    /// Rich-text description as a ProseMirror document.
    #[serde(default)]
    pub description_text: Option<Value>,
    /// Rich-text description as HTML.
    #[serde(default)]
    pub description_html: Option<Value>,
    #[serde(default)]
    pub network: ProjectNetwork,
    #[serde(default)]
    pub workspace: Option<Expandable<WorkspaceLite>>,

    // People.
    #[serde(default)]
    pub project_lead: Option<Expandable<UserLite>>,
    /// Assigned to new work items that are created without an explicit assignee.
    #[serde(default)]
    pub default_assignee: Option<Expandable<UserLite>>,

    // Appearance.
    #[serde(default)]
    pub emoji: Option<String>,
    #[serde(default)]
    pub icon_prop: Option<Value>,
    /// Current icon/emoji choice: `{"in_use": "icon", "icon": {"name", "color"}}`.
    /// Generated server-side when a project is created without one.
    #[serde(default)]
    pub logo_props: Option<Value>,
    #[serde(default)]
    pub cover_image: Option<String>,
    #[serde(default)]
    pub cover_image_asset: Option<Uuid>,
    /// Resolved cover image URL, from either the uploaded asset or `cover_image`.
    #[serde(default)]
    pub cover_image_url: Option<String>,

    // Enabled features.
    #[serde(default)]
    pub module_view: bool,
    #[serde(default)]
    pub cycle_view: bool,
    #[serde(default)]
    pub issue_views_view: bool,
    #[serde(default)]
    pub page_view: bool,
    #[serde(default)]
    pub intake_view: bool,
    #[serde(default)]
    pub is_issue_type_enabled: bool,
    #[serde(default)]
    pub is_time_tracking_enabled: bool,
    /// Whether guests can see every project feature or only work items.
    #[serde(default)]
    pub guest_view_all_features: bool,

    // Defaults and automation.
    #[serde(default)]
    pub default_state: Option<Uuid>,
    /// Estimate scale in use, if any.
    #[serde(default)]
    pub estimate: Option<Uuid>,
    /// Months of inactivity before a work item is auto-archived. `0` disables it;
    /// valid range is 0–12.
    #[serde(default)]
    pub archive_in: i32,
    /// Months of inactivity before a work item is auto-closed. `0` disables it;
    /// valid range is 0–12.
    #[serde(default)]
    pub close_in: i32,
    /// IANA timezone name. Inherited from the workspace when not set explicitly.
    #[serde(default)]
    pub timezone: Option<String>,

    // Import bookkeeping.
    #[serde(default)]
    pub external_source: Option<String>,
    #[serde(default)]
    pub external_id: Option<String>,

    // Audit trail.
    #[serde(default)]
    pub created_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub updated_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub created_by: Option<Expandable<UserLite>>,
    #[serde(default)]
    pub updated_by: Option<Expandable<UserLite>>,
    #[serde(default)]
    pub archived_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub deleted_at: Option<DateTime<Utc>>,

    // Annotations present on list/retrieve only.
    #[serde(default)]
    pub total_members: Option<i64>,
    #[serde(default)]
    pub total_cycles: Option<i64>,
    #[serde(default)]
    pub total_modules: Option<i64>,
    /// Whether the authenticated user is an active member of this project.
    #[serde(default)]
    pub is_member: Option<bool>,
    /// The authenticated user's role in this project.
    #[serde(default)]
    pub member_role: Option<Role>,
    /// The authenticated user's manual ordering of this project in the sidebar;
    /// this is what the list endpoint sorts by unless `order_by` says otherwise.
    #[serde(default)]
    pub sort_order: Option<f64>,
    /// Whether the project has a public deploy board.
    #[serde(default)]
    pub is_deployed: Option<bool>,
}

impl Identified for Project {
    fn id(&self) -> Uuid {
        self.id
    }
}

/// `GET /workspaces/{slug}/projects/`
///
/// `order_by` accepts an allowlisted subset of project fields; `sort_order`
/// (the user's own sidebar ordering) is the default.
pub type ListProjectsParams = PageParams;

/// `POST /workspaces/{slug}/projects/` — 201 with a [`Project`].
///
/// `name` and `identifier` are required and neither may contain the characters
/// ``&+,:;$^}{*=?@#|'<>.()%!-``. `identifier` is uppercased and must be unique
/// within the workspace — a clash returns 409.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CreateProjectRequest {
    pub name: String,
    /// Short key prefixed to work item numbers. Max 12 characters.
    pub identifier: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Must be an active member of the workspace.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_lead: Option<Uuid>,
    /// Must be a member of the workspace.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_assignee: Option<Uuid>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub emoji: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon_prop: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cover_image: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub module_view: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cycle_view: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub issue_views_view: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_view: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub intake_view: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guest_view_all_features: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_issue_type_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_time_tracking_enabled: Option<bool>,

    /// Months of inactivity before auto-archiving. 0–12, `0` disables.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub archive_in: Option<i32>,
    /// Months of inactivity before auto-closing. 0–12, `0` disables.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub close_in: Option<i32>,
    /// IANA timezone name, e.g. `Europe/London`. Defaults to the workspace's.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timezone: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub external_source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub external_id: Option<String>,
}

/// `PATCH /workspaces/{slug}/projects/{project_id}/` — 200 with a [`Project`].
///
/// Every field is optional; only the ones you set are sent, and only those are
/// changed. Adds `default_state` and `estimate` over [`CreateProjectRequest`],
/// both of which must belong to this project.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct UpdateProjectRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identifier: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_lead: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_assignee: Option<Uuid>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub emoji: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon_prop: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cover_image: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub module_view: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cycle_view: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub issue_views_view: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_view: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub intake_view: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guest_view_all_features: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_issue_type_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_time_tracking_enabled: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub archive_in: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub close_in: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timezone: Option<String>,

    /// State new work items land in. Must be a state of this project.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_state: Option<Uuid>,
    /// Estimate scale to use. Must belong to this project.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimate: Option<Uuid>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub external_source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub external_id: Option<String>,
}

// ---------------------------------------------------------------------------
// Work items
// ---------------------------------------------------------------------------

/// A work item (an "issue" in Plane's older vocabulary, and still in its URLs).
///
/// The list endpoint hides work items that are archived, drafts, in a
/// [`StateGroup::Triage`] state, or in an archived project. Fetch those through
/// their dedicated endpoints instead.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkItem {
    pub id: Uuid,
    #[serde(default)]
    pub name: String,
    /// Per-project counter. Combined with the project identifier this is the
    /// human-facing key, e.g. `PROJ-12`.
    #[serde(default)]
    pub sequence_id: i32,
    #[serde(default)]
    pub project: Option<Expandable<ProjectLite>>,
    #[serde(default)]
    pub workspace: Option<Expandable<WorkspaceLite>>,

    /// Body as HTML. Sanitised server-side; defaults to `<p></p>`.
    #[serde(default)]
    pub description_html: String,
    /// Collaborative-editing blob. Opaque — round-trip it, don't parse it.
    #[serde(default)]
    pub description_binary: Option<Value>,

    #[serde(default)]
    pub priority: Priority,
    #[serde(default)]
    pub state: Option<Expandable<StateLite>>,
    #[serde(default)]
    pub parent: Option<Expandable<WorkItemLite>>,
    /// Work item type, when the project has typed work items enabled.
    #[serde(rename = "type", default)]
    pub work_item_type: Option<Uuid>,

    /// Estimate in points. Superseded by [`WorkItem::estimate_point`] on projects
    /// using a configured estimate scale.
    #[serde(default)]
    pub point: Option<i32>,
    #[serde(default)]
    pub estimate_point: Option<Expandable<EstimatePoint>>,

    /// Assignee UUIDs, or full users when `expand=assignees` is in play.
    ///
    /// Plane declares this write-only on some releases, so it can come back
    /// absent or null even when the work item has assignees; `None` means
    /// "not reported", not "unassigned".
    #[serde(default)]
    pub assignees: Option<Vec<Expandable<UserLite>>>,
    /// Label UUIDs, or full labels when `expand=labels` is in play. Same caveat
    /// as [`WorkItem::assignees`].
    #[serde(default)]
    pub labels: Option<Vec<Expandable<LabelLite>>>,

    #[serde(default)]
    pub start_date: Option<NaiveDate>,
    #[serde(default)]
    pub target_date: Option<NaiveDate>,
    #[serde(default)]
    pub completed_at: Option<DateTime<Utc>>,
    /// Note this is a plain date, unlike [`Project::archived_at`].
    #[serde(default)]
    pub archived_at: Option<NaiveDate>,
    #[serde(default)]
    pub last_activity_at: Option<DateTime<Utc>>,

    /// Manual ordering within a board column. Defaults to 65535 and grows by
    /// 10000 per insertion.
    #[serde(default)]
    pub sort_order: f64,
    #[serde(default)]
    pub is_draft: bool,

    #[serde(default)]
    pub external_source: Option<String>,
    #[serde(default)]
    pub external_id: Option<String>,

    #[serde(default)]
    pub created_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub updated_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub created_by: Option<Expandable<UserLite>>,
    #[serde(default)]
    pub updated_by: Option<Expandable<UserLite>>,
    #[serde(default)]
    pub deleted_at: Option<DateTime<Utc>>,
}

impl Identified for WorkItem {
    fn id(&self) -> Uuid {
        self.id
    }
}

/// `GET /workspaces/{slug}/projects/{project_id}/work-items/`
///
/// Setting both `external_id` and `external_source` looks up the single work item
/// imported under that pair instead of listing.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ListWorkItemsParams {
    // Kept flat rather than embedding `PageParams`: `#[serde(flatten)]` forces
    // map serialisation, which the urlencoded query serialiser handles poorly.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub per_page: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expand: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fields: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub external_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub external_source: Option<String>,
}

/// `POST /workspaces/{slug}/projects/{project_id}/work-items/` — 201 with a
/// [`WorkItem`].
///
/// `name` is the only required field. Assignees must be project members with at
/// least [`Role::Member`]; labels, state and estimate point must belong to this
/// project; `parent` must be a work item in the same project. Reusing an
/// `external_source`/`external_id` pair returns 409 with the existing `id`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CreateWorkItemRequest {
    pub name: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub description_html: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<Priority>,
    /// State UUID. Falls back to the project's default state.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<Uuid>,
    /// Work item type UUID. Falls back to the project's default type.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub type_id: Option<Uuid>,

    /// Omit to fall back to the project's `default_assignee`. An empty vector
    /// also suppresses that fallback.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assignees: Option<Vec<Uuid>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub labels: Option<Vec<Uuid>>,

    /// Estimate in points, 0–12.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub point: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimate_point: Option<Uuid>,

    /// Must not be later than `target_date`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_date: Option<NaiveDate>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_date: Option<NaiveDate>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort_order: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_draft: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub external_source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub external_id: Option<String>,

    /// Backdates the work item. Only useful when importing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<DateTime<Utc>>,
    /// Attributes authorship to another user. Only useful when importing;
    /// defaults to the token's owner.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by: Option<Uuid>,
}

/// `PATCH /workspaces/{slug}/projects/{project_id}/work-items/{id}/` — 200 with a
/// [`WorkItem`].
///
/// Every field is optional; only the ones you set are sent, and only those are
/// changed. Sending `assignees` or `labels` replaces the whole set rather than
/// adding to it — send the full list, and `Some(vec![])` to clear.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct UpdateWorkItemRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description_html: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<Priority>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub type_id: Option<Uuid>,

    /// Replaces the existing assignees. `Some(vec![])` unassigns everyone.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assignees: Option<Vec<Uuid>>,
    /// Replaces the existing labels. `Some(vec![])` removes them all.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub labels: Option<Vec<Uuid>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub point: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimate_point: Option<Uuid>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_date: Option<NaiveDate>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_date: Option<NaiveDate>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort_order: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_draft: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub external_source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub external_id: Option<String>,
}

/// `GET /workspaces/{slug}/work-items/search/`
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SearchWorkItemsParams {
    /// Matched against work item name, sequence id and project identifier.
    pub search: String,
    /// Restrict to one project. Required unless `workspace_search` is on.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<Uuid>,
    /// Search the whole workspace rather than a single project.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_search: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
}

/// Search results. Note this endpoint is *not* paginated — it returns a bare
/// object rather than a [`Paginated`] envelope.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SearchWorkItems {
    #[serde(default = "Vec::new")]
    pub issues: Vec<WorkItemSearchResult>,
}

/// One search hit. Deliberately thin — refetch the work item for the full record.
///
/// This endpoint returns raw database rows rather than running them through a
/// serializer, so the field names carry Django's `__` join syntax and the types
/// are whatever the column is. Plane's OpenAPI annotation points at a serializer
/// that is never actually applied, and describes `sequence_id` as a string; on
/// the wire it is an integer.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct WorkItemSearchResult {
    pub id: Uuid,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub sequence_id: i32,
    /// The owning project's identifier, e.g. `PROJ`.
    #[serde(rename = "project__identifier", default)]
    pub project_identifier: String,
    #[serde(default)]
    pub project_id: Option<Uuid>,
    #[serde(rename = "workspace__slug", default)]
    pub workspace_slug: String,
    /// Work item type, when the project has typed work items enabled.
    #[serde(default)]
    pub type_id: Option<Uuid>,
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// The body of a non-2xx response.
///
/// Plane is inconsistent here: validation failures (400) come back as a map of
/// field name to messages, while most other failures use a flat `error` string.
/// A 409 from a duplicate `external_id` additionally carries the `id` of the
/// existing record.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ApiError {
    #[serde(default)]
    pub error: Option<String>,
    /// DRF's standard key, used for authentication and permission failures —
    /// a 403 arrives as `{"detail": "Given API token is not valid"}` rather
    /// than using `error`.
    #[serde(default)]
    pub detail: Option<String>,
    /// Used by the rate limiter, which answers with
    /// `{"error_code": 5900, "error_message": "RATE_LIMIT_EXCEEDED"}`.
    #[serde(default)]
    pub error_message: Option<String>,
    /// Numeric companion to [`ApiError::error_message`].
    #[serde(default)]
    pub error_code: Option<i64>,
    /// Present on 409 conflicts: the id of the record that already exists.
    #[serde(default)]
    pub id: Option<String>,
    /// Per-field validation messages, for 400 responses.
    #[serde(flatten)]
    pub fields: std::collections::BTreeMap<String, Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sparse_patch_bodies_only_carry_set_fields() {
        let body = UpdateWorkItemRequest {
            priority: Some(Priority::High),
            ..Default::default()
        };
        assert_eq!(
            serde_json::to_string(&body).unwrap(),
            r#"{"priority":"high"}"#
        );
    }

    #[test]
    fn network_round_trips_through_its_integer_form() {
        assert_eq!(
            serde_json::from_str::<ProjectNetwork>("0").unwrap(),
            ProjectNetwork::Secret
        );
        assert_eq!(
            serde_json::to_string(&ProjectNetwork::Public).unwrap(),
            "2"
        );
        assert!(serde_json::from_str::<ProjectNetwork>("1").is_err());
    }

    #[test]
    fn expandable_reads_both_bare_ids_and_nested_objects() {
        let bare: Expandable<StateLite> =
            serde_json::from_str(r#""550e8400-e29b-41d4-a716-446655440000""#).unwrap();
        assert!(bare.expanded().is_none());

        let nested: Expandable<StateLite> = serde_json::from_str(
            r##"{"id":"550e8400-e29b-41d4-a716-446655440000","name":"In Progress",
                 "color":"#F59E0B","group":"started"}"##,
        )
        .unwrap();
        assert_eq!(nested.expanded().unwrap().group, StateGroup::Started);
        assert_eq!(bare.id(), nested.id());
    }

    #[test]
    fn work_items_decode_from_a_trimmed_fields_response() {
        // `?fields=id,name` strips everything else off the payload.
        let item: WorkItem = serde_json::from_str(
            r#"{"id":"550e8400-e29b-41d4-a716-446655440000","name":"Ship it"}"#,
        )
        .unwrap();
        assert_eq!(item.priority, Priority::None);
        assert!(item.assignees.is_none());
    }

    #[test]
    fn paginated_stops_handing_out_cursors_at_the_last_page() {
        let page: Paginated<Project> = serde_json::from_str(
            r#"{"grouped_by":null,"sub_grouped_by":null,"total_count":1,"next_cursor":"20:1:0",
                "prev_cursor":"20:0:0","next_page_results":false,"prev_page_results":false,
                "count":1,"total_pages":1,"total_results":1,"extra_stats":null,"results":[]}"#,
        )
        .unwrap();
        assert_eq!(page.next(), None);
    }
}
