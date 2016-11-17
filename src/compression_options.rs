pub const HIGH_MAX_HASH_CHECKS: u16 = 1024;
#[allow(unused)]
pub const MAX_HASH_CHECKS: u16 = 32 * 1024;
pub const DEFAULT_MAX_HASH_CHECKS: u16 = 128;
pub const DEFAULT_LAZY_IF_LESS_THAN: u16 = 32;

pub enum SpecialOptions {
    Normal,
    _ForceFixed,
    _ForceStored,
}

pub const DEFAULT_OPTIONS: CompressionOptions = CompressionOptions {
    max_hash_checks: DEFAULT_MAX_HASH_CHECKS,
    lazy_if_less_than: DEFAULT_LAZY_IF_LESS_THAN,
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
    lazy_if_less_than: 0,
    special: SpecialOptions::Normal,
};

pub struct CompressionOptions {
    // The maximum number of checks to make in the hash table for matches
    pub max_hash_checks: u16,
    // pub _lazy_matching: bool,
    // pub _window_size: u16,
    // Only lazy match if we have a length less than this value
    pub lazy_if_less_than: u16,
    // pub _decent_match: u16,
    // Force fixed/stored (Not implemented yet)
    pub special: SpecialOptions,
}

impl CompressionOptions {
    pub fn high() -> CompressionOptions {
        CompressionOptions {
            max_hash_checks: HIGH_MAX_HASH_CHECKS,
            lazy_if_less_than: DEFAULT_LAZY_IF_LESS_THAN,
            special: SpecialOptions::Normal,
        }
    }
}

impl Default for CompressionOptions {
    fn default() -> CompressionOptions {
        DEFAULT_OPTIONS
    }
}
