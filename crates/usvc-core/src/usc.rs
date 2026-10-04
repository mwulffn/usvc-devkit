//! The `.usc` game package: a 512-byte header, a preview image and the raw
//! binary that the game loader writes to flash at `0x6000`.

pub const HEADER_SIZE: usize = 512;
pub const PREVIEW_WIDTH: usize = 96;
pub const PREVIEW_HEIGHT: usize = 72;
/// The preview is padded to whole 512-byte sectors.
pub const PREVIEW_SIZE: usize = (PREVIEW_WIDTH * PREVIEW_HEIGHT).div_ceil(512) * 512;
const FIELD: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UscError {
    TooShort,
    BadMagic,
    BadLength,
}

impl std::fmt::Display for UscError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let msg = match self {
            UscError::TooShort => "file is too short to be a .usc package",
            UscError::BadMagic => "missing USVC signature",
            UscError::BadLength => "binary length in header exceeds the file",
        };
        f.write_str(msg)
    }
}

impl std::error::Error for UscError {}

#[derive(Debug, Clone)]
pub struct Usc {
    pub checksum: u32,
    pub short_title: String,
    pub title: [String; 4],
    pub description: [String; 4],
    pub authors: [String; 2],
    pub date: String,
    pub version: String,
    /// `PREVIEW_WIDTH` x `PREVIEW_HEIGHT` bytes in the console's 8-bit colour.
    pub preview: Vec<u8>,
    pub binary: Vec<u8>,
}

fn text(data: &[u8], offset: usize) -> String {
    let field = &data[offset..offset + FIELD];
    let end = field.iter().position(|&b| b == 0).unwrap_or(FIELD);
    String::from_utf8_lossy(&field[..end]).into_owned()
}

/// Sum of the little-endian words of the binary, padded to a word boundary.
pub fn checksum(binary: &[u8]) -> u32 {
    binary.chunks(4).fold(0u32, |sum, c| {
        let mut w = [0u8; 4];
        w[..c.len()].copy_from_slice(c);
        sum.wrapping_add(u32::from_le_bytes(w))
    })
}

impl Usc {
    pub fn parse(data: &[u8]) -> Result<Usc, UscError> {
        let body = HEADER_SIZE + PREVIEW_SIZE;
        if data.len() < body {
            return Err(UscError::TooShort);
        }
        if &data[0..4] != b"USVC" {
            return Err(UscError::BadMagic);
        }
        let word = |o: usize| u32::from_le_bytes([data[o], data[o + 1], data[o + 2], data[o + 3]]);
        let length = word(8) as usize;
        if body + length > data.len() {
            return Err(UscError::BadLength);
        }
        let lines = |base: usize, i: usize| text(data, base + i * FIELD);
        Ok(Usc {
            checksum: word(4),
            short_title: text(data, 32),
            title: std::array::from_fn(|i| lines(64, i)),
            description: std::array::from_fn(|i| lines(192, i)),
            authors: std::array::from_fn(|i| lines(320, i)),
            date: text(data, 384),
            version: text(data, 416),
            preview: data[HEADER_SIZE..HEADER_SIZE + PREVIEW_WIDTH * PREVIEW_HEIGHT].to_vec(),
            binary: data[body..body + length].to_vec(),
        })
    }
}
