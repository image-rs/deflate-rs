use length_encode::EncodedLength;
use length_encode::{encode_lengths, huffman_lengths_from_frequency, COPY_PREVIOUS,
                    REPEAT_ZERO_3_BITS, REPEAT_ZERO_7_BITS};
use huffman_table::{create_codes, NUM_LITERALS_AND_LENGTHS, NUM_DISTANCE_CODES, MAX_CODE_LENGTH};
use bitstream::{BitWriter, LsbWriter};
use output_writer::FrequencyType;

use std::io::{Write, Result};
use std::cmp;

// The minimum number of literal/length values
pub const MIN_NUM_LITERALS_AND_LENGTHS: usize = 257;
// The minimum number of distances
pub const MIN_NUM_DISTANCES: usize = 1;

// The output ordering of the lengths for the huffman codes used to encode the lengths
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

// How many bytes (not including padding and the 3-bit block type) the stored block header takes up.
const STORED_BLOCK_HEADER_LENGTH: u64 = 4;

/// Creates a new slice from the input slice that stops at the final non-zero value
pub fn remove_trailing_zeroes<T: From<u8> + PartialEq>(input: &[T], min_length: usize) -> &[T] {
    let num_zeroes = input.iter().rev().take_while(|&a| *a == T::from(0)).count();
    &input[0..cmp::max(input.len() - num_zeroes, min_length)]
}

/// Calculate the number of bits that will be used to represent a block with the given frequencies
/// and code lengths (not including headers).
///
/// Frequencies that are missing code lengths or vice versa will not be counted. This should
/// be okay, as the places this function is used any frequencies that are missing lengths
/// should be zero anyhow.
fn calculate_block_length(frequencies: &[FrequencyType], code_lengths: &[u8]) -> u64 {
    debug_assert!(frequencies.len() >= code_lengths.len());
    frequencies.iter()
        .zip(code_lengths)
        .fold(0, |acc, (&f, &l)| acc + (u64::from(f) * u64::from(l)))
}

/// A struct containing the different data needed to write the header for a dynamic block.
pub struct DynamicBlockHeader {
    /// Literal/length huffman lengths.
    pub l_lengths: Vec<u8>,
    /// Distance huffman lengths.
    pub d_lengths: Vec<u8>,
    /// Run-length encoded huffman length values.
    pub encoded_lengths: Vec<EncodedLength>,
    /// Length of the run-length encoding symbols.
    pub huffman_table_lengths: Vec<u8>,
}

/// Generate the lengths of the huffman codes we will be using, using the
/// frequency of the different symbols/lengths/distances, checking if the block does not expand
/// in size.
pub fn gen_huffman_lengths(l_freqs: &[FrequencyType],
                           d_freqs: &[FrequencyType],
                           num_input_bytes: u64)
                           -> Option<DynamicBlockHeader> {

    // The huffman spec allows us to exclude zeroes at the end of the
    // table of huffman lengths.
    // Since a frequency of 0 will give an huffman
    // length of 0. We strip off the trailing zeroes before even
    // generating the lengths to save some work.
    // There is however a minimum number of values we have to keep
    // according to the deflate spec.
    let l_lengths =
        huffman_lengths_from_frequency(remove_trailing_zeroes(l_freqs,
                                                              MIN_NUM_LITERALS_AND_LENGTHS),
                                       MAX_CODE_LENGTH);
    let d_lengths = huffman_lengths_from_frequency(remove_trailing_zeroes(d_freqs,
                                                                          MIN_NUM_DISTANCES),
                                                   MAX_CODE_LENGTH);


    // Encode length values
    let (encoded, freqs) = encode_lengths(l_lengths.iter().chain(d_lengths.iter()).cloned())
        .unwrap();

    // Create huffman lengths for the length/distance code lengths
    let huffman_table_lengths = huffman_lengths_from_frequency(&freqs, MAX_HUFFMAN_CODE_LENGTH);


    // Calculate how many bytes of space this block will take up.
    let total_compressed_length =
        (calculate_block_length(l_freqs, &l_lengths) + calculate_block_length(d_freqs, &d_lengths) +
         calculate_block_length(&freqs, &huffman_table_lengths)) / 8;

    // Check if the block is actually compressed. If using a dynamic block
    // increases the length of the block (for instance if the input data is mostly random or
    // already compressed), we want to output a stored(uncompressed) block instead to avoid wasting
    // space.
    if total_compressed_length > num_input_bytes + STORED_BLOCK_HEADER_LENGTH {
        None
    } else {
        Some(DynamicBlockHeader {
            l_lengths: l_lengths,
            d_lengths: d_lengths,
            encoded_lengths: encoded,
            huffman_table_lengths: huffman_table_lengths,
        })
    }
}

/// Write the specified huffman lengths to the bit writer
pub fn write_huffman_lengths<W: Write>(header: &DynamicBlockHeader,
                                       writer: &mut LsbWriter<W>)
                                       -> Result<()> {

    let literal_len_lengths = &header.l_lengths;
    let distance_lengths = &header.d_lengths;
    let huffman_table_lengths = &header.huffman_table_lengths;
    let encoded_lengths = &header.encoded_lengths;

    assert!(literal_len_lengths.len() <= NUM_LITERALS_AND_LENGTHS);
    assert!(literal_len_lengths.len() >= MIN_NUM_LITERALS_AND_LENGTHS);
    assert!(distance_lengths.len() <= NUM_DISTANCE_CODES);
    assert!(distance_lengths.len() >= MIN_NUM_DISTANCES);

    // Number of length codes - 257
    let hlit = (literal_len_lengths.len() - MIN_NUM_LITERALS_AND_LENGTHS) as u16;
    writer.write_bits(hlit, HLIT_BITS)?;
    // Number of distance codes - 1
    let hdist = (distance_lengths.len() - MIN_NUM_DISTANCES) as u16;
    writer.write_bits(hdist, HDIST_BITS)?;

    let used_hclens = HUFFMAN_LENGTH_ORDER.len() -
                      HUFFMAN_LENGTH_ORDER.iter()
        .rev()
        .take_while(|&&n| huffman_table_lengths[n as usize] == 0)
        .count();

    // Number of huffman table lengths - 4
    // TODO: Is this safe?
    let hclen = used_hclens.saturating_sub(4);

    writer.write_bits(hclen as u16, HCLEN_BITS)?;

    // Write the lengths for the huffman table describing the huffman table
    // Each length is 3 bits
    for n in &HUFFMAN_LENGTH_ORDER[..used_hclens] {
        writer.write_bits(huffman_table_lengths[usize::from(*n)] as u16, 3)?;
    }

    // Generate codes for the main huffman table using the lengths we just wrote
    let codes = create_codes(huffman_table_lengths).expect("Failed to create huffman codes!");

    // Write the actual huffman lengths
    for v in encoded_lengths {
        match *v {
            EncodedLength::Length(n) => {
                let code = codes[usize::from(n)];
                writer.write_bits(code.code, code.length)?;
            }
            EncodedLength::CopyPrevious(n) => {
                let code = codes[COPY_PREVIOUS];
                writer.write_bits(code.code, code.length)?;
                assert!(n >= 3);
                assert!(n <= 6);
                writer.write_bits((n - 3).into(), 2)?;
            }
            EncodedLength::RepeatZero3Bits(n) => {
                let code = codes[REPEAT_ZERO_3_BITS];
                writer.write_bits(code.code, code.length)?;
                assert!(n >= 3);
                writer.write_bits((n - 3).into(), 3)?;
            }
            EncodedLength::RepeatZero7Bits(n) => {
                let code = codes[REPEAT_ZERO_7_BITS];
                writer.write_bits(code.code, code.length)?;
                assert!(n >= 11);
                assert!(n <= 138);
                writer.write_bits((n - 11).into(), 7)?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod test {
    use super::calculate_block_length;

    #[test]
    fn block_length() {
        let freqs = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 44, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                     0, 0, 0, 0, 0, 0, 0, 68, 0, 14, 0, 0, 0, 0, 3, 7, 6, 1, 0, 12, 14, 9, 2, 6,
                     9, 4, 1, 1, 4, 1, 1, 0, 0, 1, 3, 0, 6, 0, 0, 0, 4, 4, 1, 2, 5, 3, 2, 2, 9, 0,
                     0, 3, 1, 5, 5, 8, 0, 6, 10, 5, 2, 0, 0, 1, 2, 0, 8, 11, 4, 0, 1, 3, 31, 13,
                     23, 22, 56, 22, 8, 11, 43, 0, 7, 33, 15, 45, 40, 16, 1, 28, 37, 35, 26, 3, 7,
                     11, 9, 1, 1, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                     0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                     0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                     0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                     0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                     0, 0, 0, 0, 0, 0, 0, 1, 126, 114, 66, 31, 41, 25, 15, 21, 20, 16, 15, 10, 7,
                     5, 1, 1];


        let lens = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 5, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                    0, 0, 0, 0, 0, 0, 4, 0, 7, 0, 0, 0, 0, 9, 8, 8, 10, 0, 7, 7, 7, 10, 8, 7, 8,
                    10, 10, 8, 10, 10, 0, 0, 10, 9, 0, 8, 0, 0, 0, 8, 8, 10, 9, 8, 9, 9, 9, 7, 0,
                    0, 9, 10, 8, 8, 7, 0, 8, 7, 8, 9, 0, 0, 10, 9, 0, 7, 7, 8, 0, 10, 9, 6, 7, 6,
                    6, 5, 6, 7, 7, 5, 0, 8, 5, 7, 5, 5, 6, 10, 6, 5, 5, 6, 9, 8, 7, 7, 10, 10, 0,
                    10, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                    0, 10, 4, 4, 4, 5, 5, 6, 7, 6, 6, 6, 6, 7, 8, 8, 10, 10];

        assert_eq!(calculate_block_length(&freqs, &lens), 7701);
    }
}
