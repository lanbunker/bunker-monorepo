//! One draw function per screen, in the terminal style of the site: black
//! ground, phosphor green accent, gray rules, a prompt line and a blinking
//! block cursor. Every position comes from the `Layout`, so the same code
//! serves a wide canvas and a tall one. The screen draws what the view model
//! says and decides nothing.

use cabd_core::view::{GameCard, Row, Screen, Status, Tone, ViewModel};
use sdl2::pixels::Color;
use sdl2::rect::Rect;
use sdl2::render::{Canvas, RenderTarget};

use super::ScreenError;
use super::canvas::{Layout, to_i32};
use super::text::{Align, Cache, Painter, Size};

// The palette of `web/src/styles/global.css`, by the same names.
const BG: Color = Color::RGB(0x05, 0x05, 0x05);
const PANEL: Color = Color::RGB(0x0a, 0x0a, 0x0a);
const INK: Color = Color::RGB(0xe4, 0xe4, 0xe4);
const BRIGHT: Color = Color::RGB(0xff, 0xff, 0xff);
const DIM: Color = Color::RGB(0x9c, 0x9c, 0x9c);
const MUTE: Color = Color::RGB(0x5e, 0x5e, 0x5e);
const BORDER: Color = Color::RGB(0x4a, 0x4a, 0x4a);
const FAINT: Color = Color::RGB(0x26, 0x26, 0x26);
const ACCENT: Color = Color::RGB(0x00, 0xff, 0x41);
const ACCENT_SOFT: Color = Color::RGB(0x35, 0xc9, 0x5d);
const WARN: Color = Color::RGB(0xff, 0xb0, 0x00);

/// The inner padding of the safe area. The overscan inset is on top of it.
const PAD: i32 = 8;
/// The padding inside a panel.
const PANEL_PAD: i32 = 6;
const GLYPH: i32 = 8;
const LINE: i32 = 12;
const ROW: i32 = 12;
const BIG_GLYPH: i32 = 16;
/// The QR panel takes this share of the width in landscape, so the scores
/// keep room for a handle and a seven-digit score on one row.
const QR_PANEL_PERCENT: u32 = 45;

type Pen<'a, 'd, T> = Painter<'a, 'd, T>;

pub(crate) fn draw_view<T: RenderTarget>(
    canvas: &mut Canvas<T>,
    cache: &mut Cache<'_, T::Context>,
    layout: &Layout,
    vm: &ViewModel,
    blink_on: bool,
) -> Result<(), ScreenError> {
    let mut p = Painter::new(canvas, cache);
    p.canvas.set_draw_color(BG);
    p.canvas.clear();

    let area = inner(layout.safe, PAD);
    let session = vm.session.as_deref();
    match &vm.screen {
        Screen::Attract {
            qr_payload,
            leaderboard,
        } => {
            let body = chrome(
                &mut p,
                area,
                session,
                &vm.command,
                blink_on,
                "press any button",
            )?;
            draw_attract(&mut p, layout, body, qr_payload.as_deref(), leaderboard)?;
        }
        Screen::Select { games, selected } => {
            let body = chrome(
                &mut p,
                area,
                session,
                &vm.command,
                blink_on,
                "a play   b back",
            )?;
            draw_select(&mut p, body, games, *selected)?;
        }
        Screen::Postgame {
            score_display,
            status,
            leaderboard,
        } => {
            let body = chrome(
                &mut p,
                area,
                session,
                &vm.command,
                blink_on,
                "a again   b menu",
            )?;
            draw_postgame(&mut p, body, score_display, status.as_ref(), leaderboard)?;
        }
    }
    Ok(())
}

/// The header, the prompt line and the footer that every screen shares.
/// Returns the body rectangle between them.
fn chrome<T: RenderTarget>(
    p: &mut Pen<'_, '_, T>,
    area: Rect,
    session: Option<&str>,
    command: &str,
    blink_on: bool,
    hint: &str,
) -> Result<Rect, ScreenError> {
    let top = area.top();
    p.text(
        Size::Small,
        "LANBUNKER",
        (area.left(), top),
        ACCENT,
        Align::Left,
    )?;
    match session {
        Some(handle) => p.text(
            Size::Small,
            handle,
            (area.right(), top),
            BRIGHT,
            Align::Right,
        )?,
        None => p.text(
            Size::Small,
            "guest",
            (area.right(), top),
            MUTE,
            Align::Right,
        )?,
    };
    let rule_y = top + GLYPH + 4;
    p.fill(Rect::new(area.left(), rule_y, area.width(), 1), BORDER)?;

    let prompt_y = rule_y + 6;
    let (w, _) = p.text(
        Size::Small,
        "bunker@cab:~$",
        (area.left(), prompt_y),
        DIM,
        Align::Left,
    )?;
    let command_x = area.left() + to_i32(w) + GLYPH;
    let command = fit(command, area.right() - command_x);
    p.text(
        Size::Small,
        &command,
        (command_x, prompt_y),
        ACCENT_SOFT,
        Align::Left,
    )?;

    let footer_y = area.bottom() - GLYPH;
    let (w, _) = p.text(Size::Small, ">", (area.left(), footer_y), MUTE, Align::Left)?;
    let hint_x = area.left() + to_i32(w) + GLYPH;
    let (hw, _) = p.text(Size::Small, hint, (hint_x, footer_y), DIM, Align::Left)?;
    if blink_on {
        p.fill(
            Rect::new(hint_x + to_i32(hw) + GLYPH, footer_y, 7, 8),
            ACCENT,
        )?;
    }

    let body_top = prompt_y + LINE + 4;
    let body_bottom = footer_y - 6;
    Ok(Rect::new(
        area.left(),
        body_top,
        area.width(),
        to_u32(body_bottom - body_top),
    ))
}

fn draw_attract<T: RenderTarget>(
    p: &mut Pen<'_, '_, T>,
    layout: &Layout,
    body: Rect,
    qr_payload: Option<&str>,
    leaderboard: &[Row],
) -> Result<(), ScreenError> {
    // The QR panel and the scores panel sit side by side on a wide canvas and
    // one above the other on a tall one.
    let gap = PAD;
    let (qr_panel, list_panel) = if layout.tate {
        let half = body.height().saturating_sub(to_u32(gap)) / 2;
        (
            Rect::new(body.left(), body.top(), body.width(), half),
            Rect::new(
                body.left(),
                body.top() + to_i32(half) + gap,
                body.width(),
                half,
            ),
        )
    } else {
        let qr_w = body.width() * QR_PANEL_PERCENT / 100;
        let list_w = body.width().saturating_sub(qr_w + to_u32(gap));
        (
            Rect::new(body.left(), body.top(), qr_w, body.height()),
            Rect::new(
                body.left() + to_i32(qr_w) + gap,
                body.top(),
                list_w,
                body.height(),
            ),
        )
    };

    panel(p, qr_panel)?;
    let qr_inner = inner(qr_panel, PANEL_PAD);
    let qr_area = Rect::new(
        qr_inner.left(),
        qr_inner.top(),
        qr_inner.width(),
        qr_inner.height().saturating_sub(to_u32(LINE)),
    );
    let caption_at = (qr_inner.center().x(), qr_inner.bottom() - GLYPH);
    let mid = (qr_area.center().x(), qr_area.center().y() - 4);
    match qr_payload {
        Some(payload) if p.qr(qr_area, payload)? => {
            p.text(
                Size::Small,
                "scan to check in",
                caption_at,
                DIM,
                Align::Center,
            )?;
        }
        Some(_) => {
            p.text(Size::Small, "qr error", mid, WARN, Align::Center)?;
        }
        None => {
            p.text(Size::Small, "offline", mid, MUTE, Align::Center)?;
            p.text(Size::Small, "guest mode", caption_at, DIM, Align::Center)?;
        }
    }

    panel(p, list_panel)?;
    let list = inner(list_panel, PANEL_PAD);
    p.text(
        Size::Small,
        "top scores",
        (list.left(), list.top()),
        DIM,
        Align::Left,
    )?;
    draw_rows(p, below_title(list), leaderboard)
}

fn draw_select<T: RenderTarget>(
    p: &mut Pen<'_, '_, T>,
    body: Rect,
    games: &[GameCard],
    selected: usize,
) -> Result<(), ScreenError> {
    panel(p, body)?;
    let list = inner(body, PANEL_PAD);
    p.text(
        Size::Small,
        "games",
        (list.left(), list.top()),
        DIM,
        Align::Left,
    )?;
    let mut y = list.top() + LINE + 2;
    let title_x = list.left() + 2 * GLYPH;
    for (i, game) in games.iter().enumerate() {
        let is_selected = i == selected;
        if is_selected {
            p.fill(
                Rect::new(list.left() - 2, y - 2, list.width() + 4, to_u32(ROW)),
                FAINT,
            )?;
            p.text(Size::Small, ">", (list.left(), y), ACCENT, Align::Left)?;
        }
        let color = if is_selected { ACCENT } else { INK };
        let title = fit(&game.title, list.right() - title_x);
        p.text(Size::Small, &title, (title_x, y), color, Align::Left)?;
        y += ROW;
    }
    if games.is_empty() {
        p.text(Size::Small, "no games", (title_x, y), MUTE, Align::Left)?;
    }
    Ok(())
}

fn draw_postgame<T: RenderTarget>(
    p: &mut Pen<'_, '_, T>,
    body: Rect,
    score_display: &str,
    status: Option<&Status>,
    leaderboard: &[Row],
) -> Result<(), ScreenError> {
    let score_h = LINE + BIG_GLYPH + 4 + LINE + PANEL_PAD * 2;
    let score_panel = Rect::new(body.left(), body.top(), body.width(), to_u32(score_h));
    panel(p, score_panel)?;
    let score = inner(score_panel, PANEL_PAD);
    let cx = score.center().x();
    let mut y = score.top();
    p.text(
        Size::Small,
        "game over",
        (score.left(), y),
        DIM,
        Align::Left,
    )?;
    p.text(Size::Small, "score", (score.right(), y), DIM, Align::Right)?;
    y += LINE;
    p.text(Size::Big, score_display, (cx, y), ACCENT, Align::Center)?;
    y += BIG_GLYPH + 4;
    if let Some(status) = status {
        let color = match status.tone {
            Tone::Ink => INK,
            Tone::Warn => WARN,
            Tone::Dim => DIM,
        };
        let text = fit(&status.text, to_i32(score.width()));
        p.text(Size::Small, &text, (cx, y), color, Align::Center)?;
    }

    let list_top = score_panel.bottom() + PAD;
    let list_panel = Rect::new(
        body.left(),
        list_top,
        body.width(),
        to_u32(body.bottom() - list_top),
    );
    panel(p, list_panel)?;
    let list = inner(list_panel, PANEL_PAD);
    p.text(
        Size::Small,
        "top scores",
        (list.left(), list.top()),
        DIM,
        Align::Left,
    )?;
    draw_rows(p, below_title(list), leaderboard)
}

/// A bordered panel with the panel ground, as on the site.
fn panel<T: RenderTarget>(p: &mut Pen<'_, '_, T>, rect: Rect) -> Result<(), ScreenError> {
    p.fill(rect, PANEL)?;
    p.outline(rect, BORDER)
}

/// Rows of rank, handle and score inside `at`. Rows that do not fit the height
/// are not drawn, and a handle that does not fit the width is cut, so the
/// columns never touch and nothing runs over the footer.
fn draw_rows<T: RenderTarget>(
    p: &mut Pen<'_, '_, T>,
    at: Rect,
    rows: &[Row],
) -> Result<(), ScreenError> {
    if rows.is_empty() {
        p.text(
            Size::Small,
            "no scores yet",
            (at.left(), at.top()),
            MUTE,
            Align::Left,
        )?;
        return Ok(());
    }
    let columns = glyphs(at.width());
    let max_rows = usize::try_from(at.height() / to_u32(ROW)).unwrap_or(0);
    let mut y = at.top();
    for row in rows.iter().take(max_rows) {
        let color = if row.highlight { ACCENT } else { INK };
        // Rank, a space, the handle, at least one space, the score.
        let room = columns.saturating_sub(row.score_display.len() + 4);
        let handle: String = row.handle.chars().take(room).collect();
        let label = format!("{:>2} {handle}", row.rank);
        p.text(Size::Small, &label, (at.left(), y), color, Align::Left)?;
        let score_color = if row.highlight { ACCENT } else { BRIGHT };
        p.text(
            Size::Small,
            &row.score_display,
            (at.right(), y),
            score_color,
            Align::Right,
        )?;
        y += ROW;
    }
    Ok(())
}

/// The rows area of a panel whose first line is a title.
fn below_title(list: Rect) -> Rect {
    let top = list.top() + LINE + 2;
    Rect::new(list.left(), top, list.width(), to_u32(list.bottom() - top))
}

/// Cuts a line to the glyphs that fit in `width` pixels.
fn fit(text: &str, width: i32) -> String {
    text.chars().take(glyphs(to_u32(width))).collect()
}

fn glyphs(width: u32) -> usize {
    usize::try_from(width / to_u32(GLYPH)).unwrap_or(0)
}

fn inner(rect: Rect, pad: i32) -> Rect {
    Rect::new(
        rect.left() + pad,
        rect.top() + pad,
        rect.width().saturating_sub(to_u32(2 * pad)).max(1),
        rect.height().saturating_sub(to_u32(2 * pad)).max(1),
    )
}

fn to_u32(v: i32) -> u32 {
    u32::try_from(v).unwrap_or(0)
}
