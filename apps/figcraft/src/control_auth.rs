//! Opt-in native control authentication. Tokens never enter logs, preferences or command journals.
use serde_json::{Value, json};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
pub struct Lease {
    path: PathBuf,
    token: String,
    active: AtomicBool,
}
impl Lease {
    pub fn create(dir: &Path, port: u16) -> Result<Self, String> {
        std::fs::create_dir_all(dir).map_err(|e| format!("control credentials: {e}"))?;
        let token = format!("{}{}", figcraft_collab_net::token::secret()?, figcraft_collab_net::token::secret()?);
        let path = dir.join(format!("control-{port}.token"));
        if std::fs::symlink_metadata(&path).is_ok_and(|m| !m.file_type().is_file()) {
            return Err("control credential path is not a regular file".into());
        }
        let temp = dir.join(format!(".control-{port}-{}.tmp", std::process::id()));
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let result = (|| {
            let mut file = options.open(&temp)?;
            file.write_all(token.as_bytes())?;
            file.sync_all()?;
            std::fs::rename(&temp, &path)
        })();
        if let Err(e) = result {
            let _ = std::fs::remove_file(&temp);
            return Err(format!("cannot publish control credential: {e}"));
        }
        Ok(Self { path, token, active: AtomicBool::new(true) })
    }
    pub fn active(&self) -> bool {
        self.active.load(Ordering::Acquire)
    }
    pub fn retire(&self) {
        self.active.store(false, Ordering::Release);
        if std::fs::read_to_string(&self.path).is_ok_and(|s| s == self.token) {
            let _ = std::fs::remove_file(&self.path);
        }
    }
    pub fn authenticate(&self, stream: &mut TcpStream) -> std::io::Result<bool> {
        let deadline = Instant::now() + Duration::from_secs(2);
        let mut line = Vec::with_capacity(256);
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() || line.len() >= 1024 || !self.active() {
                return Ok(false);
            }
            stream.set_read_timeout(Some(remaining))?;
            let mut b = [0];
            if stream.read(&mut b)? == 0 {
                return Ok(false);
            }
            if b[0] == b'\n' {
                break;
            }
            line.push(b[0]);
        }
        let valid = serde_json::from_slice::<Value>(&line).ok().is_some_and(|v| {
            v.as_object().is_some_and(|o| o.keys().all(|k| matches!(k.as_str(), "id" | "method" | "token")))
                && v["method"] == "auth"
                && v["token"].as_str().is_some_and(|t| figcraft_engine::figcraft_collab::same_secret(t, &self.token))
        });
        let reply = if valid {
            json!({"id":null,"ok":true,"result":{"authenticated":true,"protocol":1}})
        } else {
            json!({"id":null,"ok":false,"error":"control authentication required"})
        };
        writeln!(stream, "{reply}")?;
        Ok(valid)
    }
}
impl Drop for Lease {
    fn drop(&mut self) {
        self.retire()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn dir(tag: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("figcraft-auth-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }
    #[test]
    fn private_token_lease_never_erases_a_replacement() {
        let dir = dir("lease");
        let lease = Lease::create(&dir, 39791).unwrap();
        let path = dir.join("control-39791.token");
        let first = std::fs::read_to_string(&path).unwrap();
        assert_eq!(first.len(), 44);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        }
        let newer = Lease::create(&dir, 39791).unwrap();
        assert_ne!(std::fs::read_to_string(&path).unwrap(), first);
        lease.retire();
        assert!(path.exists());
        newer.retire();
        assert!(!path.exists());
        let _ = std::fs::remove_dir_all(dir);
    }
    #[cfg(unix)]
    #[test]
    fn linked_credential_target_is_rejected_without_touching_its_target() {
        use std::os::unix::fs::symlink;
        let dir = dir("linked");
        let target = dir.join("other");
        std::fs::write(&target, b"private").unwrap();
        symlink(&target, dir.join("control-39792.token")).unwrap();
        assert!(Lease::create(&dir, 39792).is_err());
        assert_eq!(std::fs::read(&target).unwrap(), b"private");
        let _ = std::fs::remove_dir_all(dir);
    }
}
