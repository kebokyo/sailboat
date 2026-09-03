//! The description pane: a work item's prose, rendered from Plane's HTML.
//!
//! Reads from the same [`WorkItemDetailsWidget`] as the attributes table -- the
//! description arrives on the same record, so there is nothing extra to fetch.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    prelude::Stylize,
    style::Color,
    text::{Line, Span},
    widgets::{Block, Paragraph, Widget, Wrap},
};

use crate::app::widgets::{LoadingState, WorkItemDetailsWidget};
use crate::ui::render::description_to_text;

/// Renders the description of whatever [`WorkItemDetailsWidget`] currently holds.
///
/// A separate type from the attributes table because the two draw different
/// things from the same widget, and `Widget` can only be implemented once per
/// type.
pub struct Description<'a>(pub &'a WorkItemDetailsWidget, pub bool);

impl Widget for Description<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let Description(widget, focused) = self;
        let state = widget.state.read().unwrap();

        // Same rule as every other pane: focus alone is not enough.
        let ready = state.loading_state == LoadingState::Loaded;
        let dimmed = !(focused && ready);
        let title = if dimmed { Color::DarkGray } else { Color::White };
        let block = Block::new().title(Span::from("description").fg(title));

        let text = match &state.work_item {
            Some(item) => {
                let rendered = description_to_text(&item.description_html, dimmed);
                // Emptiness has to be judged after rendering, not before: an
                // untouched editor stores `<p></p>`, which is not an empty
                // string but produces no lines.
                if rendered.lines.is_empty() {
                    Line::from(Span::from("no description").fg(Color::DarkGray)).into()
                } else {
                    rendered
                }
            }
            None => return Widget::render(block, area, buf),
        };

        // Wrapped rather than truncated: prose is the one thing here that has to
        // reflow to the pane it is given.
        Paragraph::new(text)
            .block(block)
            .wrap(Wrap { trim: false })
            .render(area, buf);
    }
}
