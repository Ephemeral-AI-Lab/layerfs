//! Actual cell-codec and composed-window work; SQL delivery is counted separately.
/// Cumulative engine copy/initialization counts. These cover cell encoding and
/// composed reads, not driver/internal SQLite copies, kernel buffers or residency.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PayloadWork {
    pub write_input_bytes: u64,
    pub write_cells: u64,
    pub partial_write_cells: u64,
    pub cell_copy_bytes: u64,
    pub cell_zeroed_bytes: u64,
    pub read_window_zeroed_bytes: u64,
    pub read_local_copy_bytes: u64,
}
impl PayloadWork {
    pub fn since(self, before: Self) -> Self {
        Self {
            write_input_bytes: self
                .write_input_bytes
                .saturating_sub(before.write_input_bytes),
            write_cells: self.write_cells.saturating_sub(before.write_cells),
            partial_write_cells: self
                .partial_write_cells
                .saturating_sub(before.partial_write_cells),
            cell_copy_bytes: self.cell_copy_bytes.saturating_sub(before.cell_copy_bytes),
            cell_zeroed_bytes: self
                .cell_zeroed_bytes
                .saturating_sub(before.cell_zeroed_bytes),
            read_window_zeroed_bytes: self
                .read_window_zeroed_bytes
                .saturating_sub(before.read_window_zeroed_bytes),
            read_local_copy_bytes: self
                .read_local_copy_bytes
                .saturating_sub(before.read_local_copy_bytes),
        }
    }
    pub fn accumulate(&mut self, delta: Self) {
        self.write_input_bytes = self
            .write_input_bytes
            .saturating_add(delta.write_input_bytes);
        self.write_cells = self.write_cells.saturating_add(delta.write_cells);
        self.partial_write_cells = self
            .partial_write_cells
            .saturating_add(delta.partial_write_cells);
        self.cell_copy_bytes = self.cell_copy_bytes.saturating_add(delta.cell_copy_bytes);
        self.cell_zeroed_bytes = self
            .cell_zeroed_bytes
            .saturating_add(delta.cell_zeroed_bytes);
        self.read_window_zeroed_bytes = self
            .read_window_zeroed_bytes
            .saturating_add(delta.read_window_zeroed_bytes);
        self.read_local_copy_bytes = self
            .read_local_copy_bytes
            .saturating_add(delta.read_local_copy_bytes);
    }
}
