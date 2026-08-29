fn main() -> std::io::Result<()> {
    let mut terminal = ratatui::init();
    let result = layerfs_tui::run(&mut terminal);
    ratatui::restore();
    result
}
