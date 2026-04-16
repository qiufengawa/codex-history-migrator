use std::path::Path;

use anyhow::Result;
use rusqlite::{Connection, DatabaseName, backup::Progress};

pub fn open_connection(path: &Path) -> Result<Connection> {
    let conn = Connection::open(path)?;
    conn.busy_timeout(std::time::Duration::from_secs(30))?;
    Ok(conn)
}

pub fn backup_database(source_path: &Path, backup_path: &Path) -> Result<()> {
    let source = open_connection(source_path)?;
    source.backup(DatabaseName::Main, backup_path, None::<fn(Progress)>)?;
    Ok(())
}

pub fn restore_database(target_path: &Path, backup_path: &Path) -> Result<()> {
    let mut target = open_connection(target_path)?;
    target.restore(DatabaseName::Main, backup_path, None::<fn(Progress)>)?;
    Ok(())
}
