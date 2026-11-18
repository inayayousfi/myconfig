//! The installer's look: InayOS Black and Pink, with large text and large click targets.
use iced::{
    Background, Border, Color, Font, Theme,
    theme::Palette,
    widget::{button, container, text_input, toggler as switch},
};

/// Fira Sans, which the app carries, so the screens look the same whatever fonts the system
/// has.
pub const FONT: Font = Font::with_name("Fira Sans");

pub const BLACK: Color = Color::BLACK;
pub const WHITE: Color = Color::WHITE;
/// The InayOS accent, RGB 255, 78, 173.
pub const PINK: Color = Color::from_rgb8(255, 78, 173);
/// Text that explains or is unavailable.
pub const DIM: Color = Color::from_rgb8(170, 170, 170);
/// The background of a list row or a panel.
pub const SURFACE: Color = Color::from_rgb8(28, 28, 28);
pub const DANGER: Color = Color::from_rgb8(255, 90, 90);

/// The text size of ordinary text. The window is read from a distance on a laptop and
/// clicked with imprecise pointers, so everything is large.
pub const TEXT: u32 = 20;
pub const TITLE: u32 = 34;
/// The padding inside every button and list row, which makes the click targets large.
pub const PAD: u16 = 16;
/// The height of one list row, which keyboard scrolling uses to keep the selection visible.
pub const ROW: f32 = 58.0;

pub fn theme() -> Theme {
    Theme::custom(
        "InayOS Black and Pink",
        Palette {
            background: BLACK,
            text: WHITE,
            primary: PINK,
            success: Color::from_rgb8(120, 220, 140),
            warning: Color::from_rgb8(255, 200, 90),
            danger: DANGER,
        },
    )
}

/// A list row: pink when it is the selection, outlined in pink when its list has the
/// keyboard focus.
pub fn row(selected: bool, focused: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| {
        let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
        let disabled = matches!(status, button::Status::Disabled);
        let background = if selected {
            PINK
        } else if hovered {
            Color::from_rgb8(50, 50, 50)
        } else {
            SURFACE
        };
        let text_color = if selected {
            BLACK
        } else if disabled {
            DIM
        } else {
            WHITE
        };
        button::Style {
            background: Some(Background::Color(background)),
            text_color,
            border: Border {
                color: if focused && selected { WHITE } else { PINK },
                width: if focused && selected { 3.0 } else { 0.0 },
                radius: 8.0.into(),
            },
            ..button::Style::default()
        }
    }
}

/// The main action of a screen: pink with black text.
pub fn primary(theme: &Theme, status: button::Status) -> button::Style {
    let mut style = button::primary(theme, status);
    style.text_color = BLACK;
    style.border.radius = 8.0.into();
    if matches!(status, button::Status::Disabled) {
        style.background = Some(Background::Color(Color::from_rgb8(90, 40, 70)));
        style.text_color = DIM;
    }
    style
}

/// Every other button: outlined in pink.
pub fn secondary(_: &Theme, status: button::Status) -> button::Style {
    let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
    let disabled = matches!(status, button::Status::Disabled);
    button::Style {
        background: Some(Background::Color(if hovered {
            Color::from_rgb8(50, 50, 50)
        } else {
            BLACK
        })),
        text_color: if disabled { DIM } else { WHITE },
        border: Border {
            color: if disabled { DIM } else { PINK },
            width: 2.0,
            radius: 8.0.into(),
        },
        ..button::Style::default()
    }
}

/// A panel that groups a screen's message or log.
pub fn panel(_: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(SURFACE)),
        border: Border {
            color: SURFACE,
            width: 0.0,
            radius: 8.0.into(),
        },
        ..container::Style::default()
    }
}

/// A text field: outlined in grey, in pink with a thicker line while it has the text focus.
pub fn input(theme: &Theme, status: text_input::Status) -> text_input::Style {
    let mut style = text_input::default(theme, status);
    style.background = Background::Color(SURFACE);
    style.placeholder = DIM;
    style.value = WHITE;
    style.selection = Color { a: 0.5, ..PINK };
    style.border = match status {
        text_input::Status::Focused { .. } => Border {
            color: PINK,
            width: 3.0,
            radius: 8.0.into(),
        },
        _ => Border {
            color: DIM,
            width: 1.0,
            radius: 8.0.into(),
        },
    };
    style
}

/// A switch. The ones the installer shows are all unavailable, so the off state must stay
/// visible on black.
pub fn toggler(theme: &Theme, status: switch::Status) -> switch::Style {
    let mut style = switch::default(theme, status);
    style.background = Background::Color(Color::from_rgb8(60, 60, 60));
    style.foreground = Background::Color(DIM);
    style.text_color = Some(WHITE);
    style
}
