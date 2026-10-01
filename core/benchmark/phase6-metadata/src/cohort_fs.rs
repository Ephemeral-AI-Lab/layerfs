use crate::engine::{Engine, Result};

impl Engine {
    pub fn cohort_source(&mut self, id: i64, data: &[u8], fill: bool, length: i64) -> Result<()> {
        self.exec(
            "INSERT INTO sources VALUES(?1,?2,?3,?4)",
            &[&id, &i64::from(fill), &data, &length],
        )?;
        Ok(())
    }
    pub fn touch_dir(&mut self, ino: i64) -> Result<()> {
        let kind = self.scalar("SELECT kind FROM kinds WHERE ino=?1", &[&ino])?;
        if kind != 1 {
            return Err("parent is not directory".into());
        }
        self.inode(1, ino, -1, 0)
    }
    pub fn cohort_create(
        &mut self,
        parent: i64,
        name: &[u8],
        ino: i64,
        payload: Option<&[u8]>,
    ) -> Result<()> {
        self.begin()?;
        self.touch_dir(parent)?;
        let g = self.scalar("SELECT active FROM workspace WHERE w=1", &[])?;
        let (kind, value, size) = match payload {
            None => (1, -1, 0),
            Some(bytes) => {
                self.cohort_source(ino, bytes, false, bytes.len() as i64)?;
                (2, ino, bytes.len() as i64)
            }
        };
        self.exec("INSERT INTO kinds VALUES(?1,?2)", &[&ino, &kind])?;
        self.inode(1, ino, value, size)?;
        self.exec(
            "INSERT INTO names VALUES(1,?1,?2,?3,9223372036854775807,?4)",
            &[&parent, &name, &g, &ino],
        )?;
        self.commit()
    }
    fn unbind(&mut self, parent: i64, name: &[u8], g: i64) -> Result<Option<i64>> {
        let rows=self.ints("SELECT ino,born FROM names WHERE w=1 AND parent=?1 AND name=?2 AND dead=9223372036854775807",&[&parent,&name])?;
        let Some(r) = rows.first() else {
            return Ok(None);
        };
        if r[1] == g {
            self.exec(
                "DELETE FROM names WHERE w=1 AND parent=?1 AND name=?2 AND born=?3",
                &[&parent, &name, &g],
            )?;
        } else {
            self.exec("UPDATE names SET dead=?3 WHERE w=1 AND parent=?1 AND name=?2 AND dead=9223372036854775807",&[&parent,&name,&g])?;
        }
        Ok(Some(r[0]))
    }
    pub fn cohort_move(&mut self, parent: i64, name: &[u8], dest: i64, new: &[u8]) -> Result<()> {
        self.begin()?;
        self.touch_dir(parent)?;
        if dest != parent {
            self.touch_dir(dest)?;
        }
        let g = self.scalar("SELECT active FROM workspace WHERE w=1", &[])?;
        let inode = self.unbind(parent, name, g)?.ok_or("missing move source")?;
        if let Some(displaced) = self.unbind(dest, new, g)? {
            self.exec("INSERT INTO retained_orphans VALUES(?1)", &[&displaced])?;
        }
        self.exec(
            "INSERT INTO names VALUES(1,?1,?2,?3,9223372036854775807,?4)",
            &[&dest, &new, &g, &inode],
        )?;
        self.commit()
    }
}
