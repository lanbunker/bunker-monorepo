//! Runs one game: reset the hiscore file from its template, spawn the launcher
//! command, wait, read the file back. The command comes from the configuration,
//! so the same code runs `runcommand.sh` on the Pi and a shell script on a Mac.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use tracing::{info, warn};

use crate::cabinet::{Game, Outcome};
use crate::hiscore;

const ROM_PLACEHOLDER: &str = "{rom}";
const HISCORE_PLACEHOLDER: &str = "{hi}";
const HEX_DUMP_LEN: usize = 64;

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
    #[error("cannot create the hiscore folder {path}")]
    CreateDir {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("cannot copy the template {template} over {live}")]
    Template {
        template: PathBuf,
        live: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("cannot remove the stale hiscore file {path}")]
    RemoveStale {
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
    pub fn new(
        command: &str,
        rom_dir: PathBuf,
        hiscore_dir: PathBuf,
        template_dir: PathBuf,
    ) -> Result<Self, LauncherError> {
        let command: Vec<String> = command.split_whitespace().map(str::to_owned).collect();
        if command.is_empty() {
            return Err(LauncherError::EmptyCommand);
        }
        Ok(Self {
            command,
            rom_dir,
            hiscore_dir,
            template_dir,
        })
    }

    /// The live hiscore file of a game.
    pub fn hiscore_path(&self, game: &Game) -> PathBuf {
        self.hiscore_dir.join(format!("{}.hi", game.rom))
    }

    /// Blocks until the game exits. A missing or unreadable hiscore file is an
    /// `Outcome` with a note, so the player always gets a postgame screen. An
    /// error means the game never started.
    pub fn run(&self, game: &Game) -> Result<Outcome, LauncherError> {
        let live = self.hiscore_path(game);
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
        info!(rom = %game.rom, pid, code = ?status.code(), seconds, "game exited");
        if !status.success() {
            // A launcher that failed leaves the template on disk, and the
            // template decodes as a score of zero. That is not a run.
            warn!(rom = %game.rom, code = ?status.code(), "the launcher failed, no score is read");
            return Ok(Outcome {
                rom: game.rom.clone(),
                score: None,
                note: Some("The game did not run".to_owned()),
            });
        }

        Ok(read_outcome(game, &live, template.as_deref()))
    }

    /// Puts the template in place and returns its bytes, so the read-back can
    /// tell an untouched file from a real run.
    fn reset_hiscore(&self, game: &Game, live: &Path) -> Result<Option<Vec<u8>>, LauncherError> {
        std::fs::create_dir_all(&self.hiscore_dir).map_err(|source| LauncherError::CreateDir {
            path: self.hiscore_dir.clone(),
            source,
        })?;
        let template = self.template_dir.join(format!("{}.hi", game.rom));
        if template.is_file() {
            let bytes = std::fs::read(&template).map_err(|source| LauncherError::Template {
                template: template.clone(),
                live: live.to_owned(),
                source,
            })?;
            std::fs::write(live, &bytes).map_err(|source| LauncherError::Template {
                template: template.clone(),
                live: live.to_owned(),
                source,
            })?;
            info!(rom = %game.rom, template = %template.display(), live = %live.display(), bytes = bytes.len(), "hiscore reset from template");
            return Ok(Some(bytes));
        }
        // Without a template, a stale file from an earlier run would read as
        // the score of this run. No file is the safer start.
        match std::fs::remove_file(live) {
            Ok(()) => {
                warn!(rom = %game.rom, live = %live.display(), "no template, stale hiscore file removed")
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                warn!(rom = %game.rom, "no template and no live hiscore file, the game starts with its defaults")
            }
            Err(source) => {
                return Err(LauncherError::RemoveStale {
                    path: live.to_owned(),
                    source,
                });
            }
        }
        Ok(None)
    }
}

fn read_outcome(game: &Game, live: &Path, template: Option<&[u8]>) -> Outcome {
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
    if template.is_some_and(|template| template == bytes.as_slice()) {
        warn!(rom = %game.rom, live = %live.display(), "the hiscore file is still the template, the game wrote nothing");
        return Outcome {
            rom: game.rom.clone(),
            score: None,
            note: Some("The game wrote no new score".to_owned()),
        };
    }
    match hiscore::decode(game.decoder, &bytes) {
        Ok(score) => {
            info!(rom = %game.rom, bytes = bytes.len(), score = score.0, "hiscore decoded");
            Outcome {
                rom: game.rom.clone(),
                score: Some(score),
                note: None,
            }
        }
        Err(e) => {
            warn!(
                rom = %game.rom,
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

fn hex_head(bytes: &[u8]) -> String {
    bytes
        .iter()
        .take(HEX_DUMP_LEN)
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
}
