//! The workspace's projects, and the counts shown alongside them.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use color_eyre::eyre::Result;
use ratatui::widgets::TableState;
use uuid::Uuid;

use crate::api::Client;
use crate::api::types::{PageParams, Project};

use super::{LoadingState, restored_selection};

#[derive(Debug, Clone, Default)]
pub struct ListProjectsWidget {
    pub state: Arc<RwLock<ListProjectsState>>,
}

#[derive(Debug, Default)]
pub(crate) struct ListProjectsState {
    pub projects: Vec<Project>,
    pub loading_state: LoadingState,
    pub table_state: TableState,
    /// Work item counts, keyed by project. Each one costs its own request, so
    /// they are cached across refreshes and only fetched for projects not seen
    /// before -- a refresh of an unchanged workspace stays a single request.
    pub work_item_counts: HashMap<Uuid, i64>,
    cursor: Option<String>,
}

impl ListProjectsWidget {
    /// Re-fetch the first page of projects.
    ///
    /// Deliberately leaves the current contents and selection alone. The rows
    /// already on screen stay put until their replacements arrive, so tabbing
    /// back here does not blank the pane for the length of a round trip -- the
    /// loading indicator in the title is the only sign a refresh is running.
    pub(crate) fn run(&self, api: &Client) {
        let this = self.clone(); // clone the widget to pass to the background task
        tokio::spawn(this.fetch_projects(api.clone()));
    }

    /// Runs as a detached task, so there is nowhere to return an error *to*.
    /// Failures land in [`LoadingState::Error`] for the UI to render instead.
    async fn fetch_projects(self, api: Client) {
        self.set_loading_state(LoadingState::Loading);
        match self.refresh_projects(&api).await {
            Ok(()) => self.set_loading_state(LoadingState::Loaded),
            Err(error) => self.set_loading_state(LoadingState::Error(error.to_string())),
        }
    }

    /// The fallible half, split out so `?` has somewhere to go.
    ///
    /// Swaps the whole list in one assignment once the response is in hand, so
    /// there is no window where the pane is empty.
    async fn refresh_projects(&self, api: &Client) -> Result<()> {
        // A refresh always starts from the first page. `cursor` is still stored
        // below so a future "load more" has somewhere to resume from.
        let params = PageParams::default();

        let page = api.list_projects(&params).await?;
        // Read the cursor before `results` is moved out of `page`.
        let next_cursor = page.next().map(str::to_string);

        let mut state = self.state.write().unwrap();

        // Which project the cursor was sitting on, before the contents change.
        let selected_id = state
            .table_state
            .selected()
            .and_then(|index| state.projects.get(index))
            .map(|project| project.id);

        state.projects = page.results;
        state.cursor = next_cursor;

        let next_selection = restored_selection(&state.projects, selected_id);
        state.table_state.select(next_selection);

        // Counts are not in the list payload. Fetch only the ones we do not
        // already hold, so tabbing back and forth costs nothing extra.
        let missing: Vec<Uuid> = state
            .projects
            .iter()
            .map(|project| project.id)
            .filter(|id| !state.work_item_counts.contains_key(id))
            .collect();
        drop(state);

        if !missing.is_empty() {
            tokio::spawn(self.clone().fetch_missing_counts(api.clone(), missing));
        }

        Ok(())
    }

    /// Fills in work item counts one project at a time.
    ///
    /// Sequential rather than concurrent on purpose: a workspace with thirty
    /// projects would otherwise fire thirty requests at once and spend most of
    /// the 60-per-minute budget on a single refresh. A count that fails to
    /// arrive renders as a placeholder rather than failing the list.
    async fn fetch_missing_counts(self, api: Client, ids: Vec<Uuid>) {
        for id in ids {
            if let Ok(summary) = api.project_summary(id).await {
                self.state
                    .write()
                    .unwrap()
                    .work_item_counts
                    .insert(id, summary.counts.issues);
            }
        }
    }

    pub(crate) fn set_loading_state(&self, state: LoadingState) {
        self.state.write().unwrap().loading_state = state;
    }

    pub(crate) fn scroll_down(&self) {
        self.state.write().unwrap().table_state.scroll_down_by(1);
    }

    pub(crate) fn scroll_up(&self) {
        self.state.write().unwrap().table_state.scroll_up_by(1);
    }

    /// Whether the list is showing data that is current. Input is ignored while
    /// it is not, so a keypress cannot act on rows about to be replaced.
    pub(crate) fn is_ready(&self) -> bool {
        self.state.read().unwrap().loading_state == LoadingState::Loaded
    }

    /// The project under the cursor. Cloned rather than borrowed so the caller
    /// is not holding a lock guard while it decides what to do next.
    pub(crate) fn selected_project(&self) -> Option<Project> {
        let state = self.state.read().unwrap();
        state
            .table_state
            .selected()
            .and_then(|index| state.projects.get(index))
            .cloned()
    }
}
