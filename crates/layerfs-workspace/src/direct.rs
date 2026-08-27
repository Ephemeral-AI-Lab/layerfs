use crate::{Presentation, Result, WorkspaceDriver, WorkspaceError};
use std::path::Path;

#[derive(Default)]
pub struct DirectDriver {
    frozen: bool,
}

impl WorkspaceDriver for DirectDriver {
    fn presentation(&self) -> Presentation {
        Presentation::Direct
    }

    fn view_path(&self) -> Option<&Path> {
        None
    }

    fn freeze(&mut self) -> Result<()> {
        if self.frozen {
            return Err(WorkspaceError::InvalidState);
        }
        self.frozen = true;
        Ok(())
    }

    fn cleanup(&mut self) -> Result<()> {
        self.frozen = true;
        Ok(())
    }
}
