//! Owns the `mxbikes.exe` child process.
//!
//! The agent spawns the game itself rather than managing whatever process happens to be
//! running. That ownership is what makes the rest possible: a `Child` handle gives exit
//! detection without polling the process table, so a crashed server can be brought back
//! automatically, and "restart" is not a race between a kill and someone else's respawn.

use crate::config::{Config, ServerKind};
use serde::Serialize;
use std::process::{Child, Command};
use std::time::Instant;

pub struct Supervisor {
    cfg: Config,
    child: Option<Child>,
    started_at: Option<Instant>,
    /// Times the game came back after exiting on its own. A climbing count with a low
    /// uptime is the signature of a server crash-looping on bad config.
    restarts: u32,
    /// Set while an operator-requested stop is in flight, so the crash watcher doesn't
    /// treat a deliberate shutdown as a failure and immediately undo it.
    stopping: bool,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct Status {
    pub running: bool,
    pub pid: Option<u32>,
    pub uptime_secs: u64,
    pub restarts: u32,
}

impl Supervisor {
    pub fn new(cfg: Config) -> Self {
        Self {
            cfg,
            child: None,
            started_at: None,
            restarts: 0,
            stopping: false,
        }
    }

    pub fn config(&self) -> &Config {
        &self.cfg
    }

    /// Start the game unless it's already up.
    pub fn start(&mut self) -> Result<(), String> {
        if self.is_alive() {
            return Ok(());
        }
        let exe = self.cfg.exe_path();
        if !exe.is_file() {
            return Err(format!("{} not found", exe.display()));
        }
        let mut command = Command::new(&exe);
        command.current_dir(&self.cfg.game_dir);
        match self.cfg.kind {
            ServerKind::Stock => {
                command.args([
                    "-dedicated",
                    &self.cfg.game_port.to_string(),
                    "-set",
                    "params",
                    &self.cfg.ini,
                    "-log",
                ]);
            }
            ServerKind::Native => {
                command.args(["--config", &self.cfg.native_config]);
            }
        }
        let child = command
            .spawn()
            .map_err(|e| format!("couldn't start {}: {e}", exe.display()))?;
        self.child = Some(child);
        self.started_at = Some(Instant::now());
        self.stopping = false;
        Ok(())
    }

    /// Stop the game if it's up. Idempotent.
    pub fn stop(&mut self) -> Result<(), String> {
        self.stopping = true;
        if let Some(child) = self.child.as_mut() {
            let _ = child.kill();
            // Reap it, so the OS isn't left holding a zombie and `is_alive` settles.
            let _ = child.wait();
        }
        self.child = None;
        self.started_at = None;
        Ok(())
    }

    pub fn restart(&mut self) -> Result<(), String> {
        self.stop()?;
        self.start()
    }

    /// Whether the child is still running, reaping it if it has exited.
    pub fn is_alive(&mut self) -> bool {
        let Some(child) = self.child.as_mut() else {
            return false;
        };
        match child.try_wait() {
            // Exited: clear the handle so a later start isn't refused by a dead child.
            Ok(Some(_)) => {
                self.child = None;
                self.started_at = None;
                false
            }
            Ok(None) => true,
            // We can't tell; assume gone rather than wedging on a handle we can't query.
            Err(_) => {
                self.child = None;
                self.started_at = None;
                false
            }
        }
    }

    /// Bring the game back if it exited on its own. Called on a timer.
    ///
    /// Returns whether a restart happened, so the caller can log it.
    pub fn revive_if_crashed(&mut self) -> bool {
        if self.stopping || self.is_alive() {
            return false;
        }
        if self.start().is_ok() {
            self.restarts += 1;
            return true;
        }
        false
    }

    pub fn status(&mut self) -> Status {
        let running = self.is_alive();
        Status {
            running,
            pid: if running {
                self.child.as_ref().map(|c| c.id())
            } else {
                None
            },
            uptime_secs: if running {
                self.started_at.map(|t| t.elapsed().as_secs()).unwrap_or(0)
            } else {
                0
            },
            restarts: self.restarts,
        }
    }

    pub fn read_ini(&self) -> Result<String, String> {
        let path = self.cfg.ini_path();
        std::fs::read_to_string(&path).map_err(|e| format!("couldn't read {}: {e}", path.display()))
    }

    pub fn write_ini(&self, text: &str) -> Result<(), String> {
        let path = self.cfg.ini_path();
        std::fs::write(&path, text).map_err(|e| format!("couldn't write {}: {e}", path.display()))
    }

    pub fn read_server_config(&self) -> Result<String, String> {
        let path = self.cfg.server_config_path();
        std::fs::read_to_string(&path).map_err(|e| format!("couldn't read {}: {e}", path.display()))
    }

    pub fn write_server_config(&self, text: &str) -> Result<(), String> {
        let path = self.cfg.server_config_path();
        let temporary = path.with_extension("tmp");
        let backup = path.with_extension("previous");
        std::fs::write(&temporary, text)
            .map_err(|e| format!("couldn't write {}: {e}", temporary.display()))?;
        let existed = path.exists();
        if existed {
            let _ = std::fs::remove_file(&backup);
            std::fs::rename(&path, &backup)
                .map_err(|e| format!("couldn't back up {}: {e}", path.display()))?;
        }
        if let Err(error) = std::fs::rename(&temporary, &path) {
            if existed {
                let _ = std::fs::rename(&backup, &path);
            }
            return Err(format!("couldn't replace {}: {error}", path.display()));
        }
        if existed {
            let _ = std::fs::remove_file(backup);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn cfg_for(dir: PathBuf) -> Config {
        // Parsed rather than constructed so the test also covers the defaults.
        let json = format!(
            r#"{{"token":"t","game_dir":{},"ini":"dedicated.ini"}}"#,
            serde_json::to_string(&dir).unwrap()
        );
        serde_json::from_str(&json).unwrap()
    }

    #[test]
    fn a_fresh_supervisor_reports_stopped() {
        let mut s = Supervisor::new(cfg_for(std::env::temp_dir()));
        let st = s.status();
        assert!(!st.running);
        assert_eq!(st.pid, None);
        assert_eq!(st.uptime_secs, 0);
        assert_eq!(st.restarts, 0);
    }

    #[test]
    fn starting_without_the_exe_says_which_path_was_missing() {
        let dir = std::env::temp_dir().join("mxb-agent-no-exe");
        let _ = std::fs::create_dir_all(&dir);
        let mut s = Supervisor::new(cfg_for(dir.clone()));
        let err = s.start().unwrap_err();
        assert!(err.contains("mxbikes.exe"), "unhelpful error: {err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn stop_is_idempotent_on_a_server_that_never_started() {
        let mut s = Supervisor::new(cfg_for(std::env::temp_dir()));
        assert!(s.stop().is_ok());
        assert!(s.stop().is_ok());
    }

    #[test]
    fn a_deliberate_stop_is_not_revived() {
        // Otherwise the crash watcher would fight the operator: stop the server from the
        // app, and it comes straight back a second later.
        let mut s = Supervisor::new(cfg_for(std::env::temp_dir()));
        s.stop().unwrap();
        assert!(!s.revive_if_crashed());
        assert_eq!(s.status().restarts, 0);
    }

    #[test]
    fn reading_a_missing_ini_names_the_path() {
        let s = Supervisor::new(cfg_for(std::env::temp_dir().join("mxb-agent-absent")));
        let err = s.read_ini().unwrap_err();
        assert!(err.contains("dedicated.ini"), "unhelpful error: {err}");
    }
}
