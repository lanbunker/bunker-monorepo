# bunker-cabd design

The cabinet software for LAN BUNKER. This document records the decisions. The
code follows it. The components table says what exists.

The goals, in order: reliable at the party, easy to install on a fresh RetroPie
image, easy to develop and test on a Mac, readable. Nothing here exists for a
future that has not arrived.

## What this is

A small arcade cabinet on a Raspberry Pi 3 with RetroPie. It runs a curated set
of six arcade games from the 80s and 90s, single stick only. A player checks in
with a QR code on the phone, plays, and the high score goes to the player profile
in `bunker-api`. Each game has a leaderboard.

## Components

| Name | What it is | Status |
| --- | --- | --- |
| `bunker-api` | The Rust HTTP API: players, auth, tournaments | exists |
| `bunker-api` scores | Scores, leaderboards, cabinet check-in endpoints | to build |
| `web/` | The site, used from the phone for check-in | exists, check-in to build |
| `bunker-models` | Domain types shared by every Rust crate | exists |
| `cabd-core` | The cabinet software as a library | state machine, launcher, conductor exist. Outbox, API client, real decoders to build |
| `bunker-cabd` | The binary: `main` and the SDL2 screen | attract, select, postgame and `screenshots` exist. Service screen to build |

The daemon and the screen are one process. There is no separate frontend
program and no theme. Pegasus was the first plan. The official builds are
32-bit only, the last release is from 2024, and the engine is destroyed on each
game launch. For four screens, SDL2 is smaller and safer.

## The target

The cabinet starts from the RetroPie 4.8 image for the Pi 3 and nothing else.
Facts about that image, to verify on the box with the commands in the install
section:

| Fact | Value | Why it matters |
| --- | --- | --- |
| Base | Raspberry Pi OS Buster, 32-bit | the cross image starts from `debian:buster` |
| glibc | 2.28 | a binary built against a newer glibc does not start |
| Rust target | `armv7-unknown-linux-gnueabihf` | |
| Graphics | legacy firmware, dispmanx, no KMS | SDL uses the `rpi` video driver |
| SDL2 | the RetroPie build of 2.0.x with the `rpi` driver | the binary links it dynamically |
| Launch | `/opt/retropie/supplementary/runcommand/runcommand.sh` | sets the video mode, runs the hooks |
| Autostart | `/opt/retropie/configs/all/autostart.sh` | runs `emulationstation` on the console after login |
| FBNeo saves | `<save dir>/fbneo/<game>.hi` | RetroPie saves next to the ROM |

## Stack

| Part | Choice | Reason |
| --- | --- | --- |
| Cabinet OS | RetroPie 4.8 on a Raspberry Pi 3 | it owns RetroArch, the video mode and the controller configs |
| Emulation | RetroArch, `lr-fbneo`, `hiscore.dat` on | one core for all six games |
| Launch | `runcommand.sh` | it sets the video mode and restores the console |
| Screen | SDL2 with `sdl2`, `sdl2_ttf`, `sdl2_image` | runs on the console without X11, the same pattern as EmulationStation |
| Daemon | Rust, std threads and channels, `rusqlite`, `ureq`, `tracing` | no async runtime, nothing to schedule |
| Build | `cross` on macOS, deploy with rsync | the Pi 3 never compiles |

## Decisions

**All logic lives in `cabinet`. The screen draws a `ViewModel` and nothing else.**
`cabinet` is a state machine with no I/O, no threads, no SDL and no clock of its
own. It takes an `Event` and the current time. It returns the new `Frame` and a
list of `Effect` values. The conductor executes the effects. The screen draws the
frame. The screen computes no rank, no format and no navigation decision. It maps
a button to an `Input` and sends it.

**One binary, a few threads, two channels.**
SDL must run on the main thread. The conductor runs on a second thread and owns
`cabinet`, the outbox, the API client and the launcher. One channel carries
`Event` values into the conductor, from the screen and from the worker threads.
One channel carries each new `Frame` to the screen, and the screen keeps the
latest. Both are `std::sync::mpsc`. There is no HTTP and no async runtime inside
the process.

**The cabinet is the only thing that talks to `bunker-api`.**
It holds the token. A score goes to a local SQLite outbox first and is
acknowledged at once. A thread drains the outbox with retry. A network failure
or a laptop failure during the party loses no run.

**Per-launch hiscore reset by template copy.**
Before the launch, the conductor copies a pristine `.hi` template over the live
file. After the exit, it decodes the live file. The reason is the default table.
A game ships a high score table with entries such as 20000. A run below the
lowest entry never enters the table, and the file never sees it. The template
holds a table of zeros, so every run enters at rank one and the top entry after
the run is the score of this run.

**Check-in is QR plus phone, per session, not per launch.**
The screen shows a QR code with a short-lived nonce. The phone opens the site,
the player confirms, and `bunker-api` binds the nonce to the player. The
conductor waits on a long poll and opens the session. Guest mode is the default state,
and a guest run is stored with no player.

**One video mode per boot, set by the operating system.**
The binary never changes the mode. RetroArch runs in the same mode. The screen
reads the mode from SDL at startup and adapts the canvas to it. The display
section gives the rules.

## The process

```
main thread (bunker-cabd)            conductor thread (cabd-core)
┌──────────────────────┐             ┌───────────────────────────────────┐
│ screen (SDL2)        │  Event      │ loop: recv_timeout(tick)          │
│  poll SDL events     │ ─────────>  │       Event -> cabinet -> Frame   │
│  draw the Frame      │  <────────  │       Effect -> executor          │
│  60 fps              │  Frame      │ threads: outbox drainer,          │
└──────────────────────┘             │          check-in long poll       │
                                     └───────────────────────────────────┘
```

`cabinet` owns these types:

- `Event`: `Input(Input)`, `Tick`, `NonceIssued`, `CheckedIn(player)`,
  `DisplayReleased`, `GameEnded(outcome)`, `Logout`, `Command(DevCommand)`.
- `Effect`: `IssueNonce`, `PrepareAndLaunch(game)`, `QueueScore(run)`,
  `Restart`, `Shutdown`.
- `Frame`: `Show(ViewModel)` or `Suspended`.
- `ViewModel`: `screen` plus display-ready fields, such as `score_display`,
  `rank`, `is_pb`, `qr_payload`, `leaderboard: Vec<Row>`. Strings, not numbers
  to format.

The conductor loop is `recv_timeout` on the event channel, and the timeout is
the tick. The conductor blocks while a game runs. Nothing else needs it then:
the window is gone, and the outbox drainer is its own thread.

## Game launch

1. The player selects a game. The screen sends `Input::Confirm`.
2. `cabinet` returns `Frame::Suspended` and `Effect::PrepareAndLaunch(game)`.
3. The screen sees `Suspended`, destroys the window and the renderer, closes the
   joystick, and sends `Event::DisplayReleased`.
4. The conductor waits for `DisplayReleased`, copies the template, then spawns
   the launcher and waits for the exit code.
5. The conductor reads the `.hi` file, decodes the score, and sends itself
   `Event::GameEnded(outcome)`. A missing or corrupt file is an outcome too.
6. `cabinet` ranks the score, returns `Frame::Show(postgame)` and
   `Effect::QueueScore(run)`.
7. The screen sees `Show`, creates the window again and draws.

The launcher is a command line from the configuration. On the Pi it is
`runcommand.sh 0 _SYS_ arcade {rom}`. On macOS it is a shell script in `dev/`.
One script copies a fixture `.hi` and exits after a delay. Another script runs
the real RetroArch. There is one launcher implementation and no trait.

Two processes cannot own the display at the same time. Step 3 releases it.
EmulationStation does the same in `FileData::launchGame`.

## Score capture

Facts about `lr-fbneo`, from the source and the RetroPie module:

- `hiscore.dat` lives in `~/RetroPie/BIOS/fbneo/`. The core option
  `fbneo-hiscores` must be on. Runahead must be off.
- The `.hi` file is written **only on exit**, from `HiscoreExit`. A "restart"
  in the RetroArch menu writes nothing, by design. A killed process writes
  nothing.
- The write needs `HiscoreOkToWrite`: the ranges were loaded and applied, or the
  start and end marker bytes matched in RAM at least once. A template file
  satisfies the first condition.
- The path is `<save dir>/fbneo/<game>.hi`. RetroPie saves next to the ROM, so
  expect `~/RetroPie/roms/arcade/fbneo/<game>.hi`. Not verified on a Pi.
- The format is the raw concatenation of the RAM ranges, in `hiscore.dat`
  order, with no header. It is the MAME `.hi` format. The `hi2txt` XML files
  give the decoding per game: offset, length, BCD or binary, byte order.
- All six games have a `hiscore.dat` entry and the `BDF_HISCORE_SUPPORTED`
  driver flag.

The decoder is a pure function: `decode(game, bytes) -> Result<Score, HiscoreError>`.
Each game has a golden fixture: a real `.hi` file with a known score, committed
under `tests/fixtures/`. The template of a game is the fixture with the score
fields set to zero, made one time by hand and committed under `templates/`.

Whether a score reaches the file without initials entry is not verified. Test
each game on the Pi. Signage says: game over, enter initials, then exit.

## The API flow

The API never calls the cabinet. The cabinet sits behind NAT with no inbound
path, so every exchange starts from the cabinet. The API stores nonces and
scores and nothing else about the cabinet. The session lives in the cabinet, so
logout is local and a power loss leaves nothing to clean up on the server.

| When | Call | How often |
| --- | --- | --- |
| Startup, and on demand from the service screen | `GET /health/ready` | one |
| The attract screen opens, or the nonce expires while it shows | `POST /api/cabinets/{id}/nonces` | one per ten minutes |
| The attract screen shows a live nonce | `GET /api/cabinets/{id}/nonces/{nonce}?wait=30`, a long poll | one per 30 seconds |
| The phone confirms | `POST /api/checkin/{nonce}`, from the site through the Worker | one per check-in, not seen by the cabinet |
| A run is in the outbox | `POST /api/scores` | one per run, with retry |
| Startup, after a run of a game uploads, attract opens with a copy older than ten minutes | `GET /api/games/{game}/leaderboard` | a few per hour |

**The long poll is the only poll.** The API holds the request for up to 30
seconds and answers the moment the phone binds the nonce. An empty answer means
"nothing yet, ask again". Check-in is instant on the phone side, and an idle
cabinet makes about 150 requests per hour. A plain poll every two seconds would
make 1800. Nothing polls while a player plays, while the postgame screen shows
or while the service screen is open. A 30 second wait sits under the 100 second
limit of a request through Cloudflare.

**No call at postgame.** The run goes into the outbox and the postgame frame is
ready at once. The rank comes from the local copy of the leaderboard plus the
runs still in the outbox. The drainer sends one `POST` per run, with a run id
the cabinet generated, so a retry never duplicates. On failure it waits one
second, then doubles up to one minute. It sleeps when the outbox is empty.

**Two servers, one token.** Today the API is one instance, reachable by a local
address and by `api.lanbunker.eu` through the tunnel. One day it is the cloud
name only. The configuration holds both URLs. The service screen shows both
with a health check and its latency, selects the active one, and edits the
local address digit by digit as an IP and a port. The selection and the edited
address are runtime state and live in the SQLite file next to the outbox. A
failed server is not a failover trigger: the outbox holds the runs, the status
page shows red, and a person switches. Automatic failover is deferred.

The phone path always goes through the cloud, because the site runs on
Cloudflare. If only the local network reaches the API, check-in stops even with
the cabinet on the local server. The scores still queue.

The site gets a `/checkin/{nonce}` page. The QR payload is that URL. A cabinet
is a device with its own bearer token, issued by an admin. The details belong
to the API design and are not fixed here.

## Screens

Three screens for the player and one hidden screen:

1. **Attract** (guest mode): the QR code and the idle leaderboard.
2. **Select**: the six games.
3. **Postgame**: score, personal best, rank, the leaderboard.
4. **Service**: hidden. Hold two buttons for three seconds.

A guest claim screen, where a guest binds a finished run to a profile with a
second QR code, is a second iteration. It needs its own API work.

The service screen is for the person who sets up the cabinet. Its pages are
selected with the stick:

- **Status**: version, build profile, SDL video driver and display mode, canvas
  and scale factor, orientation, the active server and its last health result,
  outbox depth, current session, joystick name, uptime.
- **Server**: the local and the cloud server, each with a health check and its
  latency, the active one marked. Select the active server. Edit the local
  address digit by digit, as an IP and a port.
- **Patterns**, the 240p test suite idea, for the CRT: a border grid to see
  overscan, a crosshatch for geometry, SMPTE color bars for color and level, a
  gray ramp, a sharpness pattern of one-pixel lines, and the safe-area
  rectangle at the configured overscan percent. Left and right change the
  overscan value live and show the number, so the value for the configuration
  is read off the tube.
- **Input**: the live state of the stick and every button.
- **Actions**: force logout, restart the process, reboot, shutdown. Each one
  asks for a second press.

The patterns are pure draw functions in the screen, selected by a field of the
view model. The overscan value that the tube shows goes into the configuration
by hand. The binary does not write its own configuration.

Pictures are textures from `sdl2_image`. An animation is a value that reads the
clock each frame, so a 50 Hz and a 60 Hz display run it at the same speed.

## Display

Composite is the display of the first party. Two broadcast monitors, a Sony PVM
and a JVC, composite input only. A broadcast monitor is the good case for
composite: it accepts 240p, it decodes NTSC and PAL, its comb filter keeps text
sharp, and an underscan button shows the full frame. RGB is deferred.

| Profile | Signal | Boot configuration |
| --- | --- | --- |
| `composite-ntsc` | 240p at 60 Hz | `sdtv_mode=16`, `sdtv_aspect=1`, `hdmi_ignore_hotplug=1` |
| `composite-pal` | 288p at 50 Hz | `sdtv_mode=18`, `sdtv_aspect=1`, `hdmi_ignore_hotplug=1` |
| `hdmi` | whatever the screen reports | nothing, for development on a flat screen |

`sdtv_mode` 16 and 18 are the progressive variants of NTSC and PAL in the legacy
firmware. `hdmi_ignore_hotplug=1` keeps the composite output active with an HDMI
cable attached. The games run near 60 Hz, so `composite-ntsc` is the default
even in Europe. On 50 Hz RetroArch runs a game slow or drops frames. The profile
sets `video_refresh_rate` for RetroArch to match the mode.

The screen derives everything from the mode that SDL reports and from
`CABD_ORIENTATION`:

- **Canvas.** The short side is 240 logical pixels. The long side is
  `round(240 * aspect)`: 320 on a 4:3 mode, 427 on a 16:9 mode. In TATE the
  two swap. SDL scales the canvas to the mode with an integer factor and
  letterboxes the rest. One design, one pixel font at scale 1, the same look on
  the tube and on a flat screen.
- **TATE.** A landscape signal on a tube turned by 90 degrees. The screen
  renders into a texture of the canvas size and copies it rotated. Each layout
  is written for a wide canvas or a tall canvas and reads its size. RetroArch
  gets `video_rotation` from the same deploy argument.
- **Overscan.** `CABD_OVERSCAN_PERCENT` shrinks the safe area. Read the value
  off the tube with the patterns page.
- **Composite-safe drawing.** Large text, thick shapes, low-saturation colors,
  no saturated red next to blue, no one-pixel horizontal lines outside the
  patterns page.

Five of the six games are vertical. Ms. Pac-Man, Galaga, Dig Dug, Mr. Do! and
DoDonPachi run in portrait, and most are 288 or 320 lines tall. Bubble Bobble
is horizontal. Each entry in `games.toml` carries `orientation`. When it differs
from the cabinet, the select screen shows a small badge. On a landscape tube a
vertical game sits pillarboxed at 240 lines. On a TATE tube it fills the height.

## Crates

Two crates. One holds the software, one draws it.

```
cabd-core     library: cabinet, view, hiscore, config, outbox, api, launcher,
              conductor                                      -> bunker-models
bunker-cabd   binary: main, the SDL2 screen, the input map   -> cabd-core
```

The split exists for two reasons. The binary is the only crate that links a C
library, so every test of the logic builds and runs on a machine with no SDL,
and a change in logic never relinks SDL. And the screen has one door into the
software: it imports `cabd_core::view` for the types, calls `cabd_core::start`
and sends through the methods of the `Handle` it gets back. Nothing else. That
is a convention, and a reviewer checks it.

```
crates/cabd-core/src/
  lib.rs
  cabinet.rs cabinet/  pure: the state machine, Event, Effect
  view.rs              pure: Frame, ViewModel, Input, ScreenConfig
  hiscore.rs hiscore/  pure: decode(game, bytes), the per-game tables
  config.rs            the environment and games.toml, read in one place
  outbox.rs            rusqlite
  api.rs               ureq
  launcher.rs          copy the template, spawn, wait, read the .hi file
  conductor.rs         the loop, the threads, start()
  devsock.rs           debug builds only: the control socket
crates/cabd-core/tests/
  cabinet_test.rs hiscore_test.rs outbox_test.rs api_test.rs launcher_test.rs
  fixtures/            golden .hi files with known scores
crates/bunker-cabd/
  DESIGN.md
  templates/           pristine .hi files, one per game
  dev/                 launcher-fake.sh, launcher-retroarch.sh, never ships
  src/main.rs          clap: run | screenshots | version
  src/screen.rs screen/  window, canvas, input map, one draw function per screen
```

`cabinet`, `view` and `hiscore` import std, serde and `bunker-models`, and
nothing else. They never touch a path, a socket or a clock. The clock is a
parameter. That is what makes the tests plain assertions. `outbox`, `api` and
`launcher` each own a `thiserror` enum and return plain types, so a `rusqlite`
or `ureq` type never leaves its module.

Threads: the main thread, the conductor, the outbox drainer and the check-in
poller. Each worker is a loop that does one blocking call and sends an `Event`.
The main thread never blocks: a send from the screen is non-blocking, and the
screen reads frames with `try_recv`. No lock is held across a channel receive.

The workspace rules apply: no `mod.rs`, no panic in `src/`, one `thiserror` type
per module that fails, `nutype` for a field with rules, dependencies in the root
`Cargo.toml`.

## Configuration

The repository is the source of truth. The deploy writes every file that the Pi
needs, and the Pi holds no hand-edited file. `config.rs` reads the environment
and `games.toml` in one place and hands each module the part it needs as plain
data. `cabinet` and the screen never read the environment.

One `Config` struct with `#[arg(long, env)]` and no default on a path. A missing
path fails at startup with a message that names the variable.

| Variable | Pi | macOS |
| --- | --- | --- |
| `CABD_API_LOCAL_URL` | `http://192.168.1.10:3000` | `http://127.0.0.1:3000` |
| `CABD_API_CLOUD_URL` | `https://api.lanbunker.eu` | the same |
| `CABD_API_TOKEN` | the cabinet token | a dev token |
| `CABD_GAMES` | `/home/pi/bunker/games.toml` | `dev/games.toml` |
| `CABD_LAUNCHER` | `runcommand.sh 0 _SYS_ arcade {rom}` | `dev/launcher-fake.sh {rom}` |
| `CABD_HISCORE_DIR` | `/home/pi/RetroPie/roms/arcade/fbneo` | `.dev/hiscore` |
| `CABD_TEMPLATE_DIR` | `/home/pi/bunker/templates` | `templates` |
| `CABD_OUTBOX` | `/home/pi/bunker/outbox.db` | `.dev/outbox.db` |
| `CABD_IDLE_SECONDS` | `180` | `180` |
| `CABD_ORIENTATION` | `landscape` or `tate` | `landscape` |
| `CABD_OVERSCAN_PERCENT` | read off the tube | `0` |
| `RUST_LOG` | `info`, or `debug` while you test | `debug` |

The display mode is not a variable. The screen reads it from SDL. On macOS the
window is 4:3 by default, and `--window 1280x720` previews a wide canvas.
`make cabd-dev orientation=tate` shows TATE as the tube receives it, turned
inside a landscape window. `make cabd-dev-tate` shows the same canvas upright
in a portrait window, with the development-only `--upright` flag.

`games.toml` lists each game: the ROM name, the title, the orientation, the
decoder id and the picture.

`CABD_DEV_SOCKET` exists in debug builds only. It is a Unix socket that takes
one command per line, such as `scenario postgame` or `checkin dave`. Each
command becomes `Event::Command` and goes through `cabinet` like every other
event. From the laptop, `ssh pi 'echo "scenario postgame" | nc -U /run/cabd.sock'`
jumps the cabinet to a screen. A release build has no socket.

## Logging

Logging is how the cabinet is debugged, on the Pi over ssh and on the Mac in a
terminal. The log must let a reader who did not watch the screen say what the
cabinet did and why. `tracing` writes one line per record to stderr. On the Pi
the autostart loop appends stderr to `/home/pi/bunker/cabd.log`, and
`make pi-log` follows it. `RUST_LOG` selects the level.

What is logged, by level:

- **info**, always on at the party:
  - Startup: version, build profile, every configuration value with the token
    redacted, SDL version, video driver, display mode, canvas size and scale
    factor, orientation, joystick name and button count.
  - Every state transition: the event, the screen before, the screen after, the
    effects. One line, with the session handle if a player is checked in.
  - The launcher: the full command line, the pid, the exit code, the duration,
    and the last lines of `/dev/shm/runcommand.log` when the exit code is not
    zero.
  - The hiscore file: path, size, modification time, the decoded score, or the
    decode error with a hex dump of the first 64 bytes.
  - The outbox: each enqueue with its row id, each drain attempt with the HTTP
    status and the latency, each retry with the delay.
  - The check-in long poll: each nonce issued, each answer that binds a player,
    and each failure with the status and the latency. An empty answer is silent.
  - The server selection: each change, and each health check with its result
    and latency.
  - The screen: window created or destroyed, with the mode, texture loads, and
    a frame time summary every ten seconds: average and worst frame.
- **debug**, on while you test on hardware:
  - Every raw SDL event that the input map sees and what it became.
  - Every poll, every tick, every frame that reaches the screen.
  - Every effect as the executor starts and finishes it, with the duration.
- **trace**: the byte content of each hiscore read and each API body.

Rules: a line names the thing it talks about with an id, such as the row id,
the pid or the ROM name. An error is logged one time, where it is handled, with
its chain of causes. The token, and nothing else, is redacted. A span wraps a
game session from check-in to logout, so every line inside carries the handle.

## Dev workflow on macOS

- `cargo run -p bunker-cabd -- run` opens an SDL window. The fake launcher
  stands in for RetroArch. The whole flow runs without a Pi.
- The real API runs with `make dev`. No fake API exists. The real one is fast
  and the tests use it too.
- `cargo run -p bunker-cabd -- screenshots out/` renders every screen and every
  pattern from a sample `ViewModel` into PNG files with the software renderer
  and `SDL_VIDEODRIVER=dummy`. This is how a reviewer, or Claude Code, sees the
  output.
- The real RetroArch on macOS with `lr-fbneo` validates the capture pipeline
  end to end. Play one game, note the score, commit the `.hi` as a fixture.
- Debug builds map the function keys to the same `DevCommand` values as the
  socket: jump to a screen, check in a fake player, expire the idle timer.

## Tests

- **cabinet and hiscore**: pure tests. An event sequence in, the frames and the
  effects out. The clock is a parameter, so the idle timeout is a plain
  assertion. Each hiscore fixture decodes to the known score, and a corrupt
  file gives an error, not a wrong score.
- **outbox and api**: the test starts the real `bunker-api` in-process with
  `bunker_api::server::build_router` on a random port and a temporary SQLite
  file, the same way the API tests do. The cabinet queues a score, the drainer
  posts it, the test reads it back from the API. A second test stops the API
  and asserts that the row stays in the outbox. This test needs tokio as a
  dev-dependency, because the API is async. The code under test is not.
- **launcher**: a shell script as the launcher writes a fixture and exits. The
  test asserts the template copy, the wait and the decoded outcome.
- **conductor**: a fake clock and a channel on each side. A frame comes out for
  each event that changes the screen, and nothing else.
- **screen**: no unit test for drawing. The `screenshots` command is the check.

A test that skips must be `#[ignore]` with a reason. `make checklist` runs
everything.

## Install on a fresh RetroPie 4.8 image

The whole install is one script over ssh, then one deploy. The script is
`deploy/cabinet/install.sh`. It is idempotent: a second run changes nothing.

1. Flash the RetroPie 4.8 image for the Pi 3. Boot with a keyboard one time,
   enable ssh in `raspi-config`, connect to the network.
2. Copy the ROMs into `~/RetroPie/roms/arcade/`. They are not in the repository.
3. From the Mac, `make pi-install`. It runs `install.sh` on the Pi:
   - Records `uname -m`, `/etc/os-release`, `ldd --version` and
     `sdl2-config --version` in the log, so the cross image can be checked
     against them.
   - Installs `libsdl2-ttf-2.0-0` and `libsdl2-image-2.0-0` with apt. The
     RetroPie build of `libsdl2` is already present.
   - Installs `lr-fbneo` through `RetroPie-Setup/retropie_packages.sh` when it
     is missing. The module places `hiscore.dat`.
   - Writes `fbneo-hiscores = "enabled"` into the core options, and the arcade
     `retroarch.cfg` fragment of the chosen profile: refresh rate, rotation,
     runahead off.
   - Appends the boot configuration fragment of the profile to
     `/boot/config.txt`, between two marker lines, so a second run replaces it.
   - Creates `/home/pi/bunker/` with `games.toml`, `templates/` and `.env`. The
     token comes from `deploy/cabinet/pi.env`, which is not committed.
   - Replaces the `emulationstation` line in `autostart.sh` with the restart
     loop for the binary.
4. `make pi` builds and deploys the binary. Reboot. The cabinet starts on the
   console.

The install touches five things outside `/home/pi/bunker/`: the core options,
the arcade `retroarch.cfg`, `/boot/config.txt`, `autostart.sh` and the apt
state. Each change sits between marker lines or is a package, so the script can
undo it.

## Build and deploy

The Pi 3 never compiles. The binary is built on macOS with `cross` in Docker and
copied over ssh.

- `deploy/cabinet/Dockerfile.cross` starts from `debian:buster`, adds the
  `armhf` architecture and installs `crossbuild-essential-armhf`,
  `libsdl2-dev:armhf`, `libsdl2-ttf-dev:armhf` and `libsdl2-image-dev:armhf`.
  The glibc in the image is 2.28, the same as the Pi, so the binary starts.
- `Cross.toml` at the root points `armv7-unknown-linux-gnueabihf` at that image.
- `make pi` builds with the `pi` profile, copies the binary and
  `/home/pi/bunker/` with rsync, kills the running process so the autostart
  loop starts the new one, and follows the log. One command is the whole loop.
- `make pi-release` does the same with the `release` profile, for the party.

The binary links SDL2 dynamically. The image provides the headers and the link
stub, the Pi provides the library that runs. SDL2 keeps its ABI stable across
2.0.x versions, so the two can differ. The `sdl2` crate must not ask for a
function newer than the Pi library. The startup log prints both versions.

The loop from a saved file to a running cabinet takes seconds:

- `[profile.pi]` inherits `dev` and sets `opt-level = 1`. Level 0 is slow on a
  Pi 3 at 60 fps. Level 1 keeps incremental builds and line tables.
  Dependencies build at level 3 one time and stay in the cache.
- `cross` mounts the repository and writes to
  `target/armv7-unknown-linux-gnueabihf/`. A second build is incremental.
- A change in `cabd-core` compiles that crate and relinks the binary. SDL and
  SQLite sit in the cache.
- rsync sends the changed blocks of the binary only.

## Known unknowns

Check each one on the real Pi before you build on it.

1. **The SDL version and driver of RetroPie 4.8**, and whether a destroyed
   window releases the display for RetroArch. EmulationStation does this with
   the same library, so the pattern works, but not yet with our binary.
2. **The `.hi` path** on the Pi. The code says a `fbneo/` folder next to the ROM.
3. **Score commit without initials**. No source confirms it. Test each game.
4. **A zero table in the template.** Each game must accept it and show it. A
   game that refuses gets a template with the lowest legal values instead.
5. **runcommand from our own process.** Does `runcommand.sh 0 …` behave the same
   when its parent is not EmulationStation. Log its exit code and
   `/dev/shm/runcommand.log`.
6. **The framebuffer size in `sdtv_mode=16`.** The screen expects a 4:3 mode
   with 240 lines. If the firmware reports 480 lines, SDL scales by two and the
   look is the same, but the startup log must say which.
7. **Legibility at 240p over composite.** Test a pixel font at scale 1 on the
   tube, over the cable you will use. The patterns page is the tool.
8. **Phone to API network path** in the venue. The most likely failure on the
   night.
9. **Pi 3 performance** with SDL2 at 60 fps and two worker threads. The frame
   time summary in the log is the measure.

## Deferred

- **Guest claim**: bind a finished guest run to a profile with a second QR code.
- **Automatic failover** between the local and the cloud server.
- **RGB output** through a DPI adapter, and per-game video modes with RetroArch
  SwitchRes. The fixed 240p mode already looks native on a CRT.
- **Live score capture** while the game runs. `READ_CORE_MEMORY` over UDP 55355
  fails on FBNeo arcade drivers, because they register no memory map.
  `READ_CORE_RAM` can work, with each `hiscore.dat` address rebased into the
  driver RAM area. Not verified.
- **Marquee display**: a small HTTP server that serves the `ViewModel` to a
  second thin client.
- **Multiple cabinets**: each runs its own binary, all report to one API.
- **Video and shaders**: SDL2 has no video playback and the 2D renderer has no
  shaders. Both need a different drawing path.

## Conventions

- The screen never holds a token and never talks to `bunker-api`.
- The screen imports `cabd_core::view`, calls `cabd_core::start` and talks
  through the `Handle` it returns. Nothing else from the library.
- The binary is the only crate that links SDL.
- `cabinet` sends view-ready data. No formatting and no logic in the screen.
- The binary never changes the video mode and never writes its own
  configuration. Runtime state, such as the outbox and the server selection,
  lives in its SQLite file.
- Every layout is written for the logical canvas and reads its size. No layout
  knows the display resolution.
- Keep slow work off the `GameEnded` path. The postgame frame must be ready
  before the window comes back.
- Every decision the cabinet takes leaves one log line that a reader can act on.
