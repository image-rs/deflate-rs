use lz77::MatchingType;
use std::convert::From;

pub const HIGH_MAX_HASH_CHECKS: u16 = 768;
pub const HIGH_LAZY_IF_LESS_THAN: u16 = 128;
#[allow(unused)]
pub const MAX_HASH_CHECKS: u16 = 32 * 1024;
pub const DEFAULT_MAX_HASH_CHECKS: u16 = 128;
pub const DEFAULT_LAZY_IF_LESS_THAN: u16 = 32;

/// An enum describing the level of compression to be used by the encoder
///
/// Higher compression ratios will be slower.
#[derive(Clone, Copy, Debug)]
pub enum Compression {
    /// Fast minimal compression (CompressionOptions::fast()).
    Fast,
    /// Default level (CompressionOptions::default()).
    Default,
    /// Higher compression level (CompressionOptions::high()).
    Best,
}

/// Enum allowing some special options (not implemented yet)!
pub enum SpecialOptions {
    /// Compress normally.
    Normal,
    /// Force fixed huffman tables. (Unimplemented!).
    _ForceFixed,
    /// Force stored (uncompressed) blocks only. (Unimplemented!).
    _ForceStored,
}

pub const DEFAULT_OPTIONS: CompressionOptions = CompressionOptions {
    max_hash_checks: DEFAULT_MAX_HASH_CHECKS,
    lazy_if_less_than: DEFAULT_LAZY_IF_LESS_THAN,
    matching_type: MatchingType::Lazy,
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
    matching_type: MatchingType::Greedy,
    special: SpecialOptions::Normal,
};

/// A struct describing the options for a compressor or compression function.
///
/// These values are not stable and still subject to change!
pub struct CompressionOptions {
    /// The maximum number of checks to make in the hash table for matches.
    pub max_hash_checks: u16,
    // pub _window_size: u16,
    /// Only lazy match if we have a length less than this value.
    pub lazy_if_less_than: u16,
    // pub _decent_match: u16,
    /// Whether to use lazy or greedy matching.
    pub matching_type: MatchingType,
    /// Force fixed/stored (Not implemented yet).
    pub special: SpecialOptions,
}

impl CompressionOptions {
    /// Returns compression settings rouhgly corresponding to the High(9) setting in miniz.
    pub fn high() -> CompressionOptions {
        CompressionOptions {
            max_hash_checks: HIGH_MAX_HASH_CHECKS,
            lazy_if_less_than: HIGH_LAZY_IF_LESS_THAN,
            matching_type: MatchingType::Lazy,
            special: SpecialOptions::Normal,
        }
    }

    /// Returns compression a fast set of compression settings settings
    ///
    /// Ideally this should roughly correspond to the Fast(1) setting in miniz.
    /// However, that setting makes miniz use a somewhat different algorhithm,
    /// so currently hte fast level in this library is slower and better compressing
    /// than the corresponding level in miniz.
    pub fn fast() -> CompressionOptions {
        CompressionOptions {
            max_hash_checks: 1,
            lazy_if_less_than: 0,
            matching_type: MatchingType::Greedy,
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

impl From<Compression> for CompressionOptions {
    fn from(compression: Compression) -> CompressionOptions {
        match compression {
            Compression::Fast => CompressionOptions::fast(),
            Compression::Default => CompressionOptions::default(),
            Compression::Best => CompressionOptions::high(),
        }
    }
}
