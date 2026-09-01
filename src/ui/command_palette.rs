//! The command palette: every action in the program, searchable by name.
//!
//! A terminal interface with enough keys becomes a program you have to have
//! been taught. `ctrl+y` toggles emote highlighting, `space a` shows the
//! activity column, `alt+w` swaps which half of the combined tab has the
//! keyboard — all reasonable, none guessable. The footer can only ever show a
//! handful of them, and a help screen you have to remember to open is not much
//! better than a manual.
//!
//! `ctrl+p` opens a list of everything, filtered as you type. Every entry
//! shows the key that runs it, so using the palette teaches you the key you
//! could have pressed, and after a while you stop needing the palette for the
//! things you do often.
//!
//! **The palette does not implement anything.** Each entry names the keys it
//! stands for, and choosing it replays exactly those keys through the normal
//! key handling. That is deliberate: an entry that reimplemented an action
//! would be free to drift away from what the key actually does — and a
//! discoverability feature that lies about the keys is worse than none. It
//! also means every entry is testable by asking whether replaying its keys
//! does anything at all.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};

use crate::theme;

/// One entry in the palette.
pub struct Entry {
    /// What it does, in plain words.
    pub title: &'static str,
    /// The keys that do it, shown as a hint and replayed when it is chosen.
    /// More than one for a chord like `space a`.
    pub keys: &'static [Key],
    /// The key hint as it is written in the footer, which is not always
    /// derivable from `keys` (`q / ctrl+c`, `↑/↓`).
    pub shortcut: &'static str,
    /// Extra words that should match this entry when searching, for the times
    /// the title uses a word you would not have thought of.
    pub keywords: &'static [&'static str],
    /// The keymap action this entry stands for, when there is one.
    ///
    /// Used to show the key it is *currently* on rather than the default the
    /// entry was written with. Entries that replay a sequence of keys with no
    /// single action behind them leave this `None` and keep their written
    /// hint.
    pub action: Option<crate::keys::Action>,
    /// Whether this action needs a chat to be open before it can do anything.
    ///
    /// Recorded rather than inferred so the test that replays every entry
    /// knows which ones legitimately do nothing on a fresh session with no
    /// chat connected yet, instead of having to treat "did nothing" as
    /// always acceptable.
    pub needs_chat: bool,
}

/// A key to replay, in a form that can be written down as a constant.
#[derive(Debug, Clone, Copy)]
pub struct Key {
    pub code: KeyCode,
    pub modifiers: KeyModifiers,
}

impl Key {
    const fn plain(code: KeyCode) -> Self {
        Self {
            code,
            modifiers: KeyModifiers::NONE,
        }
    }
    const fn ctrl(c: char) -> Self {
        Self {
            code: KeyCode::Char(c),
            modifiers: KeyModifiers::CONTROL,
        }
    }
    const fn alt(c: char) -> Self {
        Self {
            code: KeyCode::Char(c),
            modifiers: KeyModifiers::ALT,
        }
    }
    const fn char(c: char) -> Self {
        Self::plain(KeyCode::Char(c))
    }

    pub fn event(self) -> KeyEvent {
        KeyEvent::new(self.code, self.modifiers)
    }
}

/// Every action the palette offers.
///
/// Grouped roughly the way the interface is: getting around first, then the
/// stream, then chat, then appearance, then leaving.
pub const ENTRIES: &[Entry] = &[
    Entry {
        title: "Go to the Stream Info tab",
        keys: &[Key::alt('1')],
        shortcut: "alt+1",
        keywords: &["tab", "stream", "info", "dashboard", "settings"],
        action: Some(crate::keys::Action::TabStreamInfo),
        needs_chat: false,
    },
    Entry {
        title: "Go to the Chat tab",
        keys: &[Key::alt('2')],
        shortcut: "alt+2",
        keywords: &["tab", "chat", "messages", "twitch", "youtube"],
        action: Some(crate::keys::Action::TabChat),
        needs_chat: false,
    },
    Entry {
        title: "Go to the Combined tab",
        keys: &[Key::alt('3')],
        shortcut: "alt+3",
        keywords: &["tab", "combined", "both", "split", "everything"],
        action: Some(crate::keys::Action::TabCombined),
        needs_chat: false,
    },
    Entry {
        title: "Swap which half of the Combined tab has the keyboard",
        keys: &[Key::alt('w')],
        shortcut: "alt+w",
        keywords: &["combined", "focus", "swap", "switch", "half", "pane"],
        action: Some(crate::keys::Action::CombinedSwapFocus),
        needs_chat: false,
    },
    Entry {
        title: "Show the message history",
        keys: &[Key::alt('m')],
        shortcut: "alt+m",
        keywords: &[
            "messages",
            "notifications",
            "history",
            "log",
            "toast",
            "what happened",
        ],
        action: Some(crate::keys::Action::MessageHistory),
        needs_chat: false,
    },
    Entry {
        title: "Go to the OBS tab",
        keys: &[Key::alt('4')],
        shortcut: "alt+4",
        keywords: &[
            "tab", "obs", "studio", "scenes", "audio", "record", "stream",
        ],
        action: Some(crate::keys::Action::TabObs),
        needs_chat: false,
    },
    Entry {
        title: "Refresh the live statistics now",
        keys: &[Key::char('r')],
        shortcut: "r",
        keywords: &["refresh", "poll", "viewers", "stats", "statistics"],
        action: Some(crate::keys::Action::RefreshStats),
        needs_chat: false,
    },
    Entry {
        title: "Copy the Twitch stream key to the clipboard",
        keys: &[Key::char('y')],
        shortcut: "y",
        keywords: &["key", "stream key", "clipboard", "copy", "obs", "twitch"],
        action: Some(crate::keys::Action::CopyTwitchKey),
        needs_chat: false,
    },
    Entry {
        title: "Copy the YouTube stream key to the clipboard",
        keys: &[Key::char('Y')],
        shortcut: "Y",
        keywords: &["key", "stream key", "clipboard", "copy", "obs", "youtube"],
        action: Some(crate::keys::Action::CopyYouTubeKey),
        needs_chat: false,
    },
    Entry {
        title: "Open the watch page in a browser",
        keys: &[Key::char('o')],
        shortcut: "o",
        keywords: &["open", "watch", "browser", "url", "link", "page"],
        action: Some(crate::keys::Action::OpenWatchPage),
        needs_chat: false,
    },
    Entry {
        title: "Finish the broadcast",
        keys: &[Key::char(' '), Key::char('s'), Key::char('x')],
        shortcut: "space s x",
        keywords: &[
            "end",
            "finish",
            "stop",
            "close",
            "complete",
            "broadcast",
            "stream",
            "offline",
        ],
        action: Some(crate::keys::Action::EndStream),
        needs_chat: false,
    },
    Entry {
        title: "Edit the stream title, category and tags",
        keys: &[Key::char('e')],
        shortcut: "e",
        keywords: &["edit", "title", "category", "tags", "form", "settings"],
        action: Some(crate::keys::Action::EditStreamInfo),
        needs_chat: false,
    },
    Entry {
        title: "Focus the message box",
        keys: &[Key::char('i')],
        shortcut: "i",
        keywords: &["chat", "compose", "write", "send", "input", "type"],
        action: Some(crate::keys::Action::ChatCompose),
        needs_chat: false,
    },
    Entry {
        title: "Search the chat",
        keys: &[Key::char('/')],
        shortcut: "/",
        keywords: &["chat", "search", "find", "grep"],
        action: Some(crate::keys::Action::ChatSearch),
        needs_chat: true,
    },
    Entry {
        title: "Reconnect the chat",
        keys: &[Key::ctrl('r')],
        shortcut: "ctrl+r",
        keywords: &["chat", "reconnect", "connection", "retry", "dropped"],
        action: Some(crate::keys::Action::ChatReconnect),
        needs_chat: true,
    },
    Entry {
        title: "Open the emoji picker",
        keys: &[Key::ctrl('e')],
        shortcut: "ctrl+e",
        keywords: &["emoji", "emote", "picker", "insert", "smiley"],
        action: Some(crate::keys::Action::ChatEmojiPicker),
        needs_chat: false,
    },
    Entry {
        title: "Clear every chat message filter",
        keys: &[Key::char('0')],
        shortcut: "0",
        keywords: &["filter", "filters", "reset", "clear", "all", "show"],
        action: Some(crate::keys::Action::ChatClearFilters),
        needs_chat: true,
    },
    Entry {
        title: "Show only messages that mention you",
        keys: &[Key::char('1')],
        shortcut: "1",
        keywords: &["filter", "mentions", "highlight", "me"],
        action: None,
        needs_chat: true,
    },
    Entry {
        title: "Next chat",
        keys: &[Key::char(']')],
        shortcut: "]",
        keywords: &["chat", "next", "switch", "channel"],
        action: Some(crate::keys::Action::ChatNextChat),
        needs_chat: true,
    },
    Entry {
        title: "Previous chat",
        keys: &[Key::char('[')],
        shortcut: "[",
        keywords: &["chat", "previous", "back", "switch", "channel"],
        action: Some(crate::keys::Action::ChatPreviousChat),
        needs_chat: true,
    },
    Entry {
        title: "Widen the left chat pane",
        keys: &[Key::char('<')],
        shortcut: "<",
        keywords: &["pane", "resize", "wider", "narrower", "split", "layout"],
        action: Some(crate::keys::Action::ChatWiden),
        needs_chat: false,
    },
    Entry {
        title: "Narrow the left chat pane",
        keys: &[Key::char('>')],
        shortcut: ">",
        keywords: &["pane", "resize", "wider", "narrower", "split", "layout"],
        action: Some(crate::keys::Action::ChatNarrow),
        needs_chat: false,
    },
    Entry {
        title: "Reset the pane sizes",
        keys: &[Key::char('=')],
        shortcut: "=",
        keywords: &["pane", "resize", "reset", "default", "layout"],
        action: Some(crate::keys::Action::ChatResetPanes),
        needs_chat: false,
    },
    Entry {
        title: "Choose a theme",
        keys: &[Key::ctrl('t')],
        shortcut: "ctrl+t",
        keywords: &[
            "theme",
            "themes",
            "colour",
            "color",
            "colours",
            "palette",
            "appearance",
            "dark",
            "light",
        ],
        action: Some(crate::keys::Action::ThemePicker),
        needs_chat: false,
    },
    Entry {
        title: "Change how much the interface animates",
        keys: &[Key::alt('a')],
        shortcut: "alt+a",
        keywords: &[
            "animation",
            "animations",
            "motion",
            "reduced",
            "off",
            "still",
            "accessibility",
        ],
        action: Some(crate::keys::Action::CycleAnimations),
        needs_chat: false,
    },
    Entry {
        title: "Show or hide the process telemetry",
        keys: &[Key::alt('t')],
        shortcut: "alt+t",
        keywords: &[
            "telemetry",
            "cpu",
            "memory",
            "fps",
            "frame rate",
            "performance",
            "status",
        ],
        action: Some(crate::keys::Action::ToggleTelemetry),
        needs_chat: false,
    },
    Entry {
        title: "Start or stop streaming in OBS",
        keys: &[Key::alt('4'), Key::char('s')],
        shortcut: "alt+4 then s",
        keywords: &[
            "obs",
            "stream",
            "streaming",
            "start",
            "stop",
            "go live",
            "broadcast",
        ],
        action: Some(crate::keys::Action::ObsToggleStream),
        needs_chat: false,
    },
    Entry {
        title: "Start or stop recording in OBS",
        keys: &[Key::alt('4'), Key::char('r')],
        shortcut: "alt+4 then r",
        keywords: &["obs", "record", "recording", "start", "stop", "capture"],
        action: Some(crate::keys::Action::ObsToggleRecord),
        needs_chat: false,
    },
    Entry {
        title: "Mute or unmute every OBS audio input",
        keys: &[Key::alt('4'), Key::char('M')],
        shortcut: "alt+4 then M",
        keywords: &[
            "obs",
            "mute",
            "unmute",
            "audio",
            "microphone",
            "mic",
            "silence",
            "panic",
        ],
        action: Some(crate::keys::Action::ObsMuteAll),
        needs_chat: false,
    },
    Entry {
        title: "Reconnect to OBS",
        keys: &[Key::alt('4'), Key::char('R')],
        shortcut: "alt+4 then R",
        keywords: &["obs", "reconnect", "connection", "retry", "dropped"],
        action: Some(crate::keys::Action::ObsReconnect),
        needs_chat: false,
    },
    Entry {
        title: "Quit",
        keys: &[Key::ctrl('c')],
        shortcut: "q / ctrl+c",
        keywords: &["quit", "exit", "close", "leave"],
        action: Some(crate::keys::Action::Quit),
        needs_chat: false,
    },
];

/// One row of the palette as it is actually shown.
///
/// Built at open time rather than written down, because the palette is meant
/// to be the complete list. [`ENTRIES`] below only covers the things a key
/// sequence does with no single `Action` behind it; everything that *is* an
/// action is generated from [`crate::keys::Action::ALL`], so a new action
/// cannot be forgotten here — which is exactly what had happened: 31 written
/// entries against more than sixty actions, with most of the OBS ones, chat
/// scrolling, account cycling and everything on the Config tab unreachable.
#[derive(Debug, Clone)]
pub struct Row {
    pub title: String,
    pub keys: Vec<KeyEvent>,
    /// The key as it is written on screen. Empty when the action is unbound,
    /// which is the case the palette exists for.
    pub shortcut: String,
    pub keywords: String,
    pub needs_chat: bool,
}

/// Build the full list: the hand-written sequences, then every action that is
/// not already covered by one.
fn build_rows(keymap: &crate::keys::Keymap) -> Vec<Row> {
    let mut rows: Vec<Row> = Vec::new();
    let mut covered: std::collections::HashSet<crate::keys::Action> =
        std::collections::HashSet::new();

    for entry in ENTRIES {
        if let Some(action) = entry.action {
            covered.insert(action);
        }
        rows.push(Row {
            title: entry.title.to_string(),
            // A bound action replays whatever it is bound to *now*; an entry
            // with no action behind it replays the keys it was written with.
            keys: entry
                .action
                .and_then(|action| keymap.chord_for(action))
                .map(|chord| chord.into_iter().map(|key| key.to_event()).collect())
                .unwrap_or_else(|| entry.keys.iter().map(|key| key.event()).collect()),
            shortcut: entry
                .action
                .and_then(|action| keymap.binding_for(action))
                .unwrap_or_else(|| entry.shortcut.to_string()),
            keywords: entry.keywords.join(" "),
            needs_chat: entry.needs_chat,
        });
    }

    for &action in crate::keys::Action::ALL.iter() {
        if covered.contains(&action) {
            continue;
        }
        rows.push(Row {
            title: action.describe().to_string(),
            keys: keymap
                .chord_for(action)
                .map(|chord| chord.into_iter().map(|key| key.to_event()).collect())
                .unwrap_or_default(),
            shortcut: keymap.binding_for(action).unwrap_or_default(),
            // The group and the action's own name, so searching for "obs" or
            // for the name in config.toml both find it.
            keywords: format!("{} {}", action.group(), action.name()),
            needs_chat: action.name().starts_with("chat."),
        });
    }

    rows
}

/// The palette's state. `None` in `App` means it is closed.
#[derive(Debug, Clone)]
pub struct CommandPalette {
    /// What has been typed so far.
    pub query: String,
    /// Which of the *matching* entries is selected.
    pub selected: usize,
    /// Every row, built from the keymap when the palette was opened.
    rows: Vec<Row>,
    /// The rows matching `query`, recomputed on each edit rather than on each
    /// call. `matches`, `chosen`, `move_by` and the drawing all want it, and
    /// the drawing wants it several times per frame.
    matching: Vec<usize>,
}

impl CommandPalette {
    /// Open the palette against the bindings currently in force.
    pub fn open(keymap: &crate::keys::Keymap) -> Self {
        let rows = build_rows(keymap);
        let matching = (0..rows.len()).collect();
        Self {
            query: String::new(),
            selected: 0,
            rows,
            matching,
        }
    }

    pub fn rows(&self) -> &[Row] {
        &self.rows
    }

    /// The rows matching the current query, as indices into [`Self::rows`].
    pub fn matches(&self) -> &[usize] {
        &self.matching
    }

    /// The row that would run if Enter were pressed now.
    pub fn chosen(&self) -> Option<&Row> {
        self.matching
            .get(self.selected)
            .and_then(|index| self.rows.get(*index))
    }

    /// Move the selection, wrapping at both ends.
    pub fn move_by(&mut self, delta: isize) {
        let count = self.matching.len() as isize;
        if count == 0 {
            self.selected = 0;
            return;
        }
        self.selected = (self.selected as isize + delta).rem_euclid(count) as usize;
    }

    pub fn select_first(&mut self) {
        self.selected = 0;
    }

    pub fn select_last(&mut self) {
        self.selected = self.matching.len().saturating_sub(1);
    }

    /// Type a character into the query.
    pub fn push(&mut self, c: char) {
        self.query.push(c);
        self.refilter();
    }

    pub fn backspace(&mut self) {
        self.query.pop();
        self.refilter();
    }

    fn refilter(&mut self) {
        self.matching = matches_for(&self.rows, &self.query);
        // A narrowed list has a different first row, so the selection goes
        // back to the top rather than pointing at whatever happens to be at
        // the old index now.
        self.selected = 0;
    }
}

/// Which entries match `query`.
///
/// The rule is "all of the typed words appear somewhere in the entry", not a
/// fuzzy subsequence match. Typing `copy key` finds "Copy the Twitch stream
/// key to the clipboard" whichever order the words are in, and typing three
/// letters does not return two-thirds of the list.
fn matches_for(rows: &[Row], query: &str) -> Vec<usize> {
    let needles: Vec<String> = query
        .split_whitespace()
        .map(|word| word.to_ascii_lowercase())
        .collect();
    if needles.is_empty() {
        return (0..rows.len()).collect();
    }

    let mut scored: Vec<(u8, usize)> = rows
        .iter()
        .enumerate()
        .filter_map(|(index, row)| {
            let title = row.title.to_ascii_lowercase();
            let haystack = format!(
                "{title} {} {}",
                row.shortcut.to_ascii_lowercase(),
                row.keywords.to_ascii_lowercase()
            );
            if !needles.iter().all(|needle| haystack.contains(needle)) {
                return None;
            }
            // Rank, so a query that matches a title from the start comes above
            // one that matches it in the middle or only through a keyword. The
            // list used to come out in the order the const happened to be
            // written in, regardless of how well anything matched.
            let first = &needles[0];
            let rank = if title.starts_with(first.as_str()) {
                0
            } else if title
                .split_whitespace()
                .any(|word| word.starts_with(first.as_str()))
            {
                1
            } else if title.contains(first.as_str()) {
                2
            } else {
                3
            };
            Some((rank, index))
        })
        .collect();

    // Stable within a rank, so equally good matches keep their listed order.
    scored.sort_by_key(|(rank, _)| *rank);
    scored.into_iter().map(|(_, index)| index).collect()
}

/// Draw the palette over the bottom half of `area`.
///
/// The bottom, not the middle: what you were looking at when you opened it is
/// usually what you want the command to act on, so covering the top half would
/// hide the thing you are about to change.
pub fn draw(
    frame: &mut Frame,
    area: Rect,
    palette: &CommandPalette,
    chat_open: bool,
) {
    let sk = theme::skin();
    let matches = palette.matches();

    // As tall as it needs to be, up to half the screen.
    let wanted = matches.len().min(12) as u16 + 4;
    let height = wanted.min(area.height);
    if height < 4 || area.width < 20 {
        return;
    }
    let rect = Rect {
        x: area.x,
        y: area.y + area.height - height,
        width: area.width,
        height,
    };

    frame.render_widget(Clear, rect);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(sk.accent))
        .style(Style::new().bg(sk.surface))
        .title(" Commands ");
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let rows = Layout::vertical([Constraint::Length(1), Constraint::Min(0)])
        .horizontal_margin(1)
        .split(inner);

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("› ", Style::new().fg(sk.accent)),
            Span::styled(palette.query.clone(), Style::new().fg(sk.foreground)),
            Span::styled("▌", Style::new().fg(sk.accent)),
            Span::styled(
                if matches.is_empty() {
                    "   nothing matches".to_string()
                } else {
                    format!("   {} of {}", matches.len(), palette.rows().len())
                },
                Style::new().fg(sk.muted),
            ),
        ])),
        rows[0],
    );

    let height = rows[1].height as usize;
    // Scroll the window so the selection stays visible in a long list.
    let first = palette
        .selected
        .saturating_sub(height.saturating_sub(1))
        .min(matches.len().saturating_sub(height.min(matches.len())));

    // Each row's key came from the keymap when the palette was opened, so a
    // rebound key shows the key it is actually on now. An unbound action
    // shows nothing at all in that column, which is the case the palette
    // exists for.
    let widest = matches
        .iter()
        .filter_map(|index| palette.rows().get(*index))
        .map(|row| row.shortcut.chars().count())
        .max()
        .unwrap_or(0);

    let lines: Vec<Line> = matches
        .iter()
        .enumerate()
        .skip(first)
        .take(height)
        .map(|(position, index)| {
            let entry = &palette.rows()[*index];
            let selected = position == palette.selected;
            // An action that needs an open chat says so, rather than being
            // chosen and appearing to do nothing.
            let unavailable = entry.needs_chat && !chat_open;
            let title_color = match (selected, unavailable) {
                (_, true) => sk.muted,
                (true, false) => sk.foreground,
                (false, false) => sk.muted,
            };
            let mut line = Line::from(vec![
                Span::styled(
                    if selected { "▸ " } else { "  " },
                    Style::new().fg(sk.accent),
                ),
                Span::styled(
                    format!("{:<widest$}  ", entry.shortcut),
                    Style::new().fg(sk.accent),
                ),
                Span::styled(
                    entry.title.clone(),
                    if selected && !unavailable {
                        Style::new().fg(title_color).add_modifier(Modifier::BOLD)
                    } else {
                        Style::new().fg(title_color)
                    },
                ),
                Span::styled(
                    if unavailable {
                        "  (needs an open chat)"
                    } else {
                        ""
                    },
                    Style::new().fg(sk.warning),
                ),
            ]);
            if selected {
                line = line.style(Style::new().bg(sk.selection));
            }
            line
        })
        .collect();

    frame.render_widget(Paragraph::new(lines), rows[1]);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn palette() -> CommandPalette {
        CommandPalette::open(&crate::keys::Keymap::default())
    }

    #[test]
    fn an_empty_query_lists_everything() {
        let palette = palette();
        assert_eq!(matches_for(palette.rows(), "").len(), palette.rows().len());
        assert_eq!(
            matches_for(palette.rows(), "   ").len(),
            palette.rows().len()
        );
    }

    /// The palette's promise is that every capability stays reachable even
    /// when it is unbound. Thirty-one hand-written entries against more than
    /// sixty actions quietly broke that: most of the OBS actions, chat
    /// scrolling, account cycling and everything on the Config tab were in no
    /// list anywhere.
    #[test]
    fn every_action_appears_in_the_palette() {
        let keymap = crate::keys::Keymap::default();
        let palette = CommandPalette::open(&keymap);

        for &action in crate::keys::Action::ALL.iter() {
            // Either a hand-written entry stands for it — those have their own
            // wording — or a row was generated from its description.
            let written = ENTRIES.iter().any(|entry| entry.action == Some(action));
            let generated = palette
                .rows()
                .iter()
                .any(|row| row.title == action.describe());
            assert!(
                written || generated,
                "{} ({}) is in no palette row",
                action.name(),
                action.describe()
            );
        }
    }

    /// Words in any order, which is what people actually type.
    #[test]
    fn every_typed_word_has_to_appear_somewhere_in_the_row() {
        let palette = palette();
        let by_title = matches_for(palette.rows(), "copy key");
        let reversed = matches_for(palette.rows(), "key copy");
        assert_eq!(by_title, reversed);
        assert!(!by_title.is_empty());
        assert!(by_title
            .iter()
            .all(|index| palette.rows()[*index].title.to_lowercase().contains("key")));
    }

    #[test]
    fn searching_ignores_case_and_finds_words_from_the_keywords() {
        // "colour" appears in no title, only in the keywords.
        let palette = palette();
        let found = matches_for(palette.rows(), "COLOUR");
        assert_eq!(found.len(), 1);
        assert_eq!(palette.rows()[found[0]].title, "Choose a theme");
    }

    #[test]
    fn a_query_matching_nothing_returns_nothing_rather_than_everything() {
        let palette = palette();
        assert!(matches_for(palette.rows(), "xyzzy").is_empty());
    }

    /// The list used to come out in the order the const happened to be
    /// written in, however well anything matched.
    #[test]
    fn a_title_that_starts_with_the_query_outranks_one_that_merely_contains_it() {
        let rows = vec![
            Row {
                title: "Toggle the mute on everything".into(),
                keys: Vec::new(),
                shortcut: String::new(),
                keywords: String::new(),
                needs_chat: false,
            },
            Row {
                title: "Mute the selected input".into(),
                keys: Vec::new(),
                shortcut: String::new(),
                keywords: String::new(),
                needs_chat: false,
            },
        ];

        let found = matches_for(&rows, "mute");
        assert_eq!(
            rows[found[0]].title, "Mute the selected input",
            "a title starting with the query has to come first"
        );
    }

    #[test]
    fn typing_resets_the_selection_to_the_top_of_the_narrowed_list() {
        let mut palette = palette();
        palette.move_by(5);
        assert_eq!(palette.selected, 5);
        palette.push('t');
        assert_eq!(palette.selected, 0);
        palette.move_by(1);
        palette.backspace();
        assert_eq!(palette.selected, 0);
    }

    #[test]
    fn the_selection_wraps_at_both_ends() {
        let mut palette = palette();
        let last = palette.rows().len() - 1;
        palette.move_by(-1);
        assert_eq!(palette.selected, last);
        palette.move_by(1);
        assert_eq!(palette.selected, 0);
    }

    #[test]
    fn moving_within_an_empty_result_list_does_not_panic() {
        let mut palette = palette();
        for c in "xyzzy".chars() {
            palette.push(c);
        }
        palette.move_by(1);
        assert_eq!(palette.selected, 0);
        assert!(palette.chosen().is_none());
    }

    /// Every hand-written entry must name at least one key, since choosing
    /// one works by replaying its keys.
    ///
    /// A *generated* row may legitimately have none — that is what an unbound
    /// action looks like, and reporting it honestly with a blank key column
    /// beats hiding it.
    #[test]
    fn every_written_entry_names_the_keys_it_replays() {
        for entry in ENTRIES {
            assert!(!entry.keys.is_empty(), "{} replays nothing", entry.title);
            assert!(!entry.shortcut.is_empty(), "{} shows no key", entry.title);
        }
    }

    /// Two rows with the same title would be indistinguishable in the list.
    #[test]
    fn no_two_rows_share_a_title() {
        let palette = palette();
        let mut titles: Vec<&str> = palette.rows().iter().map(|row| row.title.as_str()).collect();
        titles.sort_unstable();
        let count = titles.len();
        titles.dedup();
        assert_eq!(titles.len(), count, "two rows share a title");
    }
}
