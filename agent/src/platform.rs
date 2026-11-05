//! Everything the agent does to the system goes through `Platform`.
//! With a root other than "/" (tests, dry runs) files are written below that
//! root and commands are only logged, never executed.

use std::path::{Path, PathBuf};
use std::process::Command;

pub struct Platform {
    root: PathBuf,
    execute: bool,
    pub log: Vec<String>,
}

impl Platform {
    pub fn new(root: &Path, dry_run: bool) -> Self {
        let execute = !dry_run && root == Path::new("/");
        Platform { root: root.to_path_buf(), execute, log: vec![] }
    }

    fn path(&self, p: &str) -> PathBuf {
        self.root.join(p.trim_start_matches('/'))
    }

    pub fn read(&self, p: &str) -> Option<String> {
        std::fs::read_to_string(self.path(p)).ok()
    }

    /// Write if the content differs. Returns true if something changed.
    pub fn write(&mut self, p: &str, content: &str, mode: u32) -> Result<bool, String> {
        self.write_bytes(p, content.as_bytes(), mode)
    }

    pub fn write_bytes(&mut self, p: &str, content: &[u8], mode: u32) -> Result<bool, String> {
        if std::fs::read(self.path(p)).ok().as_deref() == Some(content) {
            return Ok(false);
        }
        let full = self.path(p);
        if let Some(dir) = full.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("mkdir {}: {e}", dir.display()))?;
        }
        // write atomically: temp file + rename
        let tmp = full.with_extension("valo-tmp");
        std::fs::write(&tmp, content).map_err(|e| format!("write {}: {e}", full.display()))?;
        set_mode(&tmp, mode);
        if let Err(e) = std::fs::rename(&tmp, &full) {
            // e.g. bind-mounted files (/etc/hostname in containers): write in place
            let _ = std::fs::remove_file(&tmp);
            std::fs::write(&full, content).map_err(|e2| format!("write {}: {e} / {e2}", full.display()))?;
        }
        self.log.push(format!("wrote {p}"));
        Ok(true)
    }

    pub fn remove(&mut self, p: &str) -> Result<bool, String> {
        let full = self.path(p);
        if !full.exists() {
            return Ok(false);
        }
        std::fs::remove_file(&full).map_err(|e| format!("remove {p}: {e}"))?;
        self.log.push(format!("removed {p}"));
        Ok(true)
    }

    /// Point `link` at `target` (absolute target path on the system).
    pub fn symlink(&mut self, link: &str, target: &str) -> Result<bool, String> {
        let full = self.path(link);
        if std::fs::read_link(&full).ok().as_deref() == Some(Path::new(target)) {
            return Ok(false);
        }
        if let Some(dir) = full.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let _ = std::fs::remove_file(&full);
        #[cfg(unix)]
        std::os::unix::fs::symlink(target, &full).map_err(|e| format!("symlink {link}: {e}"))?;
        self.log.push(format!("linked {link} -> {target}"));
        Ok(true)
    }

    /// Run a command. In dry-run mode it is only logged and reported as success.
    pub fn run(&mut self, cmd: &str, args: &[&str]) -> Result<String, String> {
        let line = format!("{cmd} {}", args.join(" "));
        self.log.push(format!("run {line}"));
        if !self.execute {
            return Ok(String::new());
        }
        let out = Command::new(cmd).args(args).output().map_err(|e| format!("{cmd}: {e}"))?;
        if out.status.success() {
            Ok(String::from_utf8_lossy(&out.stdout).to_string())
        } else {
            let err = String::from_utf8_lossy(&out.stderr);
            Err(format!("{line}: {}", err.trim().lines().last().unwrap_or("failed")))
        }
    }

    /// Output of a read-only query command; None in dry-run or on failure.
    pub fn query(&mut self, cmd: &str, args: &[&str]) -> Option<String> {
        if !self.execute {
            return None;
        }
        let out = Command::new(cmd).args(args).output().ok()?;
        out.status.success().then(|| String::from_utf8_lossy(&out.stdout).to_string())
    }

    /// Run a command whose failure just means "no" (e.g. `id user`).
    pub fn check(&mut self, cmd: &str, args: &[&str]) -> bool {
        if !self.execute {
            self.log.push(format!("check {cmd} {}", args.join(" ")));
            return false;
        }
        Command::new(cmd).args(args).output().map(|o| o.status.success()).unwrap_or(false)
    }
}

#[cfg(unix)]
fn set_mode(p: &Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(p, std::fs::Permissions::from_mode(mode));
}
