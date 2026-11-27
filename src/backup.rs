use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const KEEP: usize = 10;

fn data_home() -> io::Result<PathBuf> {
    if let Some(dir) = std::env::var_os("XDG_DATA_HOME") {
        return Ok(PathBuf::from(dir));
    }
    let home = std::env::var_os("HOME")
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "HOME is not set"))?;
    Ok(PathBuf::from(home).join(".local").join("share"))
}

pub fn backup_dir() -> io::Result<PathBuf> {
    Ok(data_home()?.join("pico").join("backups"))
}

fn absolute(path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    }
}

fn key_for(path: &Path) -> String {
    let canonical = fs::canonicalize(path).unwrap_or_else(|_| absolute(path));
    let bytes = canonical.to_string_lossy();
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in bytes.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

fn slot_dir(path: &Path) -> io::Result<PathBuf> {
    let dir = backup_dir()?.join(key_for(path));
    fs::create_dir_all(&dir)?;
    let marker = dir.join("path.txt");
    if !marker.exists() {
        fs::write(marker, absolute(path).to_string_lossy().as_bytes())?;
    }
    Ok(dir)
}

pub fn before_save(path: &Path) -> io::Result<PathBuf> {
    if !path.exists() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "nothing to back up yet",
        ));
    }
    let dir = slot_dir(path)?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let name = format!("{stamp}-{}.bak", std::process::id());
    let destination = dir.join(name);
    fs::copy(path, &destination)?;

    let mut backups: Vec<PathBuf> = fs::read_dir(&dir)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("bak"))
        .collect();
    backups.sort();
    while backups.len() > KEEP {
        let old = backups.remove(0);
        let _ = fs::remove_file(old);
    }
    Ok(destination)
}

pub fn latest(path: &Path) -> io::Result<Option<PathBuf>> {
    let dir = backup_dir()?.join(key_for(path));
    if !dir.exists() {
        return Ok(None);
    }
    let mut backups: Vec<PathBuf> = fs::read_dir(dir)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("bak"))
        .collect();
    backups.sort();
    Ok(backups.pop())
}

#[cfg(test)]
mod tests {
    use super::{backup_dir, before_save, latest};
    use std::fs;

    #[test]
    fn backup_rotates_and_restores_latest() {
        let root = std::env::temp_dir().join(format!("pico-backup-test-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        unsafe { std::env::set_var("XDG_DATA_HOME", &root) };
        let file = root.join("note.txt");
        fs::write(&file, "first").unwrap();
        let first = before_save(&file).unwrap();
        assert!(first.exists());
        fs::write(&file, "second").unwrap();
        let second = before_save(&file).unwrap();
        assert_eq!(fs::read_to_string(second).unwrap(), "second");
        assert_eq!(fs::read_to_string(latest(&file).unwrap().unwrap()).unwrap(), "second");
        assert_eq!(fs::read_to_string(first).unwrap(), "first");
        assert!(backup_dir().unwrap().exists());
        unsafe { std::env::remove_var("XDG_DATA_HOME") };
        fs::remove_dir_all(root).unwrap();
    }
}
