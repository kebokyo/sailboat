//! Widgets: the pieces of state that fetch from Plane and hold what came back.
//!
//! Each one owns an `Arc<RwLock<..>>` of its own state, is cheap to clone, and
//! runs its fetch as a detached task. Because a detached task has nowhere to
//! return an error to, failures land in [`LoadingState::Error`] for the UI to
//! render. The render half of each widget lives under `ui::widgets`.

use uuid::Uuid;

use crate::api::types::Identified;

pub mod list_projects;
pub mod list_work_items;
pub mod project_details;
pub mod work_item_details;

pub use list_projects::ListProjectsWidget;
pub use list_work_items::ListWorkItemsWidget;
pub use project_details::ProjectDetailsWidget;
pub use work_item_details::WorkItemDetailsWidget;


/// Where a widget is in its fetch cycle. Shared by every widget so the UI has
/// one rule for "is this current?".
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) enum LoadingState {
    #[default]
    Idle,
    Loading,
    Loaded,
    Error(String),
}

/// Where the cursor belongs after a refresh has swapped a list's contents out.
///
/// Matches on project id rather than reusing the old index: a refresh can add,
/// remove or reorder rows, and an index would quietly leave the cursor pointing
/// at a different project than the one the user chose. Falls back to the top of
/// the list when there was no selection, or when the selected project is gone.
fn restored_selection<T: Identified>(items: &[T], previously_selected: Option<Uuid>) -> Option<usize> {
    let found = previously_selected.and_then(|id| items.iter().position(|item| item.id() == id));

    match found {
        Some(index) => Some(index),
        None if items.is_empty() => None,
        None => Some(0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::types::Project;

    /// Most of `Project` is `#[serde(default)]`, so a test fixture only needs
    /// the fields under test.
    pub(crate) fn project(id: Uuid, name: &str) -> Project {
        serde_json::from_value(serde_json::json!({ "id": id, "name": name }))
            .expect("minimal project should decode")
    }

    #[test]
    fn selection_follows_the_project_when_a_refresh_reorders_the_list() {
        let (a, b, c) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
        // The user was on `b`, at index 1. A refresh brings it back at index 2.
        let refreshed = vec![project(c, "c"), project(a, "a"), project(b, "b")];
        assert_eq!(restored_selection(&refreshed, Some(b)), Some(2));
    }

    #[test]
    fn selection_stays_put_when_nothing_moved() {
        let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
        let refreshed = vec![project(a, "a"), project(b, "b")];
        assert_eq!(restored_selection(&refreshed, Some(b)), Some(1));
    }

    #[test]
    fn a_deleted_project_drops_the_cursor_to_the_top_rather_than_out_of_bounds() {
        let (a, gone) = (Uuid::new_v4(), Uuid::new_v4());
        let refreshed = vec![project(a, "a")];
        assert_eq!(restored_selection(&refreshed, Some(gone)), Some(0));
    }

    #[test]
    fn the_first_load_selects_the_top() {
        let refreshed = vec![project(Uuid::new_v4(), "a")];
        assert_eq!(restored_selection(&refreshed, None), Some(0));
    }

    #[test]
    fn a_list_is_only_ready_once_a_refresh_has_landed() {
        let widget = ListProjectsWidget::default();
        // Nothing fetched yet, so input must not act on an empty list.
        assert!(!widget.is_ready());

        widget.set_loading_state(LoadingState::Loading);
        assert!(!widget.is_ready(), "rows are mid-replacement");

        widget.set_loading_state(LoadingState::Loaded);
        assert!(widget.is_ready());

        widget.set_loading_state(LoadingState::Error("boom".to_string()));
        assert!(!widget.is_ready(), "a failed refresh leaves stale rows on screen");
    }

    #[test]
    fn an_empty_workspace_selects_nothing() {
        let empty: [Project; 0] = [];
        assert_eq!(restored_selection(&empty, None), None);
        assert_eq!(restored_selection(&empty, Some(Uuid::new_v4())), None);
    }
}
