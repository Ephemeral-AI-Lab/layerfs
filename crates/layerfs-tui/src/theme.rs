use ratatui::style::{Color, Modifier, Style};

#[derive(Clone, Copy)]
pub struct Theme {
    color: bool,
}

impl Theme {
    pub fn detect() -> Self {
        Self {
            color: std::env::var_os("NO_COLOR").is_none(),
        }
    }

    pub const fn with_color(color: bool) -> Self {
        Self { color }
    }

    pub fn title(self) -> Style {
        self.style(Color::Cyan).add_modifier(Modifier::BOLD)
    }

    pub fn muted(self) -> Style {
        if self.color {
            Style::default().fg(Color::DarkGray)
        } else {
            Style::default().add_modifier(Modifier::DIM)
        }
    }

    pub fn selected(self) -> Style {
        if self.color {
            self.style(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().add_modifier(Modifier::BOLD | Modifier::REVERSED)
        }
    }

    pub fn focus(self) -> Style {
        self.style(Color::Cyan).add_modifier(Modifier::BOLD)
    }

    pub fn success(self) -> Style {
        self.style(Color::Green).add_modifier(Modifier::BOLD)
    }

    pub fn warning(self) -> Style {
        self.style(Color::Yellow).add_modifier(Modifier::BOLD)
    }

    pub fn error(self) -> Style {
        self.style(Color::Red).add_modifier(Modifier::BOLD)
    }

    pub fn info(self) -> Style {
        self.style(Color::Blue)
    }

    fn style(self, color: Color) -> Style {
        if self.color {
            Style::default().fg(color)
        } else {
            Style::default()
        }
    }
}
