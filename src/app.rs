use std::{sync::{Arc, RwLock}, time::Duration};

use color_eyre::eyre::Result;
use crossterm::event::{Event, EventStream, KeyCode};
use ratatui::{DefaultTerminal, widgets::ListState};
use tokio_stream::StreamExt;

use uuid::Uuid;

use crate::{api::{Client, types::{ListWorkItemsParams, PageParams, Project, WorkItem}}, config::{Config, Workspace}, ui::ui};

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
            CurrentScreen::MainProjectsView => {
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
                self.current_project = None;
                self.current_screen = CurrentScreen::MainProjectsView;
            }
            _ => {}
        }
    }

    fn scroll_down(&mut self) {
        match self.current_screen {
            CurrentScreen::MainProjectsView => self.list_projects.scroll_down(),
            CurrentScreen::MainWorkItemsView => self.list_work_items.scroll_down(),
            _ => {}
        }
    }

    fn scroll_up(&mut self) {
        match self.current_screen {
            CurrentScreen::MainProjectsView => self.list_projects.scroll_up(),
            CurrentScreen::MainWorkItemsView => self.list_work_items.scroll_up(),
            _ => {}
        }
    }

    fn scroll_left(&mut self, api: &Client) {
        match self.current_screen {
            CurrentScreen::MainWorkItemsView => {
                self.list_projects.run(api);
                self.current_project = None;
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
            CurrentScreen::MainProjectsView => {
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
    pub list_state: ListState,
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
    fn run(&self, api: &Client) {
        {
            let mut state = self.state.write().unwrap();
            state.projects.clear();
            state.cursor = None;
            state.list_state.select(None);
        }

        let this = self.clone(); // clone the widget to pass to the background task
        tokio::spawn(this.fetch_projects(api.clone()));
    }

    /// Runs as a detached task, so there is nowhere to return an error *to*.
    /// Failures land in [`LoadingState::Error`] for the UI to render instead.
    async fn fetch_projects(self, api: Client) {
        self.set_loading_state(LoadingState::Loading);
        match self.load_projects(&api).await {
            Ok(()) => self.set_loading_state(LoadingState::Loaded),
            Err(error) => self.set_loading_state(LoadingState::Error(error.to_string())),
        }
    }

    /// The fallible half, split out so `?` has somewhere to go.
    async fn load_projects(&self, api: &Client) -> Result<()> {
        let mut params = PageParams::default();
        // Cloned so the guard is dropped at the end of this statement. A std guard
        // held across the await below would make this future non-Send and fail to
        // spawn -- the compiler enforces what was only a convention before.
        params.cursor = self.state.read().unwrap().cursor.clone();

        let page = api.list_projects(&params).await?;
        // Read the cursor before `results` is moved out of `page`.
        let next_cursor = page.next().map(str::to_string);

        let mut state = self.state.write().unwrap();
        state.projects.extend(page.results);
        state.cursor = next_cursor;
        state.list_state.select_first();
        Ok(())
    }

    fn set_loading_state(&self, state: LoadingState) {
        self.state.write().unwrap().loading_state = state;
    }

    fn scroll_down(&self) {
        self.state.write().unwrap().list_state.scroll_down_by(1);
    }

    fn scroll_up(&self) {
        self.state.write().unwrap().list_state.scroll_up_by(1);
    }

    /// The project under the cursor. Cloned rather than borrowed so the caller
    /// is not holding a lock guard while it decides what to do next.
    fn selected_project(&self) -> Option<Project> {
        let state = self.state.read().unwrap();
        state
            .list_state
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
    pub list_state: ListState,
    /// Which project these belong to, so paging can continue against it.
    project_id: Option<Uuid>,
    cursor: Option<String>,
}

impl ListWorkItemsWidget {
    /// Point the widget at a project and fetch its first page. Any items from a
    /// previously selected project are discarded first, so a slow response can
    /// never append onto the wrong list.
    fn run(&self, api: &Client, project_id: Uuid) {
        {
            let mut state = self.state.write().unwrap();
            state.work_items.clear();
            state.cursor = None;
            state.list_state.select(None);
            state.project_id = Some(project_id);
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
        state.work_items.extend(page.results);
        state.cursor = next_cursor;
        state.list_state.select_first();
        Ok(())
    }

    fn set_loading_state(&self, state: LoadingState) {
        self.state.write().unwrap().loading_state = state;
    }

    pub fn scroll_down(&self) {
        self.state.write().unwrap().list_state.scroll_down_by(1);
    }

    pub fn scroll_up(&self) {
        self.state.write().unwrap().list_state.scroll_up_by(1);
    }
}