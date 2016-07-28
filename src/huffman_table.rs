use std::fmt;

#[derive(Debug)]
pub enum HuffmanError {
    EmptyLengthTable,
    CodeTooLong,
    _TooManyOfLength,
    ZeroLengthCode,
}

// The number of length codes in the huffman table
pub const NUM_LENGTH_CODES: usize = 29;

// The number of distance codes in the distance huffman table
pub const NUM_DISTANCE_CODES: usize = 30;

// The minimun and maximum lengths for a match according to the DEFLATE specification
pub const MIN_MATCH: u16 = 3;
pub const MAX_MATCH: u16 = 258;

pub const NUM_LITERALS_AND_LENGTHS: usize = 288;

// Bit lengths for literal and length codes in the fixed huffman table
// The huffman codes are generated from this and the distance bit length table
pub static FIXED_CODE_LENGTHS: [u8; NUM_LITERALS_AND_LENGTHS] =
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
pub static LENGTH_EXTRA_BITS_LENGTH: [u8; NUM_LENGTH_CODES] =
    [0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0];

// Table used to get a code from a length value (see get_distance_code_and_extra_bits)
pub static LENGTH_CODE: [u16; 256] =
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
pub static BASE_LENGTH: [u16; NUM_LENGTH_CODES] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 10, 12, 14, 16, 20,
                                                   24, 28, 32, 40, 48, 56, 64, 80, 96, 112, 128,
                                                   160, 192, 224, 0];

// What number in the literal/length table the lengths start at
const LENGTH_BITS_START: u16 = 257;

// Lengths for the distance codes in the pre-defined/fixed huffman table
// (All distance codes are 5 bits long)
pub static FIXED_CODE_LENGTHS_DISTANCE: [u8; NUM_DISTANCE_CODES] = [5; NUM_DISTANCE_CODES];

pub static DISTANCE_CODES: [u8; 512] =
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
pub static DISTANCE_EXTRA_BITS: [u8; NUM_DISTANCE_CODES] = [0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5,
                                                            5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11,
                                                            11, 12, 12, 13, 13];

pub static DISTANCE_BASE: [u16; NUM_DISTANCE_CODES] =
    [0, 1, 2, 3, 4, 6, 8, 12, 16, 24, 32, 48, 64, 96, 128, 192, 256, 384, 512, 768, 1024, 1536,
     2048, 3072, 4096, 6144, 8192, 12288, 16384, 24576];

/// Reverse the first `length bits of the code `code`.
/// Returns None if `length` > 15
fn reverse_bits(code: u16, length: u8) -> Option<u16> {
    if length > 15 {
        return None;
    }
    // This is basically ported from zlib, it's not the fastest
    // or the most idiomatic way of implementing this.
    // TODO: FIx link
    // https://github.com/madler/zlib/blob/master/trees.c#L1154
    let mut ret = 0u16;
    let mut length = length;
    let mut code = code;
    while length > 0 {
        ret |= code & 1;
        code >>= 1;
        ret <<= 1;
        length -= 1;
    }
    Some(ret >> 1)
}

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

/// Get the code for the huffman table and the extra bits for the requested length.
/// returns None if length < 3 or length > 258.
fn get_length_code_and_extra_bits(length: u16) -> Option<ExtraBits> {
    if length < MIN_MATCH || length > MAX_MATCH {
        // Invalid length!;
        return None;
    }

    // The minimun match length is 3, but length code table starts at 0,
    // so we need to subtract 3 to get the correct code.
    let code = LENGTH_CODE[(length - MIN_MATCH) as usize];
    // We can then get the base length from the base length table,
    // which we use to calculate the value of the extra bits.
    let base = BASE_LENGTH[code as usize];
    let num_bits = LENGTH_EXTRA_BITS_LENGTH[code as usize];
    Some(ExtraBits {
        code_number: code + LENGTH_BITS_START,
        num_bits: num_bits,
        value: length - base - MIN_MATCH,
    })
}

/// Get the spot in the huffman table for distances `distance` corresponds to
/// Returns none if the distance is 0, or above 32768
fn get_distance_code(distance: u16) -> Option<u8> {
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

// fn get_extra_bits_for_distance_code(distance_code: u16) -> ExtraBits {
// const DISTANCES_START: u16 = 257;
// const DISTANCES_END: u16 = 285;
// if distance_code < DISTANCES_START || distance_code > DISTANCES_END {
// panic!("Distance ({}) is outside range!", distance_code);
// }
// let length = DISTANCE_EXTRA_BITS[distance_code - DISTANCES_START];
// let bits =
// }

#[derive(Copy, Clone)]
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
    fn from_reversed_bits(code: u16, length: u8) -> Option<HuffmanCode> {
        let reversed = reverse_bits(code, length);
        match reversed {
            Some(c) => {
                Some(HuffmanCode {
                    code: c,
                    length: length,
                })
            }
            None => None,
        }
    }
}

pub struct LengthAndDistanceBits {
    pub length_code: HuffmanCode,
    pub length_extra_bits: HuffmanCode,
    pub distance_code: HuffmanCode,
    pub distance_extra_bits: HuffmanCode,
}

/// A structure containing the tables of huffman codes for lengths, literals and distances
pub struct HuffmanTable {
    // Literal, end of block and length codes
    codes: Vec<HuffmanCode>, // [HuffmanCode; 288],
    // Distance codes
    distance_codes: Vec<HuffmanCode>, // [HuffmanCode; 30],
}

/// Counts the number of values of each length.
/// Returns a tuple containing the longest length value in the table and a vector of lengths
/// Returns an error if `table` is empty, or if any of the lenghts exceed 15
fn build_length_count_table(table: &[u8]) -> Result<(usize, Vec<u8>), HuffmanError> {
    // TODO: Validate the length table properly
    const MAX_CODE_LENGTH: usize = 15;

    let max_length = match table.iter().max() {
        Some(l) => *l as usize,
        None => return Err(HuffmanError::EmptyLengthTable),
    };

    // Check if the longest code length is longer than allowed
    if max_length > MAX_CODE_LENGTH {
        return Err(HuffmanError::CodeTooLong);
    }

    let mut len_counts = vec![0u8; max_length as usize + 1];
    for length in table {
        let mut num_lengths = len_counts[*length as usize];
        num_lengths += 1;
        // TODO: Make sure we don't have more of one length than we can make
        // codes for
        len_counts[*length as usize] = num_lengths;
    }
    Ok((max_length, len_counts))
}

/// Generats a vector of huffman codes given a table of bit lengths
/// Returns an error if any of the lengths are > 15
fn create_codes(length_table: &[u8]) -> Result<Vec<HuffmanCode>, HuffmanError> {

    let mut codes = vec!(HuffmanCode {
        code: 0,
        length: 0,
    }; length_table.len());

    let (max_length, lengths) = try!(build_length_count_table(length_table));

    let mut code = 0u16;
    let mut next_code = vec![0];

    for bits in 1..max_length + 1 {
        code = (code + lengths[bits - 1] as u16) << 1;
        next_code.push(code);
    }

    for n in 0..codes.len() {
        let length = length_table[n] as usize;
        //            println!("n: {}, length: {}", n, length);
        // TODO: Spec says codes of length 0 should not be assigned a value
        // Should we use a table of options here?
        if length == 0 {
            return Err(HuffmanError::ZeroLengthCode);
        }
        // The algorithm generats the code in the reverse bit order, so we need to reverse them
        // to get the correct codes.
        codes[n] = try!(HuffmanCode::from_reversed_bits(next_code[length], length as u8)
            .ok_or(HuffmanError::CodeTooLong));
        next_code[length] += 1;
    }
    Ok(codes)
}

impl HuffmanTable {
    /// Creates a `HuffmanTable` from length tables.
    /// Returns Err if the tables have lengths > 50, the
    /// tables are too short, or are otherwise not formed correctly.
    pub fn from_length_tables(literals_and_lengths: &[u8],
                              distances: &[u8])
                              -> Result<HuffmanTable, HuffmanError> {
        let mut ret = HuffmanTable {
            codes: vec!(HuffmanCode {
                code: 0,
                length: 0,
            }; 288),
            distance_codes: vec!(HuffmanCode {
                code: 0,
                length: 0,
            }; 30),
        };

        let literal_and_length_codes = try!(create_codes(literals_and_lengths));
        let distance_codes = try!(create_codes(distances));

        ret.codes = literal_and_length_codes;
        ret.distance_codes = distance_codes;

        Ok(ret)
    }

    /// Create a HuffmanTable using the fixed tables specified in the DEFLATE format specification.
    #[allow(dead_code)]
    pub fn fixed_table() -> HuffmanTable {
        // This should be safe to unwrap, if it were to panic the code is wrong,
        // tests should catch it.
        HuffmanTable::from_length_tables(&FIXED_CODE_LENGTHS, &FIXED_CODE_LENGTHS_DISTANCE)
            .expect("Error: Failed to build table for fixed huffman codes, this indicates an \
                     error somewhere in the code.")
    }

    /// Get the huffman code from the corresponding literal value
    pub fn get_literal(&self, value: u8) -> HuffmanCode {
        self.codes[value as usize]
    }

    /// Get the huffman code for the end of block value
    pub fn get_end_of_block(&self) -> HuffmanCode {
        const END_OF_BLOCK_POSITION: usize = 256;
        self.codes[END_OF_BLOCK_POSITION]
    }

    pub fn get_length_distance_code(&self,
                                    length: u16,
                                    distance: u16)
                                    -> Option<(LengthAndDistanceBits)> {

        // FIXME: We should probably validate using table length
        if length < MIN_MATCH || length > MAX_MATCH {
            return None;
        }

        const MIN_DISTANCE: u16 = 1;
        const MAX_DISTANCE: u16 = 32768;
        if distance < MIN_DISTANCE || distance > MAX_DISTANCE {
            return None;
        }

        let length_data = match get_length_code_and_extra_bits(length) {
            Some(t) => t,
            None => return None,
        };

        let length_huffman_code = self.codes[length_data.code_number as usize];

        let distance_data = match get_distance_code_and_extra_bits(distance) {
            Some(t) => t,
            None => return None,
        };

        let distance_huffman_code = self.distance_codes[distance_data.code_number as usize];

        Some(LengthAndDistanceBits {
            length_code: length_huffman_code,
            length_extra_bits: HuffmanCode {
                code: length_data.value,
                length: length_data.num_bits,
            },
            distance_code: distance_huffman_code,
            distance_extra_bits: HuffmanCode {
                code: distance_data.value,
                length: distance_data.num_bits,
            },
        })
    }
}

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

    //    println!("Correct code from pos: {:?}", table.codes[258]);
    println!("257: {:?}, 258: {:?}, 259: {:?}",
             table.codes[257],
             table.codes[258],
             table.codes[259]);
    println!("Length code: {:b}, code length: {}",
             ld.length_code.code,
             ld.length_code.length);

    println!("Distance code {:?}", table.distance_codes[4]);

    assert_eq!(ld.length_code.code, 0b00100000);
    assert_eq!(ld.distance_code.code, 0b00100);
    assert_eq!(ld.distance_extra_bits.length, 1);
    assert_eq!(ld.distance_extra_bits.code, 0);
}

#[test]
fn test_bit_reverse() {
    let bits = 0b0111_0100;
    let reversed = reverse_bits(bits, 8).expect("reverse_bits returned None!");
    assert_eq!(reversed, 0b0010_1110);
}
