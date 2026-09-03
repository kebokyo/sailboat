//! Turning a work item's description into something a terminal can show.
//!
//! Plane stores descriptions as HTML produced by its web editor. The path here
//! is HTML -> Markdown -> styled [`Text`]:
//!
//! * **HTML to Markdown** flattens the editor's markup (and its own classes,
//!   like `editor-paragraph-block`) into a form with no attributes to carry.
//! * **Markdown to `Text`** maps structure onto colour and weight, since a
//!   terminal cannot vary font size and `**bold**` markers on screen would be
//!   noise in a read-only view.
//!
//! Going through Markdown rather than straight to text is deliberate: it is the
//! form the editor will eventually round-trip through, so display and editing
//! stay in agreement about what the document is.
//!
//! Known loss: HTML that Markdown has no spelling for is dropped. Plane's custom
//! nodes -- mentions, image components -- are the ones that matter, and they
//! vanish here. That is acceptable for reading and *not* acceptable for editing;
//! an editor would need to preserve unknown nodes as opaque tokens instead.

use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
};

/// Colour for headings.
const HEADING: Color = Color::LightMagenta;
/// Colour for inline and block code.
const CODE: Color = Color::Cyan;
/// Colour for link text and list bullets.
const ACCENT: Color = Color::Blue;
/// Colour for block quotes.
const QUOTE: Color = Color::DarkGray;

/// Render a description straight from what the API returned.
pub fn description_to_text(html: &str, dimmed: bool) -> Text<'static> {
    markdown_to_text(&html_to_markdown(html), dimmed)
}

/// Stage one: Plane's editor HTML to Markdown.
///
/// Returns the input unchanged if it does not parse, which is better than
/// showing nothing -- a description that renders as its raw source is still
/// readable, and the failure is visible rather than silent.
pub fn html_to_markdown(html: &str) -> String {
    htmd::convert(html).unwrap_or_else(|_| html.to_string())
}

/// Stage two: Markdown to styled [`Text`].
///
/// `dimmed` collapses every style to grey, matching how the list panes signal
/// that a pane is not the one being driven.
pub fn markdown_to_text(markdown: &str, dimmed: bool) -> Text<'static> {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_TASKLISTS);

    let mut writer = Writer::new(dimmed);
    for event in Parser::new_ext(markdown, options) {
        writer.handle(event);
    }
    writer.finish()
}

/// Accumulates spans into lines as the parser walks the document.
struct Writer {
    dimmed: bool,
    lines: Vec<Line<'static>>,
    /// Spans of the line currently being built.
    pending: Vec<Span<'static>>,
    /// Style modifiers currently open, as a stack, so nested emphasis inside a
    /// heading keeps both.
    style: Style,
    /// One entry per open list; `Some(n)` counts an ordered list's next number.
    lists: Vec<Option<u64>>,
    /// Depth of open block quotes, which prefix their lines.
    quote_depth: usize,
    in_code_block: bool,
    /// Set between emitting a list marker and the first text of that item.
    ///
    /// A "loose" list wraps each item's text in a paragraph, and a paragraph
    /// would normally start a new block. Without this the bullet ends up alone
    /// on its own line with a blank beneath it.
    at_item_start: bool,
}

impl Writer {
    fn new(dimmed: bool) -> Self {
        Writer {
            dimmed,
            lines: Vec::new(),
            pending: Vec::new(),
            style: Style::default(),
            lists: Vec::new(),
            quote_depth: 0,
            in_code_block: false,
            at_item_start: false,
        }
    }

    /// Everything renders grey when the pane is not focused, so a single check
    /// here keeps the rest of this file from repeating it.
    fn paint(&self, style: Style) -> Style {
        if self.dimmed {
            Style::default().fg(Color::DarkGray)
        } else {
            style
        }
    }

    fn push(&mut self, text: String) {
        let style = self.paint(self.style);
        self.pending.push(Span::styled(text, style));
    }

    /// End the current line, if it has anything on it.
    fn break_line(&mut self) {
        if self.pending.is_empty() {
            return;
        }
        let mut spans = Vec::new();
        if self.quote_depth > 0 {
            let marker = "> ".repeat(self.quote_depth);
            spans.push(Span::styled(marker, self.paint(Style::default().fg(QUOTE))));
        }
        spans.append(&mut self.pending);
        self.lines.push(Line::from(spans));
    }

    /// A blank line between blocks, collapsed so runs never stack up.
    fn blank_line(&mut self) {
        self.break_line();
        if self.lines.last().is_some_and(|line| !line.spans.is_empty()) {
            self.lines.push(Line::default());
        }
    }

    fn handle(&mut self, event: Event<'_>) {
        match event {
            Event::Start(tag) => self.start(tag),
            Event::End(tag) => self.end(tag),

            Event::Text(text) => {
                if self.in_code_block {
                    // Code blocks keep their own line breaks.
                    for (index, segment) in text.split('\n').enumerate() {
                        if index > 0 {
                            self.break_line();
                        }
                        if !segment.is_empty() {
                            self.push(segment.to_string());
                        }
                    }
                } else {
                    self.push(text.to_string());
                }
            }

            Event::Code(code) => {
                let style = self.paint(self.style.fg(CODE));
                self.pending.push(Span::styled(format!("`{code}`"), style));
            }

            Event::SoftBreak => self.push(" ".to_string()),
            Event::HardBreak => self.break_line(),

            Event::Rule => {
                self.blank_line();
                self.lines.push(Line::from(Span::styled(
                    "─".repeat(24),
                    self.paint(Style::default().fg(QUOTE)),
                )));
                self.lines.push(Line::default());
            }

            Event::TaskListMarker(done) => {
                let marker = if done { "[x] " } else { "[ ] " };
                let style = self.paint(Style::default().fg(ACCENT));
                self.pending.push(Span::styled(marker, style));
            }

            // Raw HTML that survived the Markdown conversion, plus footnotes and
            // maths, have no sensible terminal spelling. Dropping them keeps the
            // prose readable rather than littering it with markup.
            _ => {}
        }
    }

    fn start(&mut self, tag: Tag<'_>) {
        match tag {
            Tag::Paragraph => {
                // Inside a list item the paragraph *is* the item's text, so it
                // continues the line the marker just started.
                if self.at_item_start {
                    self.at_item_start = false;
                } else {
                    self.blank_line();
                }
            }

            Tag::Heading { level, .. } => {
                self.blank_line();
                // Terminals have one font size, so weight and a prefix carry the
                // level instead.
                let hashes = "#".repeat(heading_depth(level));
                let style = self.paint(Style::default().fg(HEADING).add_modifier(Modifier::BOLD));
                self.pending.push(Span::styled(format!("{hashes} "), style));
                self.style = self.style.fg(HEADING).add_modifier(Modifier::BOLD);
            }

            Tag::BlockQuote(_) => {
                self.blank_line();
                self.quote_depth += 1;
            }

            Tag::CodeBlock(_) => {
                self.blank_line();
                self.in_code_block = true;
                self.style = self.style.fg(CODE);
            }

            Tag::List(first) => {
                if self.lists.is_empty() {
                    self.blank_line();
                } else {
                    self.break_line();
                }
                self.lists.push(first);
            }

            Tag::Item => {
                self.break_line();
                let depth = self.lists.len().saturating_sub(1);
                let indent = "  ".repeat(depth);
                let marker = match self.lists.last_mut() {
                    Some(Some(number)) => {
                        let current = *number;
                        *number += 1;
                        format!("{current}. ")
                    }
                    _ => "• ".to_string(),
                };
                let style = self.paint(Style::default().fg(ACCENT));
                self.pending
                    .push(Span::styled(format!("{indent}{marker}"), style));
                self.at_item_start = true;
            }

            Tag::Emphasis => self.style = self.style.add_modifier(Modifier::ITALIC),
            Tag::Strong => self.style = self.style.add_modifier(Modifier::BOLD),
            Tag::Strikethrough => self.style = self.style.add_modifier(Modifier::CROSSED_OUT),

            // The URL is dropped: it is unclickable here and would usually be
            // longer than the text it decorates. The colour marks it as a link.
            Tag::Link { .. } => self.style = self.style.fg(ACCENT).add_modifier(Modifier::UNDERLINED),
            Tag::Image { .. } => self.style = self.style.fg(QUOTE),

            _ => {}
        }
    }

    fn end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Paragraph | TagEnd::Heading(_) => {
                self.break_line();
                self.style = Style::default();
            }
            TagEnd::BlockQuote(_) => {
                self.break_line();
                self.quote_depth = self.quote_depth.saturating_sub(1);
            }
            TagEnd::CodeBlock => {
                self.break_line();
                self.in_code_block = false;
                self.style = Style::default();
            }
            TagEnd::List(_) => {
                self.break_line();
                self.lists.pop();
            }
            TagEnd::Item => self.break_line(),
            TagEnd::Emphasis => self.style = self.style.remove_modifier(Modifier::ITALIC),
            TagEnd::Strong => self.style = self.style.remove_modifier(Modifier::BOLD),
            TagEnd::Strikethrough => self.style = self.style.remove_modifier(Modifier::CROSSED_OUT),
            TagEnd::Link | TagEnd::Image => self.style = Style::default(),
            _ => {}
        }
    }

    fn finish(mut self) -> Text<'static> {
        self.break_line();
        // A trailing blank from the last block break is noise.
        while self.lines.last().is_some_and(|line| line.spans.is_empty()) {
            self.lines.pop();
        }
        Text::from(self.lines)
    }
}

fn heading_depth(level: HeadingLevel) -> usize {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    /// The visible text of a rendered document, one entry per line.
    fn rendered(html: &str) -> Vec<String> {
        description_to_text(html, false)
            .lines
            .iter()
            .map(|line| line.spans.iter().map(|s| s.content.as_ref()).collect())
            .collect()
    }

    #[test]
    fn planes_editor_classes_do_not_reach_the_screen() {
        let lines = rendered(
            r#"<p class="editor-paragraph-block">Hello</p>
               <h2 class="editor-heading-block">Settings</h2>"#,
        );
        assert!(
            !lines.iter().any(|line| line.contains("editor-")),
            "{lines:?}"
        );
        assert!(lines.contains(&"Hello".to_string()), "{lines:?}");
        assert!(lines.contains(&"## Settings".to_string()), "{lines:?}");
    }

    #[test]
    fn a_loose_list_keeps_each_item_on_one_line() {
        // Plane's editor produces loose lists, where every item's text is
        // wrapped in its own paragraph.
        let lines = rendered("<ul><li><p>first</p></li><li><p>second</p></li></ul>");
        assert!(lines.contains(&"• first".to_string()), "{lines:?}");
        assert!(lines.contains(&"• second".to_string()), "{lines:?}");
    }

    #[test]
    fn ordered_lists_count_and_nested_lists_indent() {
        let lines = rendered("<ol><li>one</li><li>two<ul><li>inner</li></ul></li></ol>");
        assert!(lines.contains(&"1. one".to_string()), "{lines:?}");
        assert!(lines.contains(&"2. two".to_string()), "{lines:?}");
        assert!(
            lines.iter().any(|line| line.contains("  • inner")),
            "nested items should indent: {lines:?}"
        );
    }

    #[test]
    fn emphasis_becomes_style_rather_than_markers() {
        let text = description_to_text("<p><strong>bold</strong> and <em>italic</em></p>", false);
        let spans: Vec<_> = text.lines[0].spans.iter().collect();

        let bold = spans.iter().find(|s| s.content == "bold").expect("bold span");
        assert!(bold.style.add_modifier.contains(Modifier::BOLD));

        let italic = spans.iter().find(|s| s.content == "italic").expect("italic span");
        assert!(italic.style.add_modifier.contains(Modifier::ITALIC));

        // The markers themselves must not survive into the output.
        let whole: String = spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(whole, "bold and italic");
    }

    #[test]
    fn a_dimmed_render_is_uniformly_grey() {
        let text = description_to_text(
            "<h2>Heading</h2><p>body with <code>code</code></p>",
            true,
        );
        for line in &text.lines {
            for span in &line.spans {
                assert_eq!(
                    span.style.fg,
                    Some(Color::DarkGray),
                    "every span dims: {span:?}"
                );
            }
        }
    }

    #[test]
    fn an_empty_description_renders_nothing() {
        // What Plane stores for a description the user never typed into.
        assert!(rendered("<p></p>").is_empty());
    }

    #[test]
    fn unparseable_input_falls_back_to_showing_itself() {
        // Better a visible oddity than a silently blank pane.
        let lines = rendered("not < really > html");
        assert!(!lines.is_empty(), "{lines:?}");
    }

    #[test]
    fn code_blocks_keep_their_line_breaks() {
        let lines = rendered("<pre><code>one\ntwo</code></pre>");
        assert!(lines.contains(&"one".to_string()), "{lines:?}");
        assert!(lines.contains(&"two".to_string()), "{lines:?}");
    }
}
