pub const HIGH_MAX_HASH_CHECKS: u16 = 1024;
#[allow(unused)]
pub const MAX_HASH_CHECKS: u16 = 32 * 1024;
pub const DEFAULT_MAX_HASH_CHECKS: u16 = 128;

pub enum SpecialOptions {
    Normal,
    _ForceFixed,
    _ForceStored,
}

pub const DEFAULT_OPTIONS: CompressionOptions = CompressionOptions {
    max_hash_checks: DEFAULT_MAX_HASH_CHECKS,
    special: SpecialOptions::Normal,
};

// const RLE_ONLY: CompressionOptions {
// max_hash_checks: 1,
// window_size: 1,
// special: SpecialOptions::Normal,
// }
//

pub const _HUFFMAN_ONLY: CompressionOptions = CompressionOptions {
    max_hash_checks: 0,
    special: SpecialOptions::Normal,
};

pub struct CompressionOptions {
    pub max_hash_checks: u16,
    // pub _lazy_matching: bool,
    // pub _window_size: u16,
    // pub _no_lazy_len: u16,
    // pub _decent_match: u16,
    pub special: SpecialOptions,
}

impl CompressionOptions {
    pub fn high() -> CompressionOptions {
        CompressionOptions {
            max_hash_checks: HIGH_MAX_HASH_CHECKS,
            special: SpecialOptions::Normal,
        }
    }
}

impl Default for CompressionOptions {
    fn default() -> CompressionOptions {
        DEFAULT_OPTIONS
    }
}
