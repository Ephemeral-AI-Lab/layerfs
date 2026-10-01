//! Fixed operation counters for SQL-native streamed construction.
#[derive(Clone, Copy, Default, Debug)]
pub struct QueryCounts {
    pub directory_lookups: u64,
    pub inode_lookups: u64,
    pub fresh_lookups: u64,
    pub name_rows: u64,
}
impl QueryCounts {
    pub fn since(self, before: Self) -> Self {
        Self {
            directory_lookups: self.directory_lookups - before.directory_lookups,
            inode_lookups: self.inode_lookups - before.inode_lookups,
            fresh_lookups: self.fresh_lookups - before.fresh_lookups,
            name_rows: self.name_rows - before.name_rows,
        }
    }
}
