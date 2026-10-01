use crate::engine::{Engine, Result};
use rusqlite::{StatementStatus, ToSql};
use std::time::Instant;

pub fn name(prefix: char, i: i64) -> Vec<u8> {
    format!("{prefix}{i:07}").into_bytes()
}
pub fn run(e: &mut Engine, n: i64) -> Result<()> {
    for i in 0..256 {
        let key = name('f', (i * 997) % n);
        let found = e.scalar(
            "SELECT ino FROM names WHERE w=1 AND parent=1 AND name=?1 AND dead=9223372036854775807",
            &[&key],
        )?;
        if found < 2 {
            return Err("lookup missing".into());
        }
    }
    let mut after = Vec::new();
    loop {
        let t = Instant::now();
        let sql="SELECT name,ino FROM names WHERE w=1 AND parent=1 AND name>?1 AND dead=9223372036854775807 ORDER BY name LIMIT 256";
        let mut s = e.db.prepare_cached(sql)?;
        for status in [
            StatementStatus::VmStep,
            StatementStatus::FullscanStep,
            StatementStatus::Sort,
        ] {
            s.reset_status(status);
        }
        let mut rows = s.query([&after])?;
        let (mut count, mut bytes) = (0, 0);
        while let Some(row) = rows.next()? {
            let key: Vec<u8> = row.get(0)?;
            let _: i64 = row.get(1)?;
            count += 1;
            bytes += key.len() + 8;
            after = key;
        }
        drop(rows);
        e.m.sql_calls += 1;
        e.m.sql_ns += t.elapsed().as_nanos();
        e.m.vm_steps += s.get_status(StatementStatus::VmStep) as u64;
        e.m.fullscan_steps += s.get_status(StatementStatus::FullscanStep) as u64;
        e.m.sorts += s.get_status(StatementStatus::Sort) as u64;
        e.m.sql_texts.insert(sql.to_owned());
        e.m.page_rows = e.m.page_rows.max(count);
        e.m.page_fields_bytes = e.m.page_fields_bytes.max(bytes);
        if count < 256 {
            break;
        }
    }
    for i in 0..64 {
        e.begin()?;
        let ino = n + 2 + i;
        e.inode(1, ino, 100000 + i, 0)?;
        let original = name('n', i);
        let alias = name('l', i);
        let renamed = name('r', i);
        for key in [&original, &alias] {
            e.exec(
                "INSERT INTO names VALUES(1,1,?1,1,9223372036854775807,?2)",
                &[key as &dyn ToSql, &ino],
            )?;
        }
        e.exec(
            "UPDATE names SET name=?2 WHERE w=1 AND parent=1 AND name=?1",
            &[&original, &renamed],
        )?;
        e.exec(
            "DELETE FROM names WHERE w=1 AND parent=1 AND name=?1",
            &[&alias],
        )?;
        e.commit()?;
    }
    Ok(())
}
pub fn deep(e: &mut Engine) -> Result<()> {
    for _ in 0..100 {
        let mut parent = 1;
        for i in 0..270 {
            parent=e.scalar("SELECT ino FROM names WHERE w=1 AND parent=?1 AND name=?2 AND dead=9223372036854775807",&[&parent,&name('d',i)])?;
        }
        if parent != 271 {
            return Err("wrong terminal inode".into());
        }
    }
    e.begin()?;
    e.exec(
        "UPDATE names SET name=?2 WHERE w=1 AND parent=1 AND name=?1",
        &[&name('d', 0), &name('r', 0)],
    )?;
    e.commit()
}
