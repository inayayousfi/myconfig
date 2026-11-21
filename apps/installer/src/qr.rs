//! The QR code of a text, drawn as black squares on white with the quiet zone that phone
//! cameras need around it.
use iced::{
    Color, Point, Rectangle, Renderer, Size, Theme, mouse,
    widget::canvas::{self, Frame, Geometry},
};

/// The light modules around the code that readers need, on each side.
const QUIET_ZONE: usize = 4;

/// The modules of a QR code, quiet zone included, row by row.
pub struct QrCode {
    /// The modules on each side.
    width: usize,
    dark: Vec<bool>,
}

impl QrCode {
    /// The code of `text`, or None when the text is too long for a QR code.
    pub fn new(text: &str) -> Option<Self> {
        let code = qrcode::QrCode::new(text.as_bytes()).ok()?;
        let inner = code.width();
        let width = inner + 2 * QUIET_ZONE;
        let colors = code.to_colors();
        let dark = (0..width * width)
            .map(|index| {
                let (row, column) = (index / width, index % width);
                let inside = |at: usize| (QUIET_ZONE..QUIET_ZONE + inner).contains(&at);
                inside(row)
                    && inside(column)
                    && colors[(row - QUIET_ZONE) * inner + column - QUIET_ZONE]
                        == qrcode::Color::Dark
            })
            .collect();
        Some(QrCode { width, dark })
    }

    /// The modules on each side, quiet zone included.
    pub fn width(&self) -> usize {
        self.width
    }

    /// Whether the module at `row` and `column` is dark.
    pub fn is_dark(&self, row: usize, column: usize) -> bool {
        self.dark[row * self.width + column]
    }
}

impl<Message> canvas::Program<Message> for QrCode {
    type State = ();

    fn draw(
        &self,
        _state: &(),
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let side = bounds.width.min(bounds.height);
        let cell = (side / self.width() as f32).floor().max(1.0);
        frame.fill_rectangle(
            Point::ORIGIN,
            Size::new(cell * self.width() as f32, cell * self.width() as f32),
            Color::WHITE,
        );
        for row in 0..self.width() {
            for column in 0..self.width() {
                if self.is_dark(row, column) {
                    frame.fill_rectangle(
                        Point::new(column as f32 * cell, row as f32 * cell),
                        Size::new(cell, cell),
                        Color::BLACK,
                    );
                }
            }
        }
        vec![frame.into_geometry()]
    }
}

#[cfg(test)]
mod tests {
    use super::QrCode;

    #[test]
    fn a_recovery_key_fits_a_code_with_its_quiet_zone_and_finder_patterns() {
        let code =
            QrCode::new("fkhjdulc-hbinrcig-ntvcnejb-uifjlvbn-cerbtnkg-ivnbvjil-tggkjhel-nedrlkrj")
                .unwrap();
        let width = code.width();
        // A version-N code has 17 + 4N modules on each side, plus 4 light ones each way.
        assert_eq!((width - 8 - 17) % 4, 0);
        for at in 0..width {
            for edge in [0, 3, width - 4, width - 1] {
                assert!(!code.is_dark(edge, at) && !code.is_dark(at, edge));
            }
        }
        // The top-left finder pattern starts right inside the quiet zone.
        assert!((4..11).all(|at| code.is_dark(4, at) && code.is_dark(at, 4)));
        assert!(!code.is_dark(5, 5));
    }
}
