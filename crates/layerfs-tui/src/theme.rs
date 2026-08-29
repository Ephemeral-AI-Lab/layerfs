use ratatui::style::Color;

#[derive(Clone, Copy)]
pub(crate) struct Palette {
    pub(crate) canvas: Color,
    pub(crate) surface: Color,
    pub(crate) border: Color,
    pub(crate) focus: Color,
    pub(crate) text: Color,
    pub(crate) secondary: Color,
    pub(crate) muted: Color,
    pub(crate) layer: Color,
    pub(crate) error: Color,
}

pub(crate) fn palette() -> Palette {
    if std::env::var_os("NO_COLOR").is_some() {
        return Palette {
            canvas: Color::Reset,
            surface: Color::Reset,
            border: Color::Reset,
            focus: Color::Reset,
            text: Color::Reset,
            secondary: Color::Reset,
            muted: Color::Reset,
            layer: Color::Reset,
            error: Color::Reset,
        };
    }

    let truecolor = std::env::var("COLORTERM").is_ok_and(|value| {
        value.eq_ignore_ascii_case("truecolor") || value.eq_ignore_ascii_case("24bit")
    });
    if truecolor {
        return Palette {
            canvas: Color::Rgb(255, 255, 255),
            surface: Color::Rgb(246, 248, 251),
            border: Color::Rgb(199, 208, 220),
            focus: Color::Rgb(32, 94, 177),
            text: Color::Rgb(13, 24, 45),
            secondary: Color::Rgb(70, 85, 108),
            muted: Color::Rgb(93, 105, 124),
            layer: Color::Rgb(13, 24, 45),
            error: Color::Rgb(183, 28, 28),
        };
    }

    if std::env::var("TERM").is_ok_and(|value| value.contains("256color")) {
        return Palette {
            canvas: Color::Indexed(231),
            surface: Color::Indexed(255),
            border: Color::Indexed(250),
            focus: Color::Indexed(25),
            text: Color::Indexed(233),
            secondary: Color::Indexed(238),
            muted: Color::Indexed(241),
            layer: Color::Indexed(17),
            error: Color::Indexed(124),
        };
    }

    Palette {
        canvas: Color::White,
        surface: Color::Gray,
        border: Color::DarkGray,
        focus: Color::Blue,
        text: Color::Black,
        secondary: Color::DarkGray,
        muted: Color::DarkGray,
        layer: Color::Black,
        error: Color::Red,
    }
}
