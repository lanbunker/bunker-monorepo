use nutype::nutype;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

pub const GLYPH_SIZE: usize = 5;

pub const GLYPH_CELLS: usize = GLYPH_SIZE * GLYPH_SIZE;

const HALF_COLUMNS: usize = 3;
const CENTER_COLUMN: usize = 2;
const MIN_LIT_CELLS: usize = 7;
const CORNERS: [usize; 4] = [0, 4, 20, 24];
const COLOR_COUNT: u32 = 12;
/// Salt for the color hash. The low bits of the grid hash cluster for similar
/// handles, so the color comes from its own hash.
const COLOR_SALT: &str = "/color";
const _: () = assert!(GLYPH_COLORS.len() == COLOR_COUNT as usize);

/// Player colors. Each one reads on the dark background at small and large sizes.
/// The order is part of the algorithm: the hash selects by index.
pub const GLYPH_COLORS: [GlyphColor; 12] = [
    GlyphColor::Amber,
    GlyphColor::Cyan,
    GlyphColor::Violet,
    GlyphColor::Coral,
    GlyphColor::Lime,
    GlyphColor::Magenta,
    GlyphColor::Ice,
    GlyphColor::Gold,
    GlyphColor::Teal,
    GlyphColor::Orange,
    GlyphColor::Sky,
    GlyphColor::Mint,
];

/// The row-major grid as one integer. Bit `i` is cell `i`, and bit 0 is the top
/// left cell. Only the low 25 bits can be set.
#[nutype(
    validate(less_or_equal = 33_554_431),
    default = 0,
    derive(
        Debug,
        Clone,
        Copy,
        PartialEq,
        Eq,
        Hash,
        Default,
        Serialize,
        Deserialize
    )
)]
pub struct GlyphBits(u32);

/// One of the twelve palette colors. It serializes as the hex string the web
/// uses, so a stored value and a rendered value never disagree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
pub enum GlyphColor {
    #[serde(rename = "#ffb000")]
    Amber,
    #[serde(rename = "#4fd1e0")]
    Cyan,
    #[serde(rename = "#b48cff")]
    Violet,
    #[serde(rename = "#ff6b57")]
    Coral,
    #[serde(rename = "#9dff57")]
    Lime,
    #[serde(rename = "#ff5cc8")]
    Magenta,
    #[serde(rename = "#cfe7ff")]
    Ice,
    #[serde(rename = "#ffd75c")]
    Gold,
    #[serde(rename = "#2dd4bf")]
    Teal,
    #[serde(rename = "#ff8c42")]
    Orange,
    #[serde(rename = "#7cc4ff")]
    Sky,
    #[serde(rename = "#7ef5c0")]
    Mint,
}

impl GlyphColor {
    pub const fn hex(self) -> &'static str {
        match self {
            Self::Amber => "#ffb000",
            Self::Cyan => "#4fd1e0",
            Self::Violet => "#b48cff",
            Self::Coral => "#ff6b57",
            Self::Lime => "#9dff57",
            Self::Magenta => "#ff5cc8",
            Self::Ice => "#cfe7ff",
            Self::Gold => "#ffd75c",
            Self::Teal => "#2dd4bf",
            Self::Orange => "#ff8c42",
            Self::Sky => "#7cc4ff",
            Self::Mint => "#7ef5c0",
        }
    }

    /// Parses the hex form. This is what a database column holds.
    pub fn from_hex(hex: &str) -> Option<Self> {
        GLYPH_COLORS.into_iter().find(|color| color.hex() == hex)
    }
}

/// What the players table stores: the grid and the color. Generated one time at
/// signup and then read, never recomputed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
pub struct Glyph {
    pub bits: GlyphBits,
    pub color: GlyphColor,
}

impl Glyph {
    pub fn cells(self) -> [bool; GLYPH_CELLS] {
        let bits = self.bits.into_inner();
        let mut cells = [false; GLYPH_CELLS];
        for (index, cell) in cells.iter_mut().enumerate() {
            *cell = (bits >> index) & 1 == 1;
        }
        cells
    }

    /// Five text rows such as `#.#.#`, the portable form for terminals and JSON.
    pub fn rows(self) -> [String; GLYPH_SIZE] {
        let cells = self.cells();
        let mut chunks = cells.chunks(GLYPH_SIZE);
        std::array::from_fn(|_| {
            chunks
                .next()
                .unwrap_or_default()
                .iter()
                .map(|lit| if *lit { '#' } else { '.' })
                .collect()
        })
    }
}

/// Builds the mark for a new account from a seed, normally the handle.
///
/// The algorithm is shared with the web renderer and must not change: the left
/// three columns come from hash bits and mirror to the right, the center column
/// always has a lit cell, and a mark with fewer than seven cells gets its corners
/// filled for weight.
pub fn generate_glyph(seed: &str) -> Glyph {
    let hash = fnv1a(seed);

    // `index % 32` is a no-op for 15 cells. It stays for parity with `glyph.ts`.
    let mut half = [false; GLYPH_SIZE * HALF_COLUMNS];
    for (index, bit) in half.iter_mut().enumerate() {
        *bit = (hash >> (index % 32)) & 1 == 1;
    }

    let center_lit = half
        .iter()
        .skip(CENTER_COLUMN)
        .step_by(HALF_COLUMNS)
        .any(|lit| *lit);
    if !center_lit && let Some(spine) = half.get_mut(CENTER_COLUMN + HALF_COLUMNS * 2) {
        *spine = true;
    }

    let mut cells = [false; GLYPH_CELLS];
    for (index, cell) in cells.iter_mut().enumerate() {
        let row = index / GLYPH_SIZE;
        let column = index % GLYPH_SIZE;
        let mirrored = if column > CENTER_COLUMN {
            GLYPH_SIZE - 1 - column
        } else {
            column
        };
        *cell = half
            .get(row * HALF_COLUMNS + mirrored)
            .copied()
            .unwrap_or(false);
    }

    let lit_count = cells.iter().filter(|lit| **lit).count();
    if lit_count < MIN_LIT_CELLS {
        for corner in CORNERS {
            if let Some(cell) = cells.get_mut(corner) {
                *cell = true;
            }
        }
    }

    let raw = cells.iter().enumerate().fold(
        0_u32,
        |bits, (index, lit)| if *lit { bits | (1 << index) } else { bits },
    );

    // 25 cells never exceed the 25 bit limit, so the validation cannot fail. The
    // default keeps the function total without a panic.
    let bits = GlyphBits::try_new(raw).unwrap_or_default();
    let color_hash = fnv1a(&format!("{seed}{COLOR_SALT}"));
    let color = GLYPH_COLORS
        .get(usize::try_from(color_hash % COLOR_COUNT).unwrap_or_default())
        .copied()
        .unwrap_or(GlyphColor::Amber);

    Glyph { bits, color }
}

/// FNV-1a over the UTF-16 code units of the lowercased seed, 32 bit. The web
/// renderer hashes `charCodeAt` values, so this reads code units and not bytes. The
/// two differ on characters outside the Basic Multilingual Plane, which a `Handle`
/// cannot hold.
fn fnv1a(seed: &str) -> u32 {
    seed.to_lowercase()
        .encode_utf16()
        .fold(2_166_136_261_u32, |hash, unit| {
            (hash ^ u32::from(unit)).wrapping_mul(16_777_619)
        })
}
