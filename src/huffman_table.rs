use std::fmt;
use bit_reverse::reverse_bits;

#[derive(Debug)]
pub enum HuffmanError {
    EmptyLengthTable,
    CodeTooLong,
    _TooManyOfLength,
}

// The number of length codes in the huffman table
pub const NUM_LENGTH_CODES: usize = 29;

// The number of distance codes in the distance huffman table
// NOTE: two mode codes are actually used when constructing codes
pub const NUM_DISTANCE_CODES: usize = 30;

// Combined number of literal and length codes
// NOTE: two mode codes are actually used when constructing codes
pub const NUM_LITERALS_AND_LENGTHS: usize = 286;


// The maximum length of a huffman code
pub const MAX_CODE_LENGTH: usize = 15;

// The minimun and maximum lengths for a match according to the DEFLATE specification
pub const MIN_MATCH: u16 = 3;
pub const MAX_MATCH: u16 = 258;

pub const MIN_DISTANCE: u16 = 1;
pub const MAX_DISTANCE: u16 = 32768;


// The position in the literal/length table of the end of block symbol
pub const END_OF_BLOCK_POSITION: usize = 256;

// Bit lengths for literal and length codes in the fixed huffman table
// The huffman codes are generated from this and the distance bit length table
#[allow(unused)]
pub static FIXED_CODE_LENGTHS: [u8; NUM_LITERALS_AND_LENGTHS + 2] =
    [8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8,
     8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8,
     8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8,
     8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8,
     8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9,
     9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9,
     9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9,
     9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9,
     9, 9, 9, 9, 9, 9, 9, 9, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7,
     7, 8, 8, 8, 8, 8, 8, 8, 8];



// The number of extra bits for the length codes
static LENGTH_EXTRA_BITS_LENGTH: [u8; NUM_LENGTH_CODES] =
    [0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0];

// Table used to get a code from a length value (see get_distance_code_and_extra_bits)
static LENGTH_CODE: [u8; 256] =
    [0, 1, 2, 3, 4, 5, 6, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 12, 12, 13, 13, 13, 13, 14, 14,
     14, 14, 15, 15, 15, 15, 16, 16, 16, 16, 16, 16, 16, 16, 17, 17, 17, 17, 17, 17, 17, 17, 18,
     18, 18, 18, 18, 18, 18, 18, 19, 19, 19, 19, 19, 19, 19, 19, 20, 20, 20, 20, 20, 20, 20, 20,
     20, 20, 20, 20, 20, 20, 20, 20, 21, 21, 21, 21, 21, 21, 21, 21, 21, 21, 21, 21, 21, 21, 21,
     21, 22, 22, 22, 22, 22, 22, 22, 22, 22, 22, 22, 22, 22, 22, 22, 22, 23, 23, 23, 23, 23, 23,
     23, 23, 23, 23, 23, 23, 23, 23, 23, 23, 24, 24, 24, 24, 24, 24, 24, 24, 24, 24, 24, 24, 24,
     24, 24, 24, 24, 24, 24, 24, 24, 24, 24, 24, 24, 24, 24, 24, 24, 24, 24, 24, 25, 25, 25, 25,
     25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25,
     25, 25, 25, 25, 25, 26, 26, 26, 26, 26, 26, 26, 26, 26, 26, 26, 26, 26, 26, 26, 26, 26, 26,
     26, 26, 26, 26, 26, 26, 26, 26, 26, 26, 26, 26, 26, 26, 27, 27, 27, 27, 27, 27, 27, 27, 27,
     27, 27, 27, 27, 27, 27, 27, 27, 27, 27, 27, 27, 27, 27, 27, 27, 27, 27, 27, 27, 27, 27, 28];

// Base values to calculate the value of the bits in length codes
static BASE_LENGTH: [u8; NUM_LENGTH_CODES] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 10, 12, 14, 16, 20, 24,
                                              28, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192,
                                              224, 255]; // 258 - MIN_MATCh

// What number in the literal/length table the lengths start at
const LENGTH_BITS_START: u16 = 257;

// Lengths for the distance codes in the pre-defined/fixed huffman table
// (All distance codes are 5 bits long)
pub static FIXED_CODE_LENGTHS_DISTANCE: [u8; NUM_DISTANCE_CODES + 2] = [5; NUM_DISTANCE_CODES + 2];

static DISTANCE_CODES: [u8; 512] =
    [0, 1, 2, 3, 4, 4, 5, 5, 6, 6, 6, 6, 7, 7, 7, 7, 8, 8, 8, 8, 8, 8, 8, 8, 9, 9, 9, 9, 9, 9, 9,
     9, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 11, 11, 11, 11, 11, 11,
     11, 11, 11, 11, 11, 11, 11, 11, 11, 11, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12,
     12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 13, 13, 13, 13,
     13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13,
     13, 13, 13, 13, 13, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14,
     14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14,
     14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14,
     15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15,
     15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15,
     15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 0, 0, 16, 17, 18, 18,
     19, 19, 20, 20, 20, 20, 21, 21, 21, 21, 22, 22, 22, 22, 22, 22, 22, 22, 23, 23, 23, 23, 23,
     23, 23, 23, 24, 24, 24, 24, 24, 24, 24, 24, 24, 24, 24, 24, 24, 24, 24, 24, 25, 25, 25, 25,
     25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 26, 26, 26, 26, 26, 26, 26, 26, 26, 26, 26,
     26, 26, 26, 26, 26, 26, 26, 26, 26, 26, 26, 26, 26, 26, 26, 26, 26, 26, 26, 26, 26, 27, 27,
     27, 27, 27, 27, 27, 27, 27, 27, 27, 27, 27, 27, 27, 27, 27, 27, 27, 27, 27, 27, 27, 27, 27,
     27, 27, 27, 27, 27, 27, 27, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28,
     28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28,
     28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28,
     28, 28, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29,
     29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29,
     29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29];

// Number of extra bits following the distance codes
static DISTANCE_EXTRA_BITS: [u8; NUM_DISTANCE_CODES] = [0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5,
                                                        6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11,
                                                        12, 12, 13, 13];

static DISTANCE_BASE: [u16; NUM_DISTANCE_CODES] =
    [0, 1, 2, 3, 4, 6, 8, 12, 16, 24, 32, 48, 64, 96, 128, 192, 256, 384, 512, 768, 1024, 1536,
     2048, 3072, 4096, 6144, 8192, 12288, 16384, 24576];

/// A struct representing the data needed to generate the bit codes for
/// a given value and huffman table.
#[derive(Copy, Clone)]
struct ExtraBits {
    // The position of the length in the huffman table.
    pub code_number: u16,
    // Number of extra bits following the code.
    pub num_bits: u8,
    // The value of the extra bits, which together with the length/distance code
    // allow us to calculate the exact length/distance.
    pub value: u16,
}

/// Get the length code that corresponds to the length value
pub fn get_length_code(length: u16) -> Option<usize> {
    if let Some(c) = Some(usize::from(LENGTH_CODE[(length - MIN_MATCH) as usize])) {
        Some(c + LENGTH_BITS_START as usize)
    } else {
        None
    }
}

/// Get the code for the huffman table and the extra bits for the requested length.
/// returns None if length < 3 or length > 258.
fn get_length_code_and_extra_bits(length: u16) -> Option<ExtraBits> {
    if length < MIN_MATCH || length > MAX_MATCH {
        // Invalid length!;
        return None;
    }

    // The minimun match length is 3, but length code table starts at 0,
    // so we need to subtract 3 to get the correct code.
    let n = LENGTH_CODE[(length - MIN_MATCH) as usize];

    // We can then get the base length from the base length table,
    // which we use to calculate the value of the extra bits.
    let base = u16::from(BASE_LENGTH[n as usize]);
    let num_bits = LENGTH_EXTRA_BITS_LENGTH[n as usize];
    Some(ExtraBits {
        code_number: u16::from(n) + LENGTH_BITS_START,
        num_bits: num_bits,
        value: length - base - MIN_MATCH,
    })

}

/// Get the spot in the huffman table for distances `distance` corresponds to
/// Returns none if the distance is 0, or above 32768
pub fn get_distance_code(distance: u16) -> Option<u8> {
    let distance = distance as usize;
    match distance {
        // Since the array starts at 0, we need to subtract 1 to get the correct code number.
        1...256 => Some(DISTANCE_CODES[distance - 1]),
        // Due to the distrubution of the distance codes above 256, we can get away with only
        // using the top bits to determine the code, rather than having a 32k long table of
        // distance codes.
        257...32768 => Some(DISTANCE_CODES[256 + ((distance - 1) >> 7)]),
        _ => None,
    }
}


fn get_distance_code_and_extra_bits(distance: u16) -> Option<ExtraBits> {
    if let Some(distance_code) = get_distance_code(distance) {
        let extra = DISTANCE_EXTRA_BITS[distance_code as usize];
        // FIXME: We should add 1 to the values in distance_base to avoid having to add one here
        let base = DISTANCE_BASE[distance_code as usize] + 1;
        Some(ExtraBits {
            code_number: distance_code.into(),
            num_bits: extra,
            value: distance - base,
        })
    } else {
        None
    }
}

#[derive(Copy, Clone, Default)]
pub struct HuffmanCode {
    pub code: u16,
    pub length: u8,
}

impl fmt::Debug for HuffmanCode {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f,
               "HuffmanCode {{ code: {:b}, length: {}}}",
               self.code,
               self.length)
    }
}

impl HuffmanCode {
    /// Create a new huffman code struct, reversing the bits in the code
    /// Returns None if the code is longer than 15 bits (the maximum allowed by the DEFLATE spec)
    fn from_reversed_bits(code: u16, length: u8) -> Option<HuffmanCode> {
        if length <= 15 {
            Some(HuffmanCode {
                code: reverse_bits(code, length),
                length: length,
            })
        } else {
            None
        }
    }
}

#[cfg(test)]
pub struct LengthAndDistanceBits {
    pub length_code: HuffmanCode,
    pub length_extra_bits: HuffmanCode,
    pub distance_code: HuffmanCode,
    pub distance_extra_bits: HuffmanCode,
}

/// Counts the number of values of each length.
/// Returns a tuple containing the longest length value in the table, it's position,
/// and a vector of lengths.
/// Returns an error if `table` is empty, or if any of the lenghts exceed 15.
fn build_length_count_table(table: &[u8]) -> Result<(usize, usize, Vec<u16>), HuffmanError> {
    // TODO: Validate the length table properly
    //
    let max_length = match table.iter().max() {
        Some(l) => (*l).into(),
        None => return Err(HuffmanError::EmptyLengthTable),
    };

    // Check if the longest code length is longer than allowed
    if max_length > MAX_CODE_LENGTH {
        return Err(HuffmanError::CodeTooLong);
    }

    let mut max_length_pos = 0;

    let mut len_counts = vec![0u16; max_length + 1];
    for (n, &length) in table.iter().enumerate() {
        // TODO: Make sure we don't have more of one length than we can make
        // codes for
        len_counts[usize::from(length)] += 1;
        if length > 0 {
            max_length_pos = n;
        }
    }
    Ok((max_length, max_length_pos, len_counts))
}

pub fn create_codes(length_table: &[u8]) -> Result<Vec<HuffmanCode>, HuffmanError> {
    let mut codes = vec![HuffmanCode::default(); length_table.len()];
    create_codes_in_place(codes.as_mut_slice(), length_table)?;
    Ok(codes)
}

/// Generats a vector of huffman codes given a table of bit lengths
/// Returns an error if any of the lengths are > 15
pub fn create_codes_in_place(code_table: &mut [HuffmanCode],
                             length_table: &[u8])
                             -> Result<(), HuffmanError> {


    let (max_length, max_length_pos, lengths) = build_length_count_table(length_table)?;

    let mut code = 0u16;
    let mut next_code = vec![0u16];

    for bits in 1..max_length + 1 {
        code = (code + lengths[bits - 1]) << 1;
        next_code.push(code.into());
    }

    for n in 0..max_length_pos + 1 {
        let length = usize::from(length_table[n]);
        if length != 0 {
            // The algorithm generates the code in the reverse bit order, so we need to reverse them
            // to get the correct codes.
            code_table[n] = HuffmanCode::from_reversed_bits(next_code[length], length as u8)
                .ok_or(HuffmanError::CodeTooLong)?;
            // We use wrapping here as we would otherwise overflow on the last code
            // This should be okay as we exit the loop after this so the value is ignored
            next_code[length] = next_code[length].wrapping_add(1);
        }
    }
    Ok(())
}

/// A structure containing the tables of huffman codes for lengths, literals and distances
pub struct HuffmanTable {
    // Literal, end of block and length codes
    codes: [HuffmanCode; 288],
    // Distance codes
    distance_codes: [HuffmanCode; 32],
}

impl HuffmanTable {
    pub fn empty() -> HuffmanTable {
        HuffmanTable {
            codes: [HuffmanCode {
                code: 0,
                length: 0,
            }; 288],
            distance_codes: [HuffmanCode {
                code: 0,
                length: 0,
            }; 32],
        }
    }

    #[cfg(test)]
    pub fn from_length_tables(literals_and_lengths: &[u8],
                              distances: &[u8])
                              -> Result<HuffmanTable, HuffmanError> {
        let mut table = HuffmanTable {
            codes: [HuffmanCode::default(); 288],
            distance_codes: [HuffmanCode::default(); 32],
        };

        create_codes_in_place(table.codes.as_mut(), literals_and_lengths)?;
        create_codes_in_place(table.distance_codes.as_mut(), distances)?;
        Ok(table)
    }

    /// Update the `HuffmanTable` from the provided length tables.
    /// Returns Err if the tables have lengths > 50, the
    /// tables are too short, or are otherwise not formed correctly.
    pub fn update_from_length_tables(&mut self,
                                     literals_and_lengths: &[u8],
                                     distances: &[u8])
                                     -> Result<(), HuffmanError> {
        create_codes_in_place(self.codes.as_mut(), literals_and_lengths)?;
        create_codes_in_place(self.distance_codes.as_mut(), distances)
    }

    /// Create a HuffmanTable using the fixed tables specified in the DEFLATE format specification.
    #[cfg(test)]
    pub fn fixed_table() -> HuffmanTable {
        // This should be safe to unwrap, if it were to panic the code is wrong,
        // tests should catch it.
        HuffmanTable::from_length_tables(&FIXED_CODE_LENGTHS, &FIXED_CODE_LENGTHS_DISTANCE)
            .expect("Error: Failed to build table for fixed huffman codes, this indicates an \
                     error somewhere in the code.")
    }

    /// Get the huffman code from the corresponding literal value
    pub fn get_literal(&self, value: u8) -> HuffmanCode {
        self.codes[usize::from(value)]
    }

    /// Get the huffman code for the end of block value
    pub fn get_end_of_block(&self) -> HuffmanCode {
        self.codes[END_OF_BLOCK_POSITION]
    }

    /// Get the huffman code and extra bits for the specified length
    ///
    /// returns None if the length is larger than MIN_MATCH or smaller than MAX_MATCH
    pub fn get_length_huffman(&self, length: u16) -> Option<((HuffmanCode, HuffmanCode))> {
        if length < MIN_MATCH || length > MAX_MATCH {
            return None;
        }

        let length_data = match get_length_code_and_extra_bits(length) {
            Some(t) => t,
            None => return None,
        };

        let length_huffman_code = self.codes[length_data.code_number as usize];
        Some((length_huffman_code,
              HuffmanCode {
            code: length_data.value,
            length: length_data.num_bits,
        }))
    }

    /// Get the huffman code and extra bits for the specified distance
    ///
    /// Returns None if distance is 0 or above 32768
    pub fn get_distance_huffman(&self, distance: u16) -> Option<((HuffmanCode, HuffmanCode))> {
        if distance < MIN_DISTANCE || distance > MAX_DISTANCE {
            return None;
        }

        let distance_data = match get_distance_code_and_extra_bits(distance) {
            Some(t) => t,
            None => return None,
        };

        let distance_huffman_code = self.distance_codes[distance_data.code_number as usize];

        Some((distance_huffman_code,
              HuffmanCode {
            code: distance_data.value,
            length: distance_data.num_bits,
        }))
    }

    #[cfg(test)]
    pub fn get_length_distance_code(&self,
                                    length: u16,
                                    distance: u16)
                                    -> Option<(LengthAndDistanceBits)> {
        let l_codes = self.get_length_huffman(length).unwrap();
        let d_codes = self.get_distance_huffman(distance).unwrap();
        Some(LengthAndDistanceBits {
            length_code: l_codes.0,
            length_extra_bits: l_codes.1,
            distance_code: d_codes.0,
            distance_extra_bits: d_codes.1,
        })
    }
}

mod test {
    // There seems to be a bug with unused importwarnings here, so we ignore them for now
    #[allow(unused_imports)]
    use super::*;
    #[allow(unused_imports)]
    use super::{get_length_code_and_extra_bits, get_distance_code_and_extra_bits,
                build_length_count_table};
    #[test]
    fn test_get_length_code() {
        let extra_bits = get_length_code_and_extra_bits(4).unwrap();
        assert_eq!(extra_bits.code_number, 258);
        assert_eq!(extra_bits.num_bits, 0);
        assert_eq!(extra_bits.value, 0);

        let extra_bits = get_length_code_and_extra_bits(165).unwrap();
        assert_eq!(extra_bits.code_number, 282);
        assert_eq!(extra_bits.num_bits, 5);
        assert_eq!(extra_bits.value, 2);

        let extra_bits = get_length_code_and_extra_bits(257).unwrap();
        assert_eq!(extra_bits.code_number, 284);
        assert_eq!(extra_bits.num_bits, 5);
        assert_eq!(extra_bits.value, 30);

        let extra_bits = get_length_code_and_extra_bits(258).unwrap();
        assert_eq!(extra_bits.code_number, 285);
        assert_eq!(extra_bits.num_bits, 0);
    }

    #[test]
    fn test_distance_code() {
        assert_eq!(get_distance_code(1).unwrap(), 0);
        assert_eq!(get_distance_code(0), None);
        assert_eq!(get_distance_code(50000), None);
        assert_eq!(get_distance_code(6146).unwrap(), 25);
        assert_eq!(get_distance_code(256).unwrap(), 15);
    }

    #[test]
    fn test_distance_extra_bits() {
        let extra = get_distance_code_and_extra_bits(527).unwrap();
        assert_eq!(extra.value, 0b1110);
        assert_eq!(extra.code_number, 18);
        assert_eq!(extra.num_bits, 8);
        let extra = get_distance_code_and_extra_bits(256).unwrap();
        assert_eq!(extra.code_number, 15);
        assert_eq!(extra.num_bits, 6);
    }

    #[test]
    fn test_length_table_fixed() {
        let _ = build_length_count_table(&FIXED_CODE_LENGTHS).unwrap();
    }

    #[test]
    fn test_length_table_max_length() {
        let table = [16u8; 288];
        let test = build_length_count_table(&table);
        match test {
            Err(HuffmanError::CodeTooLong) => (),
            _ => panic!("Didn't fail with invalid length!"),
        };
    }

    #[test]
    fn test_empty_table() {
        let table = [];
        let test = build_length_count_table(&table);
        match test {
            Err(HuffmanError::EmptyLengthTable) => (),
            _ => panic!("Empty length table didn't fail!'"),
        }
    }

    #[test]
    fn make_table_fixed() {
        let table = HuffmanTable::fixed_table();
        assert_eq!(table.codes[0].code, 0b00001100);
        assert_eq!(table.codes[143].code, 0b11111101);
        assert_eq!(table.codes[144].code, 0b000010011);
        assert_eq!(table.codes[255].code, 0b111111111);
        assert_eq!(table.codes[256].code, 0b0000000);
        assert_eq!(table.codes[279].code, 0b1110100);
        assert_eq!(table.codes[280].code, 0b00000011);
        assert_eq!(table.codes[287].code, 0b11100011);

        assert_eq!(table.distance_codes[0].code, 0);
        assert_eq!(table.distance_codes[5].code, 20);

        let ld = table.get_length_distance_code(4, 5).unwrap();

        assert_eq!(ld.length_code.code, 0b00100000);
        assert_eq!(ld.distance_code.code, 0b00100);
        assert_eq!(ld.distance_extra_bits.length, 1);
        assert_eq!(ld.distance_extra_bits.code, 0);
    }
}
