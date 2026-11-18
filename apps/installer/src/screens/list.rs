//! A scrolling list of large rows with one selected row, moved by Up and Down or by a click.
use iced::{
    Element, Length, Task,
    widget::{Column, Id, button, operation, scrollable, text},
};

use crate::theme;

/// The row after moving `by` rows from `selected`, kept inside a list of `len` rows. With no
/// selection, Down selects the first row and Up the last.
pub fn moved(selected: Option<usize>, len: usize, by: isize) -> Option<usize> {
    if len == 0 {
        return None;
    }
    let Some(at) = selected else {
        return Some(if by < 0 { len - 1 } else { 0 });
    };
    Some(at.saturating_add_signed(by).min(len - 1))
}

/// Scrolls the list `id` so that row `index` shows, two rows below the top.
pub fn reveal<M: Send + 'static>(id: &'static str, index: usize) -> Task<M> {
    let y = (index.saturating_sub(2) as f32) * theme::ROW;
    operation::scroll_to(Id::new(id), operation::AbsoluteOffset { x: 0.0, y })
}

/// One row: its label and whether it can be chosen.
pub struct Row {
    pub label: String,
    pub enabled: bool,
}

impl Row {
    pub fn new(label: impl Into<String>) -> Self {
        Row {
            label: label.into(),
            enabled: true,
        }
    }
}

/// Draws the list `id`. A click on an enabled row sends `on_select` with its index.
pub fn view<'a, M: Clone + 'a>(
    id: &'static str,
    rows: Vec<Row>,
    selected: Option<usize>,
    focused: bool,
    height: Length,
    on_select: impl Fn(usize) -> M + 'a,
) -> Element<'a, M> {
    let mut column = Column::new().spacing(4).padding([0, 12]);
    for (index, row) in rows.into_iter().enumerate() {
        let is_selected = selected == Some(index);
        let label = text(row.label).size(theme::TEXT);
        let mut entry = button(label)
            .width(Length::Fill)
            .height(Length::Fixed(theme::ROW - 4.0))
            .padding([12, theme::PAD])
            .style(theme::row(is_selected, focused));
        if row.enabled {
            entry = entry.on_press(on_select(index));
        }
        column = column.push(entry);
    }
    scrollable(column)
        .id(Id::new(id))
        .height(height)
        .width(Length::Fill)
        .into()
}

#[cfg(test)]
mod tests {
    use super::moved;

    #[test]
    fn up_and_down_stop_at_the_ends_and_start_from_either_end() {
        assert_eq!(moved(Some(0), 3, -1), Some(0));
        assert_eq!(moved(Some(1), 3, 1), Some(2));
        assert_eq!(moved(Some(2), 3, 1), Some(2));
        assert_eq!(moved(None, 3, 1), Some(0));
        assert_eq!(moved(None, 3, -1), Some(2));
        assert_eq!(moved(None, 0, 1), None);
    }
}
