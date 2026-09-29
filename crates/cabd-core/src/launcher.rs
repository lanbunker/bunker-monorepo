//! Runs one game: reset the hiscore file from its template, spawn the launcher
//! command, wait, read the file back. The command comes from the configuration,
//! so the same code runs `runcommand.sh` on the Pi and a shell script on a Mac.

use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};
use std::time::Instant;

use tracing::{info, warn};

use crate::cabinet::{Game, Outcome};
use crate::hiscore;

const ROM_PLACEHOLDER: &str = "{rom}";
const HISCORE_PLACEHOLDER: &str = "{hi}";
const HEX_DUMP_LEN: usize = 64;

/// The command that runs a game, and the folders around it.
#[derive(Debug, Clone)]
pub struct Launcher {
    command: Vec<String>,
    rom_dir: PathBuf,
    hiscore_dir: PathBuf,
    template_dir: PathBuf,
}

#[derive(Debug, thiserror::Error)]
pub enum LauncherError {
    #[error("the launcher command is empty")]
    EmptyCommand,
    #[error("the launcher command has no `{{rom}}` placeholder")]
    NoRomPlaceholder,
    #[error("cannot create the hiscore folder {path}")]
    CreateDir {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("cannot read the template {path}")]
    ReadTemplate {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("cannot write the hiscore file {path}")]
    WriteHiscore {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("cannot start `{program}`")]
    Spawn {
        program: String,
        #[source]
        source: std::io::Error,
    },
    #[error("lost the launcher process while waiting for it")]
    Wait(#[source] std::io::Error),
}

impl Launcher {
    /// `command` is split on whitespace. `{rom}` must appear in it and becomes
    /// the ROM path. `{hi}` becomes the live hiscore file.
    pub fn try_new(
        command: &str,
        rom_dir: PathBuf,
        hiscore_dir: PathBuf,
        template_dir: PathBuf,
    ) -> Result<Self, LauncherError> {
        let command: Vec<String> = command.split_whitespace().map(str::to_owned).collect();
        if command.is_empty() {
            return Err(LauncherError::EmptyCommand);
        }
        if !command.iter().any(|token| token.contains(ROM_PLACEHOLDER)) {
            return Err(LauncherError::NoRomPlaceholder);
        }
        Ok(Self {
            command,
            rom_dir,
            hiscore_dir,
            template_dir,
        })
    }

    /// Blocks until the game exits. A missing or unreadable hiscore file is an
    /// `Outcome` with a note, so the player always gets a postgame screen. An
    /// error means the game never started.
    pub fn run(&self, game: &Game) -> Result<Outcome, LauncherError> {
        let live = self.hiscore_dir.join(format!("{}.hi", game.rom));
        let template = self.reset_hiscore(game, &live)?;

        let rom_path = self.rom_dir.join(format!("{}.zip", game.rom));
        let argv: Vec<String> = self
            .command
            .iter()
            .map(|token| {
                token
                    .replace(ROM_PLACEHOLDER, &rom_path.to_string_lossy())
                    .replace(HISCORE_PLACEHOLDER, &live.to_string_lossy())
            })
            .collect();
        let (program, args) = argv.split_first().ok_or(LauncherError::EmptyCommand)?;

        info!(rom = %game.rom, command = %argv.join(" "), "launching");
        let started = Instant::now();
        let mut child =
            Command::new(program)
                .args(args)
                .spawn()
                .map_err(|source| LauncherError::Spawn {
                    program: program.clone(),
                    source,
                })?;
        let pid = child.id();
        let status = child.wait().map_err(LauncherError::Wait)?;
        let seconds = started.elapsed().as_secs_f64();
        if status.success() {
            info!(rom = %game.rom, pid, seconds, "game exited");
        } else {
            // The core writes the file before RetroArch shuts down, so a bad
            // exit code can still come with a real score. The file decides.
            warn!(rom = %game.rom, pid, code = ?status.code(), signal = ?signal_of(status), seconds, "the launcher exited with a failure");
        }

        let Some(template) = template else {
            return Ok(Outcome {
                rom: game.rom.clone(),
                score: None,
                note: Some("No score template for this game".to_owned()),
            });
        };
        Ok(read_outcome(game, &live, &template))
    }

    /// Puts the template in place and returns its bytes, so the read-back can
    /// tell an untouched file from a real run. Without a template there is no
    /// reset, and the run cannot give a score: the game would write its own
    /// default table, and the default top entry would read as the score.
    fn reset_hiscore(&self, game: &Game, live: &Path) -> Result<Option<Vec<u8>>, LauncherError> {
        std::fs::create_dir_all(&self.hiscore_dir).map_err(|source| LauncherError::CreateDir {
            path: self.hiscore_dir.clone(),
            source,
        })?;
        let template = self.template_dir.join(format!("{}.hi", game.rom));
        let bytes = match std::fs::read(&template) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                warn!(rom = %game.rom, template = %template.display(), "no template, the run gives no score");
                return Ok(None);
            }
            Err(source) => {
                return Err(LauncherError::ReadTemplate {
                    path: template,
                    source,
                });
            }
        };
        std::fs::write(live, &bytes).map_err(|source| LauncherError::WriteHiscore {
            path: live.to_owned(),
            source,
        })?;
        info!(rom = %game.rom, template = %template.display(), live = %live.display(), bytes = bytes.len(), "hiscore reset from template");
        Ok(Some(bytes))
    }
}

fn read_outcome(game: &Game, live: &Path, template: &[u8]) -> Outcome {
    let bytes = match std::fs::read(live) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            warn!(rom = %game.rom, live = %live.display(), "no hiscore file after the game");
            return Outcome {
                rom: game.rom.clone(),
                score: None,
                note: Some("The game wrote no score file".to_owned()),
            };
        }
        Err(e) => {
            warn!(rom = %game.rom, live = %live.display(), error = %e, "cannot read the hiscore file");
            return Outcome {
                rom: game.rom.clone(),
                score: None,
                note: Some("The score file could not be read".to_owned()),
            };
        }
    };
    if bytes == template {
        warn!(rom = %game.rom, live = %live.display(), "the hiscore file is still the template, the game wrote nothing");
        return Outcome {
            rom: game.rom.clone(),
            score: None,
            note: Some("The game wrote no new score".to_owned()),
        };
    }
    if bytes.len() != template.len() {
        // The core writes the same ranges every time, so another size is a
        // cut write or a wrong template, never a score.
        warn!(rom = %game.rom, live = %live.display(), bytes = bytes.len(), template = template.len(), "the hiscore file has another size than the template");
        return Outcome {
            rom: game.rom.clone(),
            score: None,
            note: Some("The score file has an unexpected size".to_owned()),
        };
    }
    match hiscore::decode(game.decoder, &bytes) {
        Ok(score) => {
            info!(rom = %game.rom, live = %live.display(), bytes = bytes.len(), score = %score, "hiscore decoded");
            Outcome {
                rom: game.rom.clone(),
                score: Some(score),
                note: None,
            }
        }
        Err(e) => {
            warn!(
                rom = %game.rom,
                live = %live.display(),
                bytes = bytes.len(),
                head = %hex_head(&bytes),
                error = %e,
                "cannot decode the hiscore file"
            );
            Outcome {
                rom: game.rom.clone(),
                score: None,
                note: Some("The score file could not be decoded".to_owned()),
            }
        }
    }
}

#[cfg(unix)]
fn signal_of(status: ExitStatus) -> Option<i32> {
    use std::os::unix::process::ExitStatusExt;
    status.signal()
}

#[cfg(not(unix))]
fn signal_of(_status: ExitStatus) -> Option<i32> {
    None
}

fn hex_head(bytes: &[u8]) -> String {
    bytes
        .iter()
        .take(HEX_DUMP_LEN)
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
}
