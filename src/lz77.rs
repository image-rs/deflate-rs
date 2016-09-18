use std::cmp;

use huffman_table::{MAX_MATCH, MIN_MATCH};
use chained_hash_table::{WINDOW_SIZE, ChainedHashTable};
use output_writer::{OutputWriter, FixedWriter};
use checksum::RollingChecksum;

/// A struct that contains the hash table, and keeps track of where we are in the input data
pub struct LZ77State {
    hash_table: ChainedHashTable,
    // The current position in the input slice
    pub current_start: usize,
    // True if this is the first window
    is_first_window: bool,
    // True if the last block has been output
    is_last_block: bool,
}

impl LZ77State {
    fn from_starting_values(b0: u8, b1: u8) -> LZ77State {
        LZ77State {
            hash_table: ChainedHashTable::from_starting_values(b0, b1),
            current_start: 0,
            is_first_window: true,
            is_last_block: false,
        }
    }

    /// Creates a new LZ77 state, adding the first to bytes to the hash table
    pub fn new(data: &[u8]) -> LZ77State {
        LZ77State::from_starting_values(data[0], data[1])
    }

    pub fn set_last(&mut self) {
        self.is_last_block = true;
    }

    pub fn is_last_block(&self) -> bool {
        self.is_last_block
    }
}

/// A structure representing values in a compressed stream of data before being huffman coded
/// We might want to represent this differently eventually to save on memory usage
/// (We don't actually need the full 16 bytes to store the length and distance data)
#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub enum LDPair {
    Literal(u8),
    LengthDistance { length: u16, distance: u16 },
    EndOfBlock,
}

/// Get the length of the checked match
/// The function returns number of bytes after and including `current_pos` match
fn get_match_length(data: &[u8], current_pos: usize, pos_to_check: usize) -> usize {
    // TODO: This can be optimised by checking multiple bytes at once and not checking the
    // first 3 bytes since we already know they match
    data[current_pos..]
        .iter()
        .zip(data[pos_to_check..].iter())
        .enumerate()
        .take_while(|&(n, (&a, &b))| n < MAX_MATCH as usize && a == b)
        .count()
}

/// Try finding the position and length of the longest match in the input data.
/// If no match is found that was better than `prev_length` or at all, or we are at the start
/// This returns (0, 0)
fn longest_match(data: &[u8],
                 hash_table: &ChainedHashTable,
                 position: usize,
                 prev_length: usize)
                 -> (usize, usize) {
    // If we are at the start, or we already have a match at the match length, we stop here.
    if position == 0 || prev_length >= MAX_MATCH as usize{
        return (0, 0);
    }

    let limit = if position > WINDOW_SIZE {
        position - WINDOW_SIZE
    } else {
        0
    };

    let max_length = cmp::min((data.len() - position), MAX_MATCH as usize);

    let mut current_head = hash_table.get_prev(hash_table.current_head() as usize) as usize;
    let starting_head = current_head;

    let mut best_length = prev_length as usize;//MIN_MATCH - 1;
    let mut best_distance = 0;

    while (current_head) >= limit && current_head != 0 {
        let distance = position - current_head;

        // We only check further if the match length can actually increase
        if distance > 0 && (position + best_length) < data.len() &&
           data[position + best_length] ==
           data[current_head + best_length] {
            let length = get_match_length(data, position, current_head);
            if length > best_length {
                best_length = length;
                best_distance = distance;
                if length == max_length {
                    // We are at the max length, so there is no point
                    // searching any longer
                    break;
                }
            }
        }
        current_head = hash_table.get_prev(current_head) as usize;
        if current_head == starting_head {
            // We've gone through one cycle.
            break;
        }
    }
    if best_length > prev_length {
        (best_length, best_distance)
    } else {
        (0, 0)
    }
}

// Get the longest match from the current position of the hash table
#[inline]
#[cfg(test)]
fn longest_match_current(data: &[u8], hash_table: &ChainedHashTable) -> (usize, usize) {
    longest_match(data,
                  hash_table,
                  hash_table.current_position(),
                  MIN_MATCH as usize- 1)
}

const DEFAULT_WINDOW_SIZE: usize = 32768;

// fn add_value<RC: RollingChecksum>(hash_table: &mut ChainedHashTable, rolling_checksum: RC) {
// hash_table.
// }

fn process_chunk<W: OutputWriter, RC: RollingChecksum>(data: &[u8],
                                                       start: usize,
                                                       end: usize,
                                                       hash_table: &mut ChainedHashTable,
                                                       writer: &mut W,
                                                       rolling_checksum: &mut RC) {
    let end = cmp::min(data.len(), end);
    let current_chunk = &data[start..end];
    let mut insert_it = current_chunk.iter().enumerate();
    let mut hash_it = (&data[start + 2..]).iter();

    // Iterate through the slice, adding literals or length/distance pairs
    while let Some((n, &b)) = insert_it.next() {
        if let Some(&hash_byte) = hash_it.next() {
            let position = n + start;
            hash_table.add_hash_value(position, hash_byte);
            rolling_checksum.update(hash_byte);
            // TODO: Currently, we only check for matches up to the end of the chunk, but ideally
            // we should be checking max_match bytes further to achieve the best possible
            // compression.
            let (match_len, match_dist) =
                longest_match(&data[..end], hash_table, position, MIN_MATCH as usize - 1);

            // If the match length is less than MIN_MATCH, we didn't find a match
            if match_len >= MIN_MATCH as usize {
                // TODO: Add heuristic checking if outputting a length/distance pair will actually
                // be shorter than adding the literal bytes

                let (to_subtract_from_length, length, distance) = {

                    let _ = insert_it.next()
                        .expect("Reached the end of the array even though we had a match! This \
                                 suggests a bug somewhere!");
                    let next_pos = position + 1;

                    if let Some(&next_hash_byte) = hash_it.next() {
                        hash_table.add_hash_value(next_pos, next_hash_byte);
                        rolling_checksum.update(next_hash_byte);

                        // Check if we can find a better match at the next byte than the one we currently have
                        let (next_match_len, next_match_dist) =
                            longest_match(&data[..end], hash_table, next_pos, match_len);
                        if next_match_len > match_len {
                            // We found a better match, so output the previous byte
                            writer.write_literal(b);
                            (0, next_match_len, next_match_dist)
                        } else {
                            (1, match_len, match_dist)
                        }
                    } else {
                        (1, match_len, match_dist)
                    }
                };
                // Casting note: length and distance is already bounded by the longest match function
                // Usize is just used for convenience
                writer.write_length_distance(length as u16, distance as u16);

                // If we did not find a better match we need to iterate one time less as we've already added
                // the next byte to the hash table and checksum
                let bytes_to_add = length - to_subtract_from_length;

                let taker = insert_it.by_ref().take(bytes_to_add - 1);
                let mut hash_taker = hash_it.by_ref().take(bytes_to_add - 1);

                // Advance the iterators and add the bytes we jump over to the hash table and checksum
                for (ipos, _) in taker {
                    if let Some(&i_hash_byte) = hash_taker.next() {
                        rolling_checksum.update(i_hash_byte);
                        hash_table.add_hash_value(ipos + start, i_hash_byte);
                    }
                }


            } else {
                // There was no match, so we simply output another literal
                writer.write_literal(b);
            }
        } else {
            // We are at the last two bytes we want to add, so there is no point searching for matches here.
            writer.write_literal(b);
        }
    }
}

/// Compress a slice
/// Will return err on failure eventually, but for now allways succeeds or panics
pub fn lz77_compress_block<W: OutputWriter, RC: RollingChecksum>(data: &[u8],
                                                                 state: &mut LZ77State,
                                                                 mut writer: &mut W,
                                                                 mut rolling_checksum: &mut RC)
                                                                 -> Option<bool> {
    // Currently we use window size as block length, in the future we might want to allow
    // differently sized blocks
    let window_size = DEFAULT_WINDOW_SIZE;

    // If the next block is very short, we merge it into the current block as the huffman tables
    // for a new block may otherwise waste space.
    const MIN_BLOCK_LENGTH: usize = 500;
    let next_block_merge = data.len() - state.current_start > window_size && data.len() - state.current_start - window_size < MIN_BLOCK_LENGTH;

    if state.is_first_window {

        let first_chunk_end = if next_block_merge {
            data.len()
        } else {
            cmp::min(window_size, data.len())
        };
        process_chunk::<W, RC>(data,
                               0,
                               first_chunk_end,
                               &mut state.hash_table,
                               &mut writer,
                               &mut rolling_checksum);
        // We are at the first block so we don't need to slide the hash table
        state.current_start += first_chunk_end;
        if first_chunk_end >= data.len() {
            state.set_last();
        }
        state.is_first_window = false;
    } else {
        for _ in 0..1 {
            let start = state.current_start;
            let slice = &data[start - window_size..];
            let end = cmp::min(window_size * 2, slice.len());
            process_chunk::<W, RC>(slice,
                               window_size,
                               end,
                               &mut state.hash_table,
                               &mut writer,
                               &mut rolling_checksum);
            if end >= slice.len() {
                state.set_last();
            } else {
                state.current_start += window_size;
                // We slide the hash table back to make space for new hash values
                // We only need to remember 32k bytes back (the maximum distance allowed by the
                // deflate spec)
                state.hash_table.slide(window_size);
            }

            if !next_block_merge {
                break;
            }
        }
    }

    writer.write_end_of_block();

    Some(true)
}

/// Compress a slice, not storing frequency information
///
/// This is a convenience function for compression with fixed huffman values
/// Only used in tests for now
#[allow(dead_code)]
pub fn lz77_compress(data: &[u8]) -> Option<Vec<LDPair>> {
    use checksum::NoChecksum;
    let mut w = FixedWriter::new();
    let mut state = LZ77State::new(data);
    let mut dummy_checksum = NoChecksum::new();
    while !state.is_last_block {
        lz77_compress_block(data, &mut state, &mut w, &mut dummy_checksum);
    }
    Some(w.buffer)
}

#[cfg(test)]
mod test {
    use super::*;

    fn decompress_lz77(input: &[LDPair]) -> Vec<u8> {
        let mut output = Vec::new();
        for p in input {
            match *p {
                LDPair::Literal(l) => output.push(l),
                LDPair::LengthDistance { distance: d, length: l } => {
                    let start = output.len() - d as usize;
                    let mut n = 0;
                    while n < l as usize {
                        let b = output[start + n];
                        output.push(b);
                        n += 1;
                    }
                }
                LDPair::EndOfBlock => (),
            }
        }
        output
    }

    /// Test that match lengths are calculated correctly
    #[test]
    fn test_match_length() {
        let test_arr = [5u8, 5, 5, 5, 5, 9, 9, 2, 3, 5, 5, 5, 5, 5];
        let l = super::get_match_length(&test_arr, 9, 0);
        assert_eq!(l, 5);
        let l2 = super::get_match_length(&test_arr, 9, 7);
        assert_eq!(l2, 0);
        let l3 = super::get_match_length(&test_arr, 10, 0);
        assert_eq!(l3, 4);
    }

    /// Test that we get the longest of the matches
    #[test]
    fn test_longest_match() {
        use chained_hash_table::{filled_hash_table, HASH_BYTES};
        use std::str::from_utf8;

        let test_data = b"xTest data, Test_data,zTest data";
        let hash_table = filled_hash_table(&test_data[..23 + 1 + HASH_BYTES - 1]);

        println!("Bytes: {}",
                 from_utf8(&test_data[..23 + 1 + HASH_BYTES - 1]).unwrap());
        println!("23: {}", from_utf8(&[test_data[23]]).unwrap());
        let (length, distance) = super::longest_match_current(test_data, &hash_table);
        println!("Distance: {}", distance);
        // We check that we get the longest match, rather than the shorter, but closer one.
        assert_eq!(distance, 22);
        assert_eq!(length, 9);
        let test_arr2 = [10u8, 10, 10, 10, 10, 10, 10, 10, 2, 3, 5, 10, 10, 10, 10, 10];
        let hash_table = filled_hash_table(&test_arr2[..HASH_BYTES + 1 + 1 + 2]);
        let (length, distance) = super::longest_match_current(&test_arr2, &hash_table);
        println!("Distance: {}, length: {}", distance, length);
        assert_eq!(distance, 1);
        assert_eq!(length, 4);
    }

    /// Helper function to print the output from the lz77 compression function
    fn print_output(input: &[LDPair]) {
        let mut output = vec![];
        for l in input {
            match *l {
                LDPair::Literal(l) => output.push(l),
                LDPair::LengthDistance { distance: d, length: l } => {
                    output.extend(format!("<Distance: {}, Length: {}>", d, l).into_bytes())
                }
                LDPair::EndOfBlock => {
                    output.extend(format!("<End of block>").into_bytes());
                }
            }
        }

        println!("{}", String::from_utf8(output).unwrap());
    }

    /// Test that a short string from an example on SO compresses correctly
    #[test]
    fn test_lz77_short() {
        use std::str;

        let test_bytes = String::from("Deflate late").into_bytes();
        let res = super::lz77_compress(&test_bytes).unwrap();
        // println!("{:?}", res);
        // TODO: Check that compression is correct
        print_output(&res);
        let decompressed = decompress_lz77(&res);
        let d_str = str::from_utf8(&decompressed).unwrap();
        println!("{}", d_str);
        assert_eq!(test_bytes, decompressed);
        assert_eq!(res[8],
                   LDPair::LengthDistance {
                       distance: 5,
                       length: 4,
                   });
    }

    /// Test that compression is working for a longer file
    #[test]
    fn test_lz77_long() {
        use std::fs::File;
        use std::io::Read;
        use std::str;
        let mut input = Vec::new();

        let mut f = File::open("src/pg11.txt").unwrap();
        f.read_to_end(&mut input).unwrap();
        let compressed = super::lz77_compress(&input).unwrap();
        assert!(compressed.len() < input.len());
        //print_output(&compressed);
        let decompressed = decompress_lz77(&compressed);
        //println!("{}", str::from_utf8(&decompressed).unwrap());
        // assert_eq!(input.len(), decompressed.len());
        assert!(&decompressed == &input);
    }

    /// Test that matches at the window border are working correctly
    #[test]
    fn test_lz77_border() {
        use chained_hash_table::WINDOW_SIZE;
        let data = vec![0; WINDOW_SIZE + 50];
        let compressed = super::lz77_compress(&data).unwrap();
        assert!(compressed.len() < data.len());
        let decompressed = decompress_lz77(&compressed);
        assert!(decompressed == data);
    }

    #[test]
    fn test_lz77_border_multiple_blocks() {
        use chained_hash_table::WINDOW_SIZE;
        let data = vec![0; (WINDOW_SIZE * 2) + 50];
        let compressed = super::lz77_compress(&data).unwrap();
        assert!(compressed.len() < data.len());
        let decompressed = decompress_lz77(&compressed);
        assert!(decompressed == data);
    }
}
