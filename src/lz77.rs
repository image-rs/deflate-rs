//! This module contains functionality for doing lz77 compression of data.
use std::cmp;
use std::ops::Range;

use input_buffer::InputBuffer;
use matching::longest_match;
use lzvalue::LZValue;
use huffman_table;
use chained_hash_table::ChainedHashTable;
use compression_options::{HIGH_MAX_HASH_CHECKS, HIGH_LAZY_IF_LESS_THAN};
use output_writer::{OutputWriter, FixedWriter};
use compress::Flush;

const MAX_MATCH: usize = huffman_table::MAX_MATCH as usize;
const MIN_MATCH: usize = huffman_table::MIN_MATCH as usize;

/// An enum describing whether we use lazy or greedy matching.
#[derive(Clone, Copy, Debug)]
pub enum MatchingType {
    /// Use lazy matching: after finding a match, the next input byte is checked, to see
    /// if there is a better match starting at that byte.
    Lazy,
    /// Use greedy matching: the matching algorithm simply uses a match right away
    /// if found.
    Greedy,
}

/// A struct that contains the hash table, and keeps track of where we are in the input data
pub struct LZ77State {
    /// Struct containing hash chains that will be used to find matches.
    hash_table: ChainedHashTable,
    /// True if this is the first window that is being processed.
    is_first_window: bool,
    /// Set to true when the last block has been processed.
    is_last_block: bool,
    /// How many bytes the last match in the previous window extended into the current one.
    overlap: usize,
    /// The maximum number of hash entries to search.
    max_hash_checks: u16,
    /// Only lazy match if we have a match length less than this.
    lazy_if_less_than: u16,
    /// Whether to use greedy or lazy parsing
    matching_type: MatchingType,
}

impl LZ77State {
    fn from_starting_values(b0: u8,
                            b1: u8,
                            max_hash_checks: u16,
                            lazy_if_less_than: u16,
                            matching_type: MatchingType)
                            -> LZ77State {
        LZ77State {
            hash_table: ChainedHashTable::from_starting_values(b0, b1),
            is_first_window: true,
            is_last_block: false,
            overlap: 0,
            max_hash_checks: max_hash_checks,
            lazy_if_less_than: lazy_if_less_than,
            matching_type: matching_type,
        }
    }

    /// Creates a new LZ77 state, adding the first to bytes to the hash value
    /// to warm it up
    pub fn _new_warmup(data: &[u8],
                       max_hash_checks: u16,
                       lazy_if_less_than: u16,
                       matching_type: MatchingType)
                       -> LZ77State {
        LZ77State::from_starting_values(data[0],
                                        data[1],
                                        max_hash_checks,
                                        lazy_if_less_than,
                                        matching_type)
    }

    /// Creates a new LZ77 state
    /// Uses two arbitrary values to warm up the hash
    pub fn new(max_hash_checks: u16,
               lazy_if_less_than: u16,
               matching_type: MatchingType)
               -> LZ77State {
        // Not sure if warming up the hash is actually needed.
        LZ77State::from_starting_values(55, 23, max_hash_checks, lazy_if_less_than, matching_type)
    }

    /// Resets the state excluding max_hash_checks and lazy_if_less_than
    pub fn reset(&mut self) {
        self.hash_table.reset();
        self.is_first_window = true;
        self.is_last_block = false;
        self.overlap = 0;
    }

    pub fn set_last(&mut self) {
        self.is_last_block = true;
    }

    pub fn is_last_block(&self) -> bool {
        self.is_last_block
    }

    pub fn is_first_window(&self) -> bool {
        self.is_first_window
    }
}

/// A structure representing either a literal, length or distance value
#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub enum LDPair {
    Literal(u8),
    Length(u16),
    Distance(u16),
}

const DEFAULT_WINDOW_SIZE: usize = 32768;

fn process_chunk<W: OutputWriter>(data: &[u8],
                                  iterated_data: Range<usize>,
                                  hash_table: &mut ChainedHashTable,
                                  writer: &mut W,
                                  max_hash_checks: u16,
                                  lazy_if_less_than: usize,
                                  matching_type: MatchingType)
                                  -> usize {
    match matching_type {
        MatchingType::Greedy => {
            process_chunk_greedy(data, iterated_data, hash_table, writer, max_hash_checks)
        }
        MatchingType::Lazy => {
            process_chunk_lazy(data,
                               iterated_data,
                               hash_table,
                               writer,
                               max_hash_checks,
                               lazy_if_less_than)
        }
    }
}

fn process_chunk_lazy<W: OutputWriter>(data: &[u8],
                                       iterated_data: Range<usize>,
                                       hash_table: &mut ChainedHashTable,
                                       writer: &mut W,
                                       max_hash_checks: u16,
                                       lazy_if_less_than: usize)
                                       -> usize {
    let end = cmp::min(data.len(), iterated_data.end);
    let start = iterated_data.start;
    let current_chunk = &data[start..end];

    let mut insert_it = current_chunk.iter().enumerate();
    let mut hash_it = {
        let hash_start = if end - start > 2 {
            start + 2
        } else {
            data.len()
        };
        (&data[hash_start..]).iter()
    };

    const NO_LENGTH: usize = MIN_MATCH as usize - 1;

    // The byte before the currently read one in the stream.
    let mut prev_byte = 0u8;
    // The previous match length, if any.
    let mut prev_length = NO_LENGTH;
    // The distance of the previous match if any.
    let mut prev_distance = 0;
    // Whether prev_byte should be output if we move one byte forward to find a better match
    // (or at the end of the stream).
    let mut add = false;
    // The number of bytes past end that was added due to finding a match that extends into
    // the lookahead window.
    let mut overlap = 0;

    // Set to true if we found a match that is equal to or longer than `lazy_if_less_than`,
    // indicating that we won't lazy match (check for a better match at the next byte).
    let mut ignore_next = false;

    // Iterate through the slice, adding literals or length/distance pairs
    while let Some((n, &b)) = insert_it.next() {
        if let Some(&hash_byte) = hash_it.next() {
            let position = n + start;
            hash_table.add_hash_value(position, hash_byte);

            // Only lazy match if we have a match shorter than a set value
            // TODO: This should be cleaned up a bit
            let (match_len, match_dist) = if !ignore_next {
                let (match_len, match_dist) = {
                    // If there already was a decent match at the previous byte
                    // and we are lazy matching, do less match checks in this step.
                    let max_hash_checks = if prev_length >= 32 {
                        max_hash_checks >> 2
                    } else {
                        max_hash_checks
                    };

                    // Check if we can find a better match here than the one we had at
                    // the previous byte.
                    longest_match(data, hash_table, position, prev_length, max_hash_checks)
                };
                if match_len > lazy_if_less_than {
                    // We found a decent match, so we won't check for a better one at the next byte.
                    ignore_next = true;
                }
                (match_len, match_dist)
            } else {
                // We already had a decent match, so we don't bother checking for another one.
                (NO_LENGTH, 0)
            };

            if prev_length >= match_len && prev_length >= MIN_MATCH as usize && prev_distance > 0 {
                // The previous match was better so we add it
                // Casting note: length and distance is already bounded by the longest match
                // function. Usize is just used for convenience
                writer.write_length_distance(prev_length as u16, prev_distance as u16);

                // We add the bytes to the hash table and checksum.
                // Since we've already added two of them, we need to add two less than
                // the length
                let bytes_to_add = prev_length - 2;
                let taker = insert_it.by_ref().take(bytes_to_add);
                let mut hash_taker = hash_it.by_ref().take(bytes_to_add);

                // Advance the iterators and add the bytes we jump over to the hash table and
                // checksum
                for (ipos, _) in taker {
                    if let Some(&i_hash_byte) = hash_taker.next() {
                        hash_table.add_hash_value(ipos + start, i_hash_byte);
                    }
                }

                // If the match is longer than the current window, we have note how many
                // bytes we overlap, since we don't need to do any matching on these bytes
                // in the next call of this function.
                if position + prev_length > end {
                    // We need to subtract 1 since the byte at pos is also included
                    overlap = position + prev_length - end - 1;
                };

                add = false;
                ignore_next = false;

            } else if add {
                // We found a better match (or there was no previous match)
                // so output the previous byte
                writer.write_literal(prev_byte);
            } else {
                add = true
            }

            prev_length = match_len;
            prev_distance = match_dist;
            prev_byte = b;
        } else {
            if add {
                // We may still have a leftover byte at this point, so we add it here if needed.
                writer.write_literal(prev_byte);
                add = false;
            }
            // We are at the last two bytes we want to add, so there is no point
            // searching for matches here.
            writer.write_literal(b);
        }
    }
    if add {
        // We may still have a leftover byte at this point, so we add it here if needed.
        writer.write_literal(prev_byte);
    }
    overlap
}

fn process_chunk_greedy<W: OutputWriter>(data: &[u8],
                                         iterated_data: Range<usize>,
                                         hash_table: &mut ChainedHashTable,
                                         writer: &mut W,
                                         max_hash_checks: u16)
                                         -> usize {
    let end = cmp::min(data.len(), iterated_data.end);
    let start = iterated_data.start;
    let current_chunk = &data[start..end];

    let mut insert_it = current_chunk.iter().enumerate();
    let mut hash_it = {
        let hash_start = if end - start > 2 {
            start + 2
        } else {
            data.len()
        };
        (&data[hash_start..]).iter()
    };

    const NO_LENGTH: usize = MIN_MATCH as usize - 1;

    // The byte before the currently read one in the stream.
    let mut prev_byte = 0u8;
    // Whether prev_byte should be output if we move one byte forward to find a better match
    // (or at the end of the stream).
    let mut add = false;
    // The number of bytes past end that was added due to finding a match that extends into
    // the lookahead window.
    let mut overlap = 0;

    // Iterate through the slice, adding literals or length/distance pairs
    while let Some((n, &b)) = insert_it.next() {
        if let Some(&hash_byte) = hash_it.next() {
            let position = n + start;
            hash_table.add_hash_value(position, hash_byte);

            // TODO: This should be cleaned up a bit
            let (match_len, match_dist) = {
                longest_match(data, hash_table, position, NO_LENGTH, max_hash_checks)
            };

            if match_len >= MIN_MATCH as usize && match_dist > 0 {
                // Casting note: length and distance is already bounded by the longest match
                // function. Usize is just used for convenience
                writer.write_length_distance(match_len as u16, match_dist as u16);

                // We add the bytes to the hash table and checksum.
                // Since we've already added one of them, we need to add one less than
                // the length
                let bytes_to_add = match_len - 1;
                let taker = insert_it.by_ref().take(bytes_to_add);
                let mut hash_taker = hash_it.by_ref().take(bytes_to_add);

                // Advance the iterators and add the bytes we jump over to the hash table and
                // checksum
                for (ipos, _) in taker {
                    if let Some(&i_hash_byte) = hash_taker.next() {
                        hash_table.add_hash_value(ipos + start, i_hash_byte);
                    }
                }

                // If the match is longer than the current window, we have note how many
                // bytes we overlap, since we don't need to do any matching on these bytes
                // in the next call of this function.
                if position + match_len > end {
                    // We need to subtract 1 since the byte at pos is also included
                    overlap = position + match_len - end;
                };

                add = false;
                // There was no match

            } else {
                writer.write_literal(b);
            }
            prev_byte = b;
        } else {
            if add {
                // We may still have a leftover byte at this point, so we add it here if needed.
                writer.write_literal(prev_byte);
                add = false;
            }
            // We are at the last two bytes we want to add, so there is no point
            // searching for matches here.
            writer.write_literal(b);
        }
    }
    if add {
        // We may still have a leftover byte at this point, so we add it here if needed.
        writer.write_literal(prev_byte);
    }
    overlap
}


#[derive(Eq, PartialEq, Clone, Copy, Debug)]
pub enum LZ77Status {
    NeedInput,
    EndBlock,
    Finished,
}

pub fn lz77_compress_block_finish<W: OutputWriter>(data: &[u8],
                                                   state: &mut LZ77State,
                                                   buffer: &mut InputBuffer,
                                                   mut writer: &mut W)
                                                   -> (usize, LZ77Status) {
    lz77_compress_block::<W>(data, state, buffer, &mut writer, Flush::Finish)
}

/// Compress a slice with lz77 compression.
///
/// This function processes one window at a time, and returns when there is no input left,
/// or it determines it's time to end a block.
///
/// Returns the number of bytes of the input that were not processed, and a status describing
/// whether there is no input, it's time to finish, or it's time to end the block.
pub fn lz77_compress_block<W: OutputWriter>(data: &[u8],
                                            state: &mut LZ77State,
                                            buffer: &mut InputBuffer,
                                            mut writer: &mut W,
                                            flush: Flush)
                                            -> (usize, LZ77Status) {
    // Currently we only support the maximum window size
    let window_size = DEFAULT_WINDOW_SIZE;

    let finish = flush == Flush::Finish || flush == Flush::Sync;
    let sync = flush == Flush::Sync;

    let mut status = LZ77Status::EndBlock;
    let mut remaining_data = buffer.add_data(data);

    while writer.buffer_length() < (window_size * 2) {
        if state.is_first_window {
            // Don't do anything until we are either flushing, or we have at least one window of
            // data.
            if buffer.current_end() >= (window_size * 2) + MAX_MATCH || finish {


                let first_chunk_end = if finish && remaining_data.is_none() {
                    // If we are finishing, make sure we include data in the lookahead area
                    buffer.current_end()
                } else {
                    cmp::min(window_size, buffer.current_end())
                };

                state.overlap = process_chunk::<W>(buffer.get_buffer(),
                                                   0..first_chunk_end,
                                                   &mut state.hash_table,
                                                   &mut writer,
                                                   state.max_hash_checks,
                                                   state.lazy_if_less_than as usize,
                                                   state.matching_type);

                // We are at the first window so we don't need to slide the hash table yet,

                if first_chunk_end >= data.len() && finish {
                    if !sync {
                        state.set_last();
                    }
                    status = LZ77Status::Finished;
                } else {
                    status = LZ77Status::EndBlock;
                }
                state.is_first_window = false;
                break;
            } else {
                status = LZ77Status::NeedInput;
                break;
            }
        } else if buffer.current_end() >= (window_size * 2) + MAX_MATCH || finish {
            // This isn't the first chunk, so we start reading at one window in in them
            // buffer plus any additional overlap from earlier.
            let start = window_size + state.overlap;

            // Determine where we have to stop iterating to slide the buffer and hash,
            // or stop because we are at the end of the input data.
            let end = if remaining_data.is_none() && finish {
                // If we are finishing, make sure we include the lookahead data
                buffer.current_end()
            } else {
                // Otherwise we process one window size of data.
                cmp::min(window_size * 2, buffer.current_end())
            };

            state.overlap = process_chunk::<W>(buffer.get_buffer(),
                                               start..end,
                                               &mut state.hash_table,
                                               &mut writer,
                                               state.max_hash_checks,
                                               state.lazy_if_less_than as usize,
                                               state.matching_type);
            if remaining_data.is_none() && finish {
                // We stopped before or at the window size, so we are at the end.
                if !sync {
                    state.set_last();
                } else {
                    // For sync flushing we need to slide the buffer and the has chains so that the
                    // next call to this function starts at the right place.
                    state.overlap = 0;
                    let n = buffer.move_down();
                    state.hash_table.slide(n);
                }
                status = LZ77Status::Finished;
                break;
            } else {
                // We are not at the end, so slide and continue
                // We slide the hash table back to make space for new hash values
                // We only need to remember 32k bytes back (the maximum distance allowed by the
                // deflate spec)
                state.hash_table.slide(window_size);

                // Slide the buffer
                remaining_data = buffer.slide(remaining_data.unwrap_or(&[]));

                status = LZ77Status::EndBlock;
            }
        } else {
            status = LZ77Status::NeedInput;
            break;
        }

    }

    (data.len() - remaining_data.unwrap_or(&[]).len(), status)
}

#[allow(dead_code)]
pub struct TestStruct {
    state: LZ77State,
    buffer: InputBuffer,
    writer: FixedWriter,
}

#[allow(dead_code)]
impl TestStruct {
    fn new() -> TestStruct {
        TestStruct {
            state: LZ77State::new(HIGH_MAX_HASH_CHECKS,
                                  HIGH_LAZY_IF_LESS_THAN,
                                  MatchingType::Lazy),
            buffer: InputBuffer::empty(),
            writer: FixedWriter::new(),
        }
    }

    fn compress_block(&mut self, data: &[u8], flush: bool) -> (usize, LZ77Status) {
        lz77_compress_block(data,
                            &mut self.state,
                            &mut self.buffer,
                            &mut self.writer,
                            if flush { Flush::Finish } else { Flush::None })
    }
}

/// Compress a slice, not storing frequency information
///
/// This is a convenience function for compression with fixed huffman values
/// Only used in tests for now
#[allow(dead_code)]
pub fn lz77_compress(data: &[u8]) -> Option<Vec<LZValue>> {
    let mut test_boxed = Box::new(TestStruct::new());
    let mut out = Vec::<LZValue>::with_capacity(data.len() / 3);
    {
        let mut test = test_boxed.as_mut();
        let mut slice = data;

        while !test.state.is_last_block {
            let bytes_written = lz77_compress_block_finish(slice,
                                                           &mut test.state,
                                                           &mut test.buffer,
                                                           &mut test.writer)
                .0;
            slice = &slice[bytes_written..];
            out.extend(test.writer.get_buffer());
            test.writer.clear_buffer();
        }

    }

    Some(out)
}

#[cfg(test)]
mod test {
    use super::*;
    use lzvalue::LZValue;
    use chained_hash_table::WINDOW_SIZE;
    use compression_options::DEFAULT_LAZY_IF_LESS_THAN;
    use test_utils::get_test_data;

    fn decompress_lz77(input: &[LZValue]) -> Vec<u8> {
        let mut output = Vec::new();
        let mut last_length = 0;
        for p in input {
            match p.value() {
                LDPair::Literal(l) => output.push(l),
                LDPair::Length(l) => last_length = l,
                LDPair::Distance(d) => {
                    let start = output.len() - d as usize;
                    let mut n = 0;
                    while n < last_length as usize {
                        let b = output[start + n];
                        output.push(b);
                        n += 1;
                    }
                }
            }
        }
        output
    }


    /// Helper function to print the output from the lz77 compression function
    fn print_output(input: &[LZValue]) {
        let mut output = vec![];
        for l in input {
            match l.value() {
                LDPair::Literal(l) => output.push(l),
                LDPair::Length(l) => output.extend(format!("<L {}>", l).into_bytes()),
                LDPair::Distance(d) => output.extend(format!("<D {}>", d).into_bytes()),
            }
        }

        println!("\"{}\"", String::from_utf8(output).unwrap());
    }

    /// Test that a short string from an example on SO compresses correctly
    #[test]
    fn compress_short() {
        use std::str;

        let test_bytes = String::from("Deflate late").into_bytes();
        let res = lz77_compress(&test_bytes).unwrap();
        // println!("{:?}", res);
        // TODO: Check that compression is correct
        // print_output(&res);
        let decompressed = decompress_lz77(&res);
        let d_str = str::from_utf8(&decompressed).unwrap();
        println!("{}", d_str);
        assert_eq!(test_bytes, decompressed);
        // assert_eq!(res[8],
        // LDPair::LengthDistance {
        // distance: 5,
        // length: 4,
        // });
    }

    /// Test that compression is working for a longer file
    #[test]
    fn compress_long() {
        use std::str;
        let input = get_test_data();
        let compressed = lz77_compress(&input).unwrap();
        assert!(compressed.len() < input.len());
        // print_output(&compressed);
        let decompressed = decompress_lz77(&compressed);
        // println!("{}", str::from_utf8(&decompressed).unwrap());
        // This is to check where the compression fails, if it were to
        for (n, (&a, &b)) in input.iter().zip(decompressed.iter()).enumerate() {
            if a != b {
                println!("First difference at {}, input: {}, output: {}", n, a, b);
                break;
            }
        }
        assert_eq!(input.len(), decompressed.len());
        assert!(&decompressed == &input);
    }

    /// Check that lazy matching is working as intended
    #[test]
    fn lazy() {
        // We want to match on `badger` rather than `nba` as it is longer
        // let data = b" nba nbadg badger nbadger";
        let data = b"nba badger nbadger";
        let compressed = lz77_compress(data).unwrap();
        let test = compressed[compressed.len() - 2];
        if let LDPair::Length(n) = test.value() {
            assert_eq!(n, 6);
        } else {
            print_output(&compressed);
            panic!();
        }
    }

    fn roundtrip(data: &[u8]) {
        let compressed = super::lz77_compress(&data).unwrap();
        let decompressed = decompress_lz77(&compressed);
        assert!(decompressed == data);
    }

    // Check that data with the exact window size is working properly
    #[test]
    #[allow(unused)]
    fn exact_window_size() {
        use std::io::Write;
        let mut data = vec![0; WINDOW_SIZE];
        roundtrip(&data);
        {
            data.write(&[22; WINDOW_SIZE]);
        }
        roundtrip(&data);
        {
            data.write(&[55; WINDOW_SIZE]);
        }
        roundtrip(&data);
    }

    /// Test that matches at the window border are working correctly
    #[test]
    fn border() {
        use chained_hash_table::WINDOW_SIZE;
        let mut data = vec![35; WINDOW_SIZE];
        data.extend(b"Test");
        let compressed = super::lz77_compress(&data).unwrap();
        assert!(compressed.len() < data.len());
        let decompressed = decompress_lz77(&compressed);
        // print_output(&compressed);
        assert_eq!(decompressed.len(), data.len());
        assert!(decompressed == data);
    }

    #[test]
    fn border_multiple_blocks() {
        use chained_hash_table::WINDOW_SIZE;
        let mut data = vec![0; (WINDOW_SIZE * 2) + 50];
        data.push(1);
        let compressed = super::lz77_compress(&data).unwrap();
        assert!(compressed.len() < data.len());
        let decompressed = decompress_lz77(&compressed);
        assert!(decompressed == data);
    }

    #[test]
    fn compress_block_status() {
        use input_buffer::InputBuffer;
        use output_writer::FixedWriter;

        let data = b"Test data data";
        let mut writer = FixedWriter::new();

        let mut buffer = InputBuffer::empty();
        let mut state = LZ77State::new(4096, DEFAULT_LAZY_IF_LESS_THAN, MatchingType::Lazy);
        let status = lz77_compress_block_finish(data, &mut state, &mut buffer, &mut writer);
        assert_eq!(status.1, LZ77Status::Finished);
        assert!(&buffer.get_buffer()[..data.len()] == data);
        assert_eq!(buffer.current_end(), data.len());
    }

    #[test]
    fn compress_block_multiple_windows() {
        use input_buffer::InputBuffer;
        use output_writer::{OutputWriter, FixedWriter};

        let data = get_test_data();
        assert!(data.len() > (WINDOW_SIZE * 2) + super::MAX_MATCH);
        let mut writer = FixedWriter::new();

        let mut buffer = InputBuffer::empty();
        let mut state = LZ77State::new(0, DEFAULT_LAZY_IF_LESS_THAN, MatchingType::Lazy);
        let (bytes_consumed, status) =
            lz77_compress_block_finish(&data, &mut state, &mut buffer, &mut writer);
        assert_eq!(buffer.get_buffer().len(),
                   (WINDOW_SIZE * 2) + super::MAX_MATCH);
        assert_eq!(status, LZ77Status::EndBlock);
        let buf_len = buffer.get_buffer().len();
        assert!(buffer.get_buffer()[..] == data[..buf_len]);
        // print_output(writer.get_buffer());
        writer.clear_buffer();
        let (_, status) = lz77_compress_block_finish(&data[bytes_consumed..],
                                                     &mut state,
                                                     &mut buffer,
                                                     &mut writer);
        assert_eq!(status, LZ77Status::EndBlock);
        // print_output(writer.get_buffer());
    }

    #[test]
    fn multiple_inputs() {
        use output_writer::OutputWriter;
        let data = b"Badger badger bababa test data 25 asfgestghresjkgh";
        let comp1 = lz77_compress(data).unwrap();
        let comp2 = {
            const SPLIT: usize = 25;
            let first_part = &data[..SPLIT];
            let second_part = &data[SPLIT..];
            let mut state = TestStruct::new();
            let (bytes_written, status) = state.compress_block(first_part, false);
            assert_eq!(bytes_written, first_part.len());
            assert_eq!(status, LZ77Status::NeedInput);
            let (bytes_written, status) = state.compress_block(second_part, true);
            assert_eq!(bytes_written, second_part.len());
            assert_eq!(status, LZ77Status::Finished);
            Vec::from(state.writer.get_buffer())
        };
        assert!(comp1 == comp2);
    }
}
