//! A single-line text input with a movable caret.
//!
//! `ratatui` draws widgets but does not manage editable text, so this fills that
//! gap. Everything is indexed by **grapheme cluster** — never by byte, and never
//! by `char` either. A byte index panics halfway through a multi-byte letter; a
//! `char` index does not panic, but it happily cuts a family emoji or a
//! flag in half, because those are several code points joined together. What a
//! person means by "one character" when they press Backspace is a grapheme
//! cluster, so that is the unit.

use unicode_segmentation::UnicodeSegmentation;

/// One editable text field.
#[derive(Debug, Clone, Default)]
pub struct TextInput {
    /// The current contents.
    value: String,
    /// Caret position, counted in grapheme clusters from the start.
    cursor: usize,
}

impl TextInput {
    pub fn new(value: impl Into<String>) -> Self {
        let value = value.into();
        let cursor = count(&value);
        Self { value, cursor }
    }

    pub fn value(&self) -> &str {
        &self.value
    }

    pub fn is_empty(&self) -> bool {
        self.value.is_empty()
    }

    /// Replace the contents, putting the caret at the end.
    pub fn set(&mut self, value: impl Into<String>) {
        self.value = value.into();
        self.cursor = count(&self.value);
    }

    /// Convert the grapheme-indexed caret into a byte index, which is what
    /// `String::insert` and `String::replace_range` need.
    fn byte_offset(&self, index: usize) -> usize {
        self.value
            .grapheme_indices(true)
            .nth(index)
            .map(|(offset, _)| offset)
            .unwrap_or(self.value.len())
    }

    pub fn insert(&mut self, c: char) {
        let offset = self.byte_offset(self.cursor);
        self.value.insert(offset, c);
        self.cursor += 1;
    }

    /// Delete the grapheme before the caret (the Backspace key).
    ///
    /// A whole cluster, so one press never leaves half an emoji behind.
    pub fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let start = self.byte_offset(self.cursor - 1);
        let end = self.byte_offset(self.cursor);
        self.value.replace_range(start..end, "");
        self.cursor -= 1;
    }

    /// Delete the grapheme after the caret (the Delete key).
    pub fn delete(&mut self) {
        if self.cursor >= count(&self.value) {
            return;
        }
        let start = self.byte_offset(self.cursor);
        let end = self.byte_offset(self.cursor + 1);
        self.value.replace_range(start..end, "");
    }

    pub fn left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    pub fn right(&mut self) {
        self.cursor = (self.cursor + 1).min(count(&self.value));
    }

    pub fn home(&mut self) {
        self.cursor = 0;
    }

    pub fn end(&mut self) {
        self.cursor = count(&self.value);
    }

    /// Append text at the caret, leaving the caret after what was added.
    ///
    /// Used by the emoji picker and the @mention completion, which insert a
    /// whole run at once rather than a keystroke at a time.
    pub fn insert_str(&mut self, text: &str) {
        let offset = self.byte_offset(self.cursor);
        self.value.insert_str(offset, text);
        self.cursor += count(text);
    }

    /// Replace the run of graphemes `from..self.cursor` with `text`.
    ///
    /// This is what completing a half-typed @mention needs: cut back to where
    /// the partial name started and put the full one in its place.
    pub fn replace_back_to(&mut self, from: usize, text: &str) {
        let from = from.min(self.cursor);
        let start = self.byte_offset(from);
        let end = self.byte_offset(self.cursor);
        self.value.replace_range(start..end, text);
        self.cursor = from + count(text);
    }

    /// Where the caret is, counted in grapheme clusters.
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// Delete the word before the caret (Ctrl+W, as in every Unix shell).
    ///
    /// Skips any run of spaces first, then deletes back to the start of the
    /// word, so pressing it after "hello world   " removes "world   ".
    pub fn delete_word_before(&mut self) {
        let clusters: Vec<&str> = self.value.graphemes(true).collect();
        let blank = |cluster: &str| cluster.chars().all(char::is_whitespace);
        let mut position = self.cursor;

        while position > 0 && blank(clusters[position - 1]) {
            position -= 1;
        }
        while position > 0 && !blank(clusters[position - 1]) {
            position -= 1;
        }

        let start = self.byte_offset(position);
        let end = self.byte_offset(self.cursor);
        self.value.replace_range(start..end, "");
        self.cursor = position;
    }

    /// Delete everything (Ctrl+U).
    pub fn clear(&mut self) {
        self.value.clear();
        self.cursor = 0;
    }

    /// The number of grapheme clusters, for the "87/100" counters in the form.
    pub fn len_chars(&self) -> usize {
        count(&self.value)
    }

    /// The slice of text to display when the field is narrower than its content,
    /// together with where the caret sits inside that slice.
    ///
    /// This keeps the caret visible by scrolling the window horizontally, the way
    /// any ordinary text box does.
    pub fn visible(&self, width: usize) -> (String, usize) {
        if width == 0 {
            return (String::new(), 0);
        }

        let clusters: Vec<&str> = self.value.graphemes(true).collect();
        // `<`, not `<=`, because the caret needs a cell of its own. The caret
        // sits *after* the last character while you are typing, so a value that
        // exactly fills the window needs width + 1 cells to show both the text
        // and the caret — one more than there is. Returning the whole value here
        // would push the caret off the end of the field, where it is clipped and
        // the field looks like it has stopped accepting input.
        if clusters.len() < width {
            return (self.value.clone(), self.cursor);
        }

        // Keep the caret inside the window, biased so there is context on the
        // left once you have scrolled away from the start.
        let start = self.cursor.saturating_sub(width.saturating_sub(1));
        let end = (start + width).min(clusters.len());

        (
            clusters[start..end].concat(),
            self.cursor.saturating_sub(start),
        )
    }
}

/// How many grapheme clusters are in `text`.
fn count(text: &str) -> usize {
    text.graphemes(true).count()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The caret position, read back the way the renderer reads it.
    fn caret(input: &TextInput) -> usize {
        input.visible(1000).1
    }

    #[test]
    fn new_puts_the_caret_at_the_end() {
        let input = TextInput::new("hello");
        assert_eq!(caret(&input), 5);
    }

    #[test]
    fn typing_inserts_at_the_caret() {
        let mut input = TextInput::new("helo");
        input.left();
        input.insert('l');
        assert_eq!(input.value(), "hello");
        assert_eq!(caret(&input), 4);
    }

    #[test]
    fn backspace_at_the_start_does_nothing() {
        let mut input = TextInput::new("abc");
        input.home();
        input.backspace();
        assert_eq!(input.value(), "abc");
    }

    #[test]
    fn delete_at_the_end_does_nothing() {
        let mut input = TextInput::new("abc");
        input.end();
        input.delete();
        assert_eq!(input.value(), "abc");
    }

    #[test]
    fn editing_multi_byte_characters_does_not_panic() {
        // Every one of these is four bytes, so any byte-indexed edit would
        // either panic or corrupt the string.
        let mut input = TextInput::new("🎮🎯🎲");
        input.home();
        input.right();
        input.insert('x');
        assert_eq!(input.value(), "🎮x🎯🎲");

        input.backspace();
        assert_eq!(input.value(), "🎮🎯🎲");

        input.delete();
        assert_eq!(input.value(), "🎮🎲");
    }

    #[test]
    fn caret_movement_is_clamped_at_both_ends() {
        let mut input = TextInput::new("ab");
        input.left();
        input.left();
        input.left();
        assert_eq!(caret(&input), 0);

        for _ in 0..10 {
            input.right();
        }
        assert_eq!(caret(&input), 2);
    }

    #[test]
    fn ctrl_w_deletes_the_previous_word_including_trailing_spaces() {
        let mut input = TextInput::new("hello brave world   ");
        input.delete_word_before();
        assert_eq!(input.value(), "hello brave ");

        input.delete_word_before();
        assert_eq!(input.value(), "hello ");
    }

    #[test]
    fn ctrl_w_on_leading_whitespace_clears_to_the_start() {
        let mut input = TextInput::new("   ");
        input.delete_word_before();
        assert_eq!(input.value(), "");
        assert_eq!(caret(&input), 0);
    }

    #[test]
    fn a_short_value_is_shown_whole() {
        let input = TextInput::new("abc");
        let (text, caret) = input.visible(20);
        assert_eq!(text, "abc");
        assert_eq!(caret, 3);
    }

    #[test]
    fn a_long_value_scrolls_to_keep_the_caret_visible() {
        let input = TextInput::new("0123456789");
        let (text, caret) = input.visible(5);
        // The caret is at the end, so the window shows the tail of the string.
        assert!(text.chars().count() <= 5);
        assert!(caret < 5, "caret {caret} escaped the window");
        assert!(text.ends_with('9'));
    }

    #[test]
    fn a_zero_width_window_is_handled_without_panicking() {
        let input = TextInput::new("abc");
        assert_eq!(input.visible(0), (String::new(), 0));
    }

    /// The boundary case: a value exactly as wide as the field. Whatever is
    /// returned has to render in `width` cells including the caret, because
    /// `draw::editing_spans` draws the caret as a cell of its own.
    #[test]
    fn a_value_that_exactly_fills_the_window_still_leaves_room_for_the_caret() {
        let input = TextInput::new("abcde");
        let (text, caret) = input.visible(5);

        // The caret is after the last character, so rendering costs
        // `text.len()` cells with one of them highlighted, or one more when the
        // caret is past the end.
        let rendered = text.chars().count().max(caret + 1);
        assert_eq!(rendered, 5, "rendered {rendered} cells into a 5-wide field");
        assert!(
            text.ends_with('e'),
            "the caret end of the value must be visible"
        );

        // With the caret inside the value there is no extra cell to find, so the
        // whole value is shown.
        let mut input = TextInput::new("abcde");
        input.cursor = 2;
        assert_eq!(input.visible(5), ("abcde".to_string(), 2));
    }
}
