use length_encode::EncodedLength;
use length_encode::{encode_lengths, huffman_lengths_from_frequency, COPY_PREVIOUS,
                    REPEAT_ZERO_3_BITS, REPEAT_ZERO_7_BITS};
use huffman_table::{create_codes, NUM_LITERALS_AND_LENGTHS, NUM_DISTANCE_CODES};

use bitstream::{BitWriter, LsbWriter};
use std::io::{Write, Result};
use std::cmp;

// The minimum number of literal/length values
pub const MIN_NUM_LITERALS_AND_LENGTHS: usize = 257;
// The minimum number of distances
pub const MIN_NUM_DISTANCES: usize = 1;

// The output ordering of the lenghts for the huffman codes used to encode the lenghts
// used to build the full huffman tree for length/literal codes.
// http://www.gzip.org/zlib/rfc-deflate.html#dyn
const HUFFMAN_LENGTH_ORDER: [u8; 19] = [16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14,
                                        1, 15];

// Number of bits used for the values specifying the number of codes
const HLIT_BITS: u8 = 5;
const HDIST_BITS: u8 = 5;
const HCLEN_BITS: u8 = 4;

// The longest a huffman code describing another huffman length can be
const MAX_HUFFMAN_CODE_LENGTH: usize = 7;

/// Creates a new slice from the input slice that stops at the final non-zero value
pub fn remove_trailing_zeroes<T: From<u8> + PartialEq>(input: &[T], min_length: usize) -> &[T] {
    let num_zeroes = input.iter().rev().take_while(|&a| *a == T::from(0)).count();
    &input[0..cmp::max(input.len() - num_zeroes, min_length)]
}

/// Write the specified huffman lengths to the bit writer
pub fn write_huffman_lengths<W: Write>(literal_len_lengths: &[u8],
                                       distance_lengths: &[u8],
                                       writer: &mut LsbWriter<W>)
                                       -> Result<()> {

    assert!(literal_len_lengths.len() <= NUM_LITERALS_AND_LENGTHS);
    assert!(literal_len_lengths.len() >= MIN_NUM_LITERALS_AND_LENGTHS);
    assert!(distance_lengths.len() <= NUM_DISTANCE_CODES);
    assert!(distance_lengths.len() >= MIN_NUM_DISTANCES);

    // Number of length codes - 257
    let hlit = (literal_len_lengths.len() - MIN_NUM_LITERALS_AND_LENGTHS) as u16;
    try!(writer.write_bits(hlit, HLIT_BITS));
    // Number of distance codes - 1
    let hdist = (distance_lengths.len() - MIN_NUM_DISTANCES) as u16;
    try!(writer.write_bits(hdist, HDIST_BITS));

    // Encode length values
    let (encoded, freqs) =
        encode_lengths(literal_len_lengths.iter().chain(distance_lengths.iter()).cloned()).unwrap();

    // Create huffman lengths for the length/distance code lengths
    let huffman_table_lengths = huffman_lengths_from_frequency(&freqs, MAX_HUFFMAN_CODE_LENGTH);

    let used_hclens = HUFFMAN_LENGTH_ORDER.len() -
                      HUFFMAN_LENGTH_ORDER.iter()
        .rev()
        .take_while(|&&n| huffman_table_lengths[n as usize] == 0)
        .count();

    // Number of huffman table lengths - 4
    let hclen = used_hclens - 4;

    try!(writer.write_bits(hclen as u16, HCLEN_BITS));

    // Write the lengths for the huffman table describing the huffman table
    // Each length is 3 bits
    for n in &HUFFMAN_LENGTH_ORDER[..used_hclens] {
        try!(writer.write_bits(huffman_table_lengths[usize::from(*n)] as u16, 3));
    }

    // Generate codes for the main huffman table using the lengths we just wrote
    let codes = create_codes(&huffman_table_lengths).expect("Failed to create huffman codes!");

    // Write the actual huffman lengths
    for v in encoded {
        match v {
            EncodedLength::Length(n) => {
                let code = codes[usize::from(n)];
                try!(writer.write_bits(code.code, code.length));
            }
            EncodedLength::CopyPrevious(n) => {
                let code = codes[COPY_PREVIOUS];
                try!(writer.write_bits(code.code, code.length));
                assert!(n >= 3);
                assert!(n <= 6);
                try!(writer.write_bits((n - 3).into(), 2));
            }
            EncodedLength::RepeatZero3Bits(n) => {
                let code = codes[REPEAT_ZERO_3_BITS];
                try!(writer.write_bits(code.code, code.length));
                assert!(n >= 3);
                try!(writer.write_bits((n - 3).into(), 3));
            }
            EncodedLength::RepeatZero7Bits(n) => {
                let code = codes[REPEAT_ZERO_7_BITS];
                try!(writer.write_bits(code.code, code.length));
                assert!(n >= 11);
                assert!(n <= 138);
                try!(writer.write_bits((n - 11).into(), 7));
            }
        }
    }
    Ok(())
}
