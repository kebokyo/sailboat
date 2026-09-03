use std::{collections::HashMap, sync::{Arc, RwLock}, time::Duration};

use color_eyre::eyre::Result;
use crossterm::event::{Event, EventStream, KeyCode};
use ratatui::{DefaultTerminal, widgets::TableState};
use tokio_stream::StreamExt;

use uuid::Uuid;

use crate::{api::{Client, types::{Identified, ListWorkItemsParams, PageParams, Project, WorkItem}}, config::Config, ui::ui};

/// The different views that sailboat displays.
#[derive(Debug, Default)]
pub enum CurrentScreen {
    /// The main view: projects on left, work items on right. Starts on the projects side.
    #[default]
    MainProjectsView,
    /// If the viewport is thin, the project list gets its own view.
    MainWorkItemsView,
    /// Viewing a specific work item's details.
    WorkItemDetailsView,
    /// Editing an attribute of a work item. Editing description is its own view: [`CurrentScreen::EditDescriptionView`]
    EditWorkItemDialog,
    /// Editing the description of a work item using Markdown.
    EditDescriptionView,
}

/// The main driver of sailboat's functionality.
#[derive(Debug, Default)]
pub struct App {
    pub current_workspace: String,
    pub current_project: Option<Project>,
    pub current_work_item: Option<WorkItem>,
    pub current_screen: CurrentScreen,

    pub list_projects: ListProjectsWidget,
    pub list_work_items: ListWorkItemsWidget,
    should_quit: bool
}

impl App {
    const FRAMES_PER_SECOND: f32 = 60.0;
    
    pub async fn run(mut self, mut terminal: DefaultTerminal) -> Result<()> {
        let config = Config::load()?;
        let api = Client::new(&config)?;

        self.current_workspace = config.workspace.slug;

        self.list_projects.run(&api);

        let period = Duration::from_secs_f32(1.0 / Self::FRAMES_PER_SECOND);
        let mut interval = tokio::time::interval(period);
        let mut events = EventStream::new();

        while !self.should_quit {
            tokio::select! {
                _ = interval.tick() => { terminal.draw(|frame| ui(frame, &self))?; },
                Some(Ok(event)) = events.next() => self.handle_event(&event, &api),
            }
        }
        Ok(())
    }

    fn handle_event(&mut self, event: &Event, api: &Client) {
        if let Some(key) = event.as_key_press_event() {
            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
                KeyCode::Char('j') | KeyCode::Down => self.scroll_down(),
                KeyCode::Char('k') | KeyCode::Up => self.scroll_up(),
                KeyCode::Char('h') | KeyCode::Left => self.scroll_left(&api),
                KeyCode::Char('l') | KeyCode::Right => self.scroll_right(&api),
                KeyCode::Enter => self.confirm(&api),
                KeyCode::Backspace => self.cancel(&api),
                _ => {}
            }
        }
    }

    fn confirm(&mut self, api: &Client) {
        match self.current_screen {
            // Load the highlighted project's work items. Chosen because it
            // keeps fetches deliberate -- loading on every scroll step would
            // fire a request per keypress and burn the 60/minute budget.
            CurrentScreen::MainProjectsView if self.list_projects.is_ready() => {
                if let Some(project) = self.list_projects.selected_project() {
                    self.list_work_items.run(api, project.id);
                    self.current_project = Some(project);
                    self.current_screen = CurrentScreen::MainWorkItemsView;
                }
            }
            _ => {}
        }
    }

    fn cancel(&mut self, api: &Client) {
        match self.current_screen {
            CurrentScreen::MainWorkItemsView => {
                self.list_projects.run(api);
                // `current_project` is left as-is: the work items pane still
                // shows that project, and the highlight in the list survives the
                // refresh, so clearing it here would contradict both.
                self.current_screen = CurrentScreen::MainProjectsView;
            }
            _ => {}
        }
    }

    // Scrolling and selection are ignored while the focused list is refreshing.
    // Its rows are about to be replaced, and acting on them risks opening a
    // project that no longer exists by the time the response lands.
    fn scroll_down(&mut self) {
        match self.current_screen {
            CurrentScreen::MainProjectsView if self.list_projects.is_ready() => {
                self.list_projects.scroll_down()
            }
            CurrentScreen::MainWorkItemsView if self.list_work_items.is_ready() => {
                self.list_work_items.scroll_down()
            }
            _ => {}
        }
    }

    fn scroll_up(&mut self) {
        match self.current_screen {
            CurrentScreen::MainProjectsView if self.list_projects.is_ready() => {
                self.list_projects.scroll_up()
            }
            CurrentScreen::MainWorkItemsView if self.list_work_items.is_ready() => {
                self.list_work_items.scroll_up()
            }
            _ => {}
        }
    }

    fn scroll_left(&mut self, api: &Client) {
        match self.current_screen {
            CurrentScreen::MainWorkItemsView => {
                self.list_projects.run(api);
                // `current_project` is left as-is: the work items pane still
                // shows that project, and the highlight in the list survives the
                // refresh, so clearing it here would contradict both.
                self.current_screen = CurrentScreen::MainProjectsView;
            }
            _ => {}
        }
    }
    
    fn scroll_right(&mut self, api: &Client) {
        match self.current_screen {
            // Load the highlighted project's work items. Chosen because it
            // keeps fetches deliberate -- loading on every scroll step would
            // fire a request per keypress and burn the 60/minute budget.
            CurrentScreen::MainProjectsView if self.list_projects.is_ready() => {
                if let Some(project) = self.list_projects.selected_project() {
                    self.list_work_items.run(api, project.id);
                    self.current_project = Some(project);
                    self.current_screen = CurrentScreen::MainWorkItemsView;
                }
            }
            _ => {}
        }
    }
}

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

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) enum LoadingState {
    #[default]
    Idle,
    Loading,
    Loaded,
    Error(String),
}

impl ListProjectsWidget {
    /// Re-fetch the first page of projects.
    ///
    /// Deliberately leaves the current contents and selection alone. The rows
    /// already on screen stay put until their replacements arrive, so tabbing
    /// back here does not blank the pane for the length of a round trip -- the
    /// loading indicator in the title is the only sign a refresh is running.
    fn run(&self, api: &Client) {
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

    fn set_loading_state(&self, state: LoadingState) {
        self.state.write().unwrap().loading_state = state;
    }

    fn scroll_down(&self) {
        self.state.write().unwrap().table_state.scroll_down_by(1);
    }

    fn scroll_up(&self) {
        self.state.write().unwrap().table_state.scroll_up_by(1);
    }

    /// Whether the list is showing data that is current. Input is ignored while
    /// it is not, so a keypress cannot act on rows about to be replaced.
    fn is_ready(&self) -> bool {
        self.state.read().unwrap().loading_state == LoadingState::Loaded
    }

    /// The project under the cursor. Cloned rather than borrowed so the caller
    /// is not holding a lock guard while it decides what to do next.
    fn selected_project(&self) -> Option<Project> {
        let state = self.state.read().unwrap();
        state
            .table_state
            .selected()
            .and_then(|index| state.projects.get(index))
            .cloned()
    }
}

/// The work items belonging to one project.
///
/// Mirrors [`ListProjectsWidget`], with the difference that its contents are
/// scoped to a project and are replaced wholesale when the selection changes.
#[derive(Debug, Clone, Default)]
pub struct ListWorkItemsWidget {
    pub state: Arc<RwLock<ListWorkItemsState>>,
}

#[derive(Debug, Default)]
pub(crate) struct ListWorkItemsState {
    pub work_items: Vec<WorkItem>,
    pub loading_state: LoadingState,
    pub table_state: TableState,
    /// Which project these belong to, so paging can continue against it.
    project_id: Option<Uuid>,
    cursor: Option<String>,
}

impl ListWorkItemsWidget {
    /// Point the widget at a project and fetch its first page.
    ///
    /// Switching to a *different* project wipes the pane first, since showing
    /// one project's items under another's heading would be wrong. Refreshing
    /// the project already on screen leaves the rows in place until their
    /// replacements arrive, so the pane never blanks.
    fn run(&self, api: &Client, project_id: Uuid) {
        {
            let mut state = self.state.write().unwrap();
            if state.project_id != Some(project_id) {
                state.work_items.clear();
                state.cursor = None;
                state.table_state.select(None);
                state.project_id = Some(project_id);
            }
        }

        let this = self.clone();
        tokio::spawn(this.fetch_work_items(api.clone(), project_id));
    }

    /// Detached like its counterpart, so failures land in [`LoadingState::Error`].
    async fn fetch_work_items(self, api: Client, project_id: Uuid) {
        self.set_loading_state(LoadingState::Loading);
        match self.load_work_items(&api, project_id).await {
            Ok(()) => self.set_loading_state(LoadingState::Loaded),
            Err(error) => self.set_loading_state(LoadingState::Error(error.to_string())),
        }
    }

    async fn load_work_items(&self, api: &Client, project_id: Uuid) -> Result<()> {
        let mut params = ListWorkItemsParams {
            // Inline each item's state so a row can show its name without a
            // follow-up request per row.
            expand: Some("state".to_string()),
            ..Default::default()
        };
        params.cursor = self.state.read().unwrap().cursor.clone();

        let page = api.list_work_items(project_id, &params).await?;
        let next_cursor = page.next().map(str::to_string);

        let mut state = self.state.write().unwrap();
        // The selection may have moved on while this was in flight; anything that
        // arrives for a project we are no longer showing gets dropped.
        if state.project_id != Some(project_id) {
            return Ok(());
        }
        // Same reasoning as the projects list: swap the contents in one
        // assignment and put the cursor back on the work item it was on.
        let selected_id = state
            .table_state
            .selected()
            .and_then(|index| state.work_items.get(index))
            .map(|item| item.id);

        state.work_items = page.results;
        state.cursor = next_cursor;

        let next_selection = restored_selection(&state.work_items, selected_id);
        state.table_state.select(next_selection);
        Ok(())
    }

    fn set_loading_state(&self, state: LoadingState) {
        self.state.write().unwrap().loading_state = state;
    }

    pub fn scroll_down(&self) {
        self.state.write().unwrap().table_state.scroll_down_by(1);
    }

    pub fn scroll_up(&self) {
        self.state.write().unwrap().table_state.scroll_up_by(1);
    }

    /// See [`ListProjectsWidget::is_ready`].
    fn is_ready(&self) -> bool {
        self.state.read().unwrap().loading_state == LoadingState::Loaded
    }
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

    /// Most of `Project` is `#[serde(default)]`, so a test fixture only needs
    /// the fields under test.
    fn project(id: Uuid, name: &str) -> Project {
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
