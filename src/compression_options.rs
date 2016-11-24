pub const HIGH_MAX_HASH_CHECKS: u16 = 1024;
pub const HIGH_LAZY_IF_LESS_THAN: u16 = 128;
#[allow(unused)]
pub const MAX_HASH_CHECKS: u16 = 32 * 1024;
pub const DEFAULT_MAX_HASH_CHECKS: u16 = 128;
pub const DEFAULT_LAZY_IF_LESS_THAN: u16 = 32;

/// Enum allowing some special options (not implemented yet!)
pub enum SpecialOptions {
    // Compress normally.
    Normal,
    // Force fixed huffman tables. (Unimplemented!)
    _ForceFixed,
    // Force stored (uncompressed) blocks only. (Unimplemented!)
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

#[doc(hidden)]
pub const _HUFFMAN_ONLY: CompressionOptions = CompressionOptions {
    max_hash_checks: 0,
    lazy_if_less_than: 0,
    special: SpecialOptions::Normal,
};

/// A struct describing the options for a compressor or compression function.
///
/// These values are not stable and still subject to change!
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
    /// Returns compression settings rouhgly corresponding to the High settings
    /// In zlib and miniz.
    pub fn high() -> CompressionOptions {
        CompressionOptions {
            max_hash_checks: HIGH_MAX_HASH_CHECKS,
            lazy_if_less_than: HIGH_LAZY_IF_LESS_THAN,
            special: SpecialOptions::Normal,
        }
    }
}

impl Default for CompressionOptions {
    /// Returns the options describing the default compression level.
    fn default() -> CompressionOptions {
        DEFAULT_OPTIONS
    }
}
