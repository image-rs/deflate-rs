//! This module contains functionality for doing lz77 compression of data.
use std::cmp;
use std::ops::Range;
use std::iter::{Iterator, Enumerate};
use std::slice::Iter;

use input_buffer::InputBuffer;
use matching::longest_match;
use lzvalue::{LZValue, LZType};
use huffman_table;
use chained_hash_table::ChainedHashTable;
use compression_options::{HIGH_MAX_HASH_CHECKS, HIGH_LAZY_IF_LESS_THAN};
use output_writer::{OutputWriter, FixedWriter, BufferStatus};
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
    /// How many bytes of input the current block contains.
    current_block_input_bytes: u64,
    /// The maximum number of hash entries to search.
    max_hash_checks: u16,
    /// Only lazy match if we have a match length less than this.
    lazy_if_less_than: u16,
    /// Whether to use greedy or lazy parsing
    matching_type: MatchingType,
}

impl LZ77State {
    /// Creates a new LZ77 state
    pub fn new(max_hash_checks: u16,
               lazy_if_less_than: u16,
               matching_type: MatchingType)
               -> LZ77State {
        LZ77State {
            hash_table: ChainedHashTable::new(),
            is_first_window: true,
            is_last_block: false,
            overlap: 0,
            current_block_input_bytes: 0,
            max_hash_checks: max_hash_checks,
            lazy_if_less_than: lazy_if_less_than,
            matching_type: matching_type,
        }
    }

    /// Resets the state excluding max_hash_checks and lazy_if_less_than
    pub fn reset(&mut self) {
        self.hash_table.reset();
        self.is_first_window = true;
        self.is_last_block = false;
        self.overlap = 0;
        self.current_block_input_bytes = 0;
    }

    pub fn set_last(&mut self) {
        self.is_last_block = true;
    }

    /// Is this the last block we are outputting?
    pub fn is_last_block(&self) -> bool {
        self.is_last_block
    }

    /// How many bytes of input the current block contains.
    pub fn current_block_input_bytes(&self) -> u64 {
        self.current_block_input_bytes
    }

    pub fn reset_input_bytes(&mut self) {
        self.current_block_input_bytes = 0;
    }
}

const DEFAULT_WINDOW_SIZE: usize = 32768;

#[derive(Debug)]
enum ProcessStatus {
    Ok,
    BufferFull(usize),
}

fn process_chunk<W: OutputWriter>(data: &[u8],
                                  iterated_data: &Range<usize>,
                                  hash_table: &mut ChainedHashTable,
                                  writer: &mut W,
                                  max_hash_checks: u16,
                                  lazy_if_less_than: usize,
                                  matching_type: MatchingType)
                                  -> (usize, ProcessStatus) {
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

/// Add the specified number of bytes to the hash table from the iterators
/// adding `start` to the position supplied to the hash table.
fn add_to_hash_table(bytes_to_add: usize,
                     start: usize,
                     insert_it: &mut Enumerate<Iter<u8>>,
                     hash_it: &mut Iter<u8>,
                     hash_table: &mut ChainedHashTable) {
    let taker = insert_it.by_ref().take(bytes_to_add);
    let mut hash_taker = hash_it.by_ref().take(bytes_to_add);

    // Advance the iterators and add the bytes we jump over to the hash table and
    // checksum
    for (ipos, _) in taker {
        if let Some(&i_hash_byte) = hash_taker.next() {
            hash_table.add_hash_value(ipos + start, i_hash_byte);
        }
    }

}

// Write the specified literal `byte` to the writer `w`, and return
// `ProcessStatus::BufferFull($pos)` if the buffer is full after writing.
macro_rules! write_literal{
    ($w:ident, $byte:ident, $pos:expr) => {
        let b_status = $w.write_literal($byte);

        if let BufferStatus::Full = b_status {
            return (0, ProcessStatus::BufferFull($pos));
        }
    };
}

#[inline]
fn match_too_far(match_len: usize, match_dist: usize) -> bool {
    const TOO_FAR: usize = 8 * 1024;
    match_len == MIN_MATCH && match_dist > TOO_FAR
}

fn process_chunk_lazy<W: OutputWriter>(data: &[u8],
                                       iterated_data: &Range<usize>,
                                       mut hash_table: &mut ChainedHashTable,
                                       writer: &mut W,
                                       max_hash_checks: u16,
                                       lazy_if_less_than: usize)
                                       -> (usize, ProcessStatus) {
    // If this is less than 3 it prevents a check from working properly
    // TODO: This is a workaround, might want to return 0 from longest match on fail instead.
    let lazy_if_less_than = {
        if lazy_if_less_than < 3 {
            3
        } else {
            lazy_if_less_than
        }
    };
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
                let (mut match_len, mut match_dist) = {
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

                // If the match is only 3 bytes long and very far back, it's probably not worth
                // outputting.
                if match_too_far(match_len, match_dist) {
                    match_len = NO_LENGTH;
                    match_dist = 0;
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

            if prev_length >= match_len && prev_length > NO_LENGTH && prev_distance > 0 {
                // The previous match was better so we add it.
                // Casting note: length and distance is already bounded by the longest match
                // function. Usize is just used for convenience.
                let b_status =
                    writer.write_length_distance(prev_length as u16, prev_distance as u16);

                // We add the bytes to the hash table and checksum.
                // Since we've already added two of them, we need to add two less than
                // the length.
                let bytes_to_add = prev_length - 2;
                add_to_hash_table(bytes_to_add,
                                  start,
                                  &mut insert_it,
                                  &mut hash_it,
                                  &mut hash_table);

                // If the match is longer than the current window, we have note how many
                // bytes we overlap, since we don't need to do any matching on these bytes
                // in the next call of this function.
                if position + prev_length > end {
                    // We need to subtract 1 since the byte at pos is also included.
                    overlap = position + prev_length - end - 1;
                };

                if let BufferStatus::Full = b_status {
                    // MATCH(lazy)
                    return (overlap, ProcessStatus::BufferFull(position + prev_length - 1));
                }

                add = false;
                ignore_next = false;

            } else if add {
                // We found a better match (or there was no previous match)
                // so output the previous byte.
                // BETTER OR NO MATCH
                write_literal!(writer, prev_byte, position);
            } else {
                add = true
            }

            prev_length = match_len;
            prev_distance = match_dist;
            prev_byte = b;
        } else {
            let position = n + start;

            // If there is a match at this point, it will not have been added, so we need to add it.
            if prev_length > NO_LENGTH && prev_distance != 0 {
                let b_status =
                    writer.write_length_distance(prev_length as u16, prev_distance as u16);
                // As this will be a 3-length match at the end of the input data, there can't be any
                // overlap.
                // TODO: Not sure if we need to signal that the buffer is full here.
                // It's only needed in the case of syncing.
                if let BufferStatus::Full = b_status {
                    return (0, ProcessStatus::BufferFull(end));
                } else {
                    return (0, ProcessStatus::Ok);
                }
            };

            if add {
                // We may still have a leftover byte at this point, so we add it here if needed.
                add = false;

                // ADD
                write_literal!(writer, prev_byte, position);

            };

            // We are at the last two bytes we want to add, so there is no point
            // searching for matches here.

            // AFTER ADD
            write_literal!(writer, b, position + 1);
        }
    }
    if add {
        // We may still have a leftover byte at this point, so we add it here if needed.
        // END
        write_literal!(writer, prev_byte, end);
    }
    (overlap, ProcessStatus::Ok)
}

fn process_chunk_greedy<W: OutputWriter>(data: &[u8],
                                         iterated_data: &Range<usize>,
                                         mut hash_table: &mut ChainedHashTable,
                                         writer: &mut W,
                                         max_hash_checks: u16)
                                         -> (usize, ProcessStatus) {
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

    // The number of bytes past end that was added due to finding a match that extends into
    // the lookahead window.
    let mut overlap = 0;

    // Iterate through the slice, adding literals or length/distance pairs.
    while let Some((n, &b)) = insert_it.next() {
        if let Some(&hash_byte) = hash_it.next() {
            let position = n + start;
            hash_table.add_hash_value(position, hash_byte);

            // TODO: This should be cleaned up a bit.
            let (match_len, match_dist) = {
                longest_match(data, hash_table, position, NO_LENGTH, max_hash_checks)
            };

            if match_len >= MIN_MATCH as usize && match_dist > 0 &&
               !match_too_far(match_len, match_dist) {
                // Casting note: length and distance is already bounded by the longest match
                // function. Usize is just used for convenience.
                let b_status = writer.write_length_distance(match_len as u16, match_dist as u16);

                // We add the bytes to the hash table and checksum.
                // Since we've already added one of them, we need to add one less than
                // the length.
                let bytes_to_add = match_len - 1;
                add_to_hash_table(bytes_to_add,
                                  start,
                                  &mut insert_it,
                                  &mut hash_it,
                                  &mut hash_table);

                // If the match is longer than the current window, we have note how many
                // bytes we overlap, since we don't need to do any matching on these bytes
                // in the next call of this function.
                if position + match_len > end {
                    // We need to subtract 1 since the byte at pos is also included.
                    overlap = position + match_len - end;
                };

                if let BufferStatus::Full = b_status {
                    // MATCH
                    return (overlap, ProcessStatus::BufferFull(position + match_len));
                }

            } else {
                // NO MATCH
                write_literal!(writer, b, position + 1);
            }
        } else {
            // We are at the last two bytes we want to add, so there is no point
            // searching for matches here.
            // END
            write_literal!(writer, b, n + start + 1);
        }
    }
    (overlap, ProcessStatus::Ok)
}


#[derive(Eq, PartialEq, Clone, Copy, Debug)]
pub enum LZ77Status {
    /// Waiting for more input before doing any processing
    NeedInput,
    /// The output buffer is full, so the current block needs to be ended so the
    /// buffer can be flushed.
    EndBlock,
    /// All pending data has been processed.
    Finished,
}

pub fn lz77_compress_block_finish<W: OutputWriter>(data: &[u8],
                                                   state: &mut LZ77State,
                                                   buffer: &mut InputBuffer,
                                                   mut writer: &mut W)
                                                   -> (usize, LZ77Status) {
    let (consumed, status, _) =
        lz77_compress_block::<W>(data, state, buffer, &mut writer, Flush::Finish);
    (consumed, status)
}

/// Compress a slice with lz77 compression.
///
/// This function processes one window at a time, and returns when there is no input left,
/// or it determines it's time to end a block.
///
/// Returns the number of bytes of the input that were consumed and a status describing
/// whether there is no input, it's time to finish, or it's time to end the block.
pub fn lz77_compress_block<W: OutputWriter>(data: &[u8],
                                            state: &mut LZ77State,
                                            buffer: &mut InputBuffer,
                                            mut writer: &mut W,
                                            flush: Flush)
                                            -> (usize, LZ77Status, usize) {
    // Currently we only support the maximum window size
    let window_size = DEFAULT_WINDOW_SIZE;

    // Indicates whether we should try to process all the data including the lookahead, or if we
    // should wait until we have at least one window size of data before doing anything.
    let finish = flush == Flush::Finish || flush == Flush::Sync;
    let sync = flush == Flush::Sync;

    let mut current_position = 0;

    // The current status of the encoding.
    let mut status = LZ77Status::EndBlock;
    // Add data to the input buffer and keep a reference to the slice of data not added yet.
    let mut remaining_data = buffer.add_data(data);

    loop {
        assert!(writer.buffer_length() <= (window_size * 2));
        if state.is_first_window {
            // Don't do anything until we are either flushing, or we have at least one window of
            // data.
            if buffer.current_end() >= (window_size * 2) + MAX_MATCH || finish {

                if buffer.get_buffer().len() > 2 {
                    let b = buffer.get_buffer();
                    // Warm up the hash with the two first values, so we can find  matches at
                    // index 0.
                    state.hash_table.add_initial_hash_values(b[0], b[1]);
                }

                let first_chunk_end = if finish && remaining_data.is_none() {
                    // If we are finishing, make sure we include data in the lookahead area.
                    buffer.current_end()
                } else {
                    cmp::min(window_size, buffer.current_end())
                };

                let start = state.overlap;

                let (overlap, p_status) = process_chunk::<W>(buffer.get_buffer(),
                                                             &(start..first_chunk_end),
                                                             &mut state.hash_table,
                                                             &mut writer,
                                                             state.max_hash_checks,
                                                             state.lazy_if_less_than as usize,
                                                             state.matching_type);

                state.overlap = overlap;
                state.current_block_input_bytes += (first_chunk_end - start + overlap) as u64;

                // If the buffer is full, we want to end the block.
                if let ProcessStatus::BufferFull(written) = p_status {
                    // The buffer being full in the first window can only really happen in tests
                    // where we've pre-filled the buffer as each literal will take up at most one
                    // space in the buffer pending any bugs.
                    state.overlap = if overlap > 0 { overlap } else { written };
                    status = LZ77Status::EndBlock;
                    current_position = written;
                    break;
                }

                // We are at the first window so we don't need to slide the hash table yet.
                // If finishing or syncing, we stop here.
                if first_chunk_end >= data.len() && finish {
                    if !sync {
                        state.set_last();
                    }
                    status = LZ77Status::Finished;
                    state.is_first_window = false;
                    break;
                }
                // Otherwise, continue.
                state.is_first_window = false;
            } else {
                status = LZ77Status::NeedInput;
                break;
            }
        } else if buffer.current_end() >= (window_size * 2) + MAX_MATCH || finish {
            // This isn't the first chunk, so we start reading at one window in in the
            // buffer plus any additional overlap from earlier.
            let start = window_size + state.overlap;

            // Determine where we have to stop iterating to slide the buffer and hash,
            // or stop because we are at the end of the input data.
            let end = if remaining_data.is_none() && finish {
                // If we are finishing, make sure we include the lookahead data.
                buffer.current_end()
            } else {
                // Otherwise we process at most one window size of data.
                cmp::min(window_size * 2, buffer.current_end())
            };

            let (overlap, p_status) = process_chunk::<W>(buffer.get_buffer(),
                                                         &(start..end),
                                                         &mut state.hash_table,
                                                         &mut writer,
                                                         state.max_hash_checks,
                                                         state.lazy_if_less_than as usize,
                                                         state.matching_type);

            state.current_block_input_bytes += (end - start + overlap) as u64;

            if let ProcessStatus::BufferFull(written) = p_status {
                // If the buffer is full, return and end the block.
                // If overlap is non-zero, the buffer was full after outputting the last byte,
                // otherwise we have to skip to the point in the buffer where we stopped in the
                // next call.
                state.overlap = if overlap > 0 {
                    overlap
                } else {
                    written - window_size
                };

                current_position = written;

                // Status is already EndBlock at this point.
                // status = LZ77Status::EndBlock;
                break;
            }

            // The buffer is not full, but we still need to note if there is any overlap into the
            // next window.
            state.overlap = overlap;

            if remaining_data.is_none() && finish {
                // We stopped before or at the window size, so we are at the end.
                if !sync {
                    // If we are finishing and not syncing, we simply indicate that we are done.
                    state.set_last();
                } else {
                    // For sync flushing we need to slide the buffer and the hash chains so that the
                    // next call to this function starts at the right place.

                    // There won't be any overlap, since when syncing, we process to the end of the.
                    // pending data.
                    state.overlap = 0;
                    let n = buffer.move_down();
                    state.hash_table.slide(n);
                }
                status = LZ77Status::Finished;
                break;
            } else {
                // We are not at the end, so slide and continue.
                // We slide the hash table back to make space for new hash values
                // We only need to remember 2^15 bytes back (the maximum distance allowed by the
                // deflate spec).
                state.hash_table.slide(window_size);

                // Also slide the buffer, discarding data we no longer need and adding new data.
                remaining_data = buffer.slide(remaining_data.unwrap_or(&[]));
            }
        } else {
            // The caller has not indicated that they want to finish or flush, and there is less
            // than a window + lookahead of new data, so we wait for more.
            status = LZ77Status::NeedInput;
            break;
        }

    }

    (data.len() - remaining_data.unwrap_or(&[]).len(), status, current_position)
}

#[cfg(test)]
pub fn decompress_lz77(input: &[LZValue]) -> Vec<u8> {
    decompress_lz77_with_backbuffer(input, &[])
}

pub fn decompress_lz77_with_backbuffer(input: &[LZValue], back_buffer: &[u8]) -> Vec<u8> {
    let mut output = Vec::new();
    for p in input {
        match p.value() {
            LZType::Literal(l) => output.push(l),
            LZType::StoredLengthDistance(l, d) => {
                // We found a match, so we have to get the data that the match refers to.
                let d = d as usize;
                let prev_output_len = output.len();
                // Get match data from the back buffer if the match extends that far.
                let consumed = if d > output.len() {
                    let into_back_buffer = d - output.len();

                    assert!(into_back_buffer <= back_buffer.len(),
                            "FATAL ERROR: Attempted to refer to a match in non-existing data!");
                    let start = back_buffer.len() - into_back_buffer;
                    let end = cmp::min(back_buffer.len(), start + l.actual_length() as usize);
                    output.extend_from_slice(&back_buffer[start..end]);

                    end - start
                } else {
                    0
                };

                // Get match data from the currently decompressed data.
                let start = prev_output_len.saturating_sub(d);
                let mut n = 0;
                while n < (l.actual_length() as usize).saturating_sub(consumed) {
                    let b = output[start + n];
                    output.push(b);
                    n += 1;
                }
            }
        }
    }
    output
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
        TestStruct::with_config(HIGH_MAX_HASH_CHECKS,
                                HIGH_LAZY_IF_LESS_THAN,
                                MatchingType::Lazy)
    }

    fn with_config(max_hash_checks: u16,
                   lazy_if_less_than: u16,
                   matching_type: MatchingType)
                   -> TestStruct {
        TestStruct {
            state: LZ77State::new(max_hash_checks, lazy_if_less_than, matching_type),
            buffer: InputBuffer::empty(),
            writer: FixedWriter::new(),
        }
    }

    fn compress_block(&mut self, data: &[u8], flush: bool) -> (usize, LZ77Status, usize) {
        lz77_compress_block(data,
                            &mut self.state,
                            &mut self.buffer,
                            &mut self.writer,
                            if flush { Flush::Finish } else { Flush::None })
    }
}

pub fn lz77_compress(data: &[u8]) -> Option<Vec<LZValue>> {
    lz77_compress_conf(data,
                       HIGH_MAX_HASH_CHECKS,
                       HIGH_LAZY_IF_LESS_THAN,
                       MatchingType::Lazy)
}

/// Compress a slice, not storing frequency information
///
/// This is a convenience function for compression with fixed huffman values
/// Only used in tests for now
#[allow(dead_code)]
pub fn lz77_compress_conf(data: &[u8],
                          max_hash_checks: u16,
                          lazy_if_less_than: u16,
                          matching_type: MatchingType)
                          -> Option<Vec<LZValue>> {
    let mut test_boxed =
        Box::new(TestStruct::with_config(max_hash_checks, lazy_if_less_than, matching_type));
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
    use lzvalue::{LZValue, LZType};
    use chained_hash_table::WINDOW_SIZE;
    use compression_options::{DEFAULT_MAX_HASH_CHECKS, DEFAULT_LAZY_IF_LESS_THAN};
    use test_utils::get_test_data;
    use output_writer::MAX_BUFFER_LENGTH;




    /// Helper function to print the output from the lz77 compression function
    fn print_output(input: &[LZValue]) {
        let mut output = vec![];
        for l in input {
            match l.value() {
                LZType::Literal(l) => output.push(l),
                LZType::StoredLengthDistance(l, d) => {
                    output.extend(format!("<L {}>", l.actual_length()).into_bytes());
                    output.extend(format!("<D {}>", d).into_bytes())
                }
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
        // TODO: Check that compression is correct

        let decompressed = decompress_lz77(&res);

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

        let decompressed = decompress_lz77(&compressed);

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
        let test = compressed[compressed.len() - 1];
        if let LZType::StoredLengthDistance(l, _) = test.value() {
            assert_eq!(l.actual_length(), 6);
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

        writer.clear_buffer();
        let (_, status) = lz77_compress_block_finish(&data[bytes_consumed..],
                                                     &mut state,
                                                     &mut buffer,
                                                     &mut writer);
        assert_eq!(status, LZ77Status::EndBlock);

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
            let (bytes_written, status, _) = state.compress_block(first_part, false);
            assert_eq!(bytes_written, first_part.len());
            assert_eq!(status, LZ77Status::NeedInput);
            let (bytes_written, status, _) = state.compress_block(second_part, true);
            assert_eq!(bytes_written, second_part.len());
            assert_eq!(status, LZ77Status::Finished);
            Vec::from(state.writer.get_buffer())
        };
        assert!(comp1 == comp2);
    }


    #[test]
    /// Test that the exit from process_chunk when buffer is full is working correctly.
    fn buffer_fill() {
        let data = get_test_data();
        // The comments above these calls refers the positions with the
        // corersponding comments in process_chunk_{greedy/lazy}.
        // POS BETTER OR NO MATCH
        buffer_test_literals(&data);
        // POS END
        // POS NO MATCH
        buffer_test_last_bytes(MatchingType::Greedy, &data);
        // POS ADD
        // POS AFTER ADD
        buffer_test_last_bytes(MatchingType::Lazy, &data);

        // POS MATCH
        buffer_test_match(MatchingType::Greedy);
        // POS MATCH(lazy)
        buffer_test_match(MatchingType::Lazy);

        // POS END
        buffer_test_add_end(&data);
    }

    // Test buffer fill when a byte is added due to no match being found.
    fn buffer_test_literals(data: &[u8]) {
        let mut state = TestStruct::with_config(0, 0, MatchingType::Lazy);
        let (bytes_consumed, status, _) = state.compress_block(&data, false);
        let total_consumed = bytes_consumed;
        assert_eq!(status, LZ77Status::EndBlock);
        assert!(bytes_consumed <= (WINDOW_SIZE * 2) + MAX_MATCH);

        // The buffer should be full.
        assert_eq!(state.writer.get_buffer().len(), u16::max_value() as usize);

        let mut out = decompress_lz77(state.writer.get_buffer());
        state.writer.clear_buffer();
        // The buffer should now be cleared.
        assert_eq!(state.writer.get_buffer().len(), 0);

        let (bytes_consumed, ..) = state.compress_block(&data[total_consumed..], false);
        // Now that the buffer has been cleared, we should have consumed more data.
        assert!(bytes_consumed > 0);
        // We should have some new data in the buffer at this point.
        assert!(state.writer.get_buffer().len() > 0);
        // total_consumed += bytes_consumed;
        out.extend_from_slice(&decompress_lz77(state.writer.get_buffer()));
        assert!(data[..out.len()] == out[..]);
    }

    // Test buffer fill at the last two bytes that are not hashed.
    fn buffer_test_last_bytes(matching_type: MatchingType, data: &[u8]) {
        const BYTES_USED: usize = MAX_BUFFER_LENGTH;
        assert!(&data[..BYTES_USED] ==
                &decompress_lz77(&lz77_compress_conf(&data[..BYTES_USED], 0, 0, matching_type)
            .unwrap())
                     [..]);
        assert!(&data[..BYTES_USED + 1] ==
                &decompress_lz77(&lz77_compress_conf(&data[..BYTES_USED + 1],
                                                     0,
                                                     0,
                                                     matching_type)
            .unwrap())
                     [..]);
    }

    // Test buffer fill when buffer is full at a match.
    fn buffer_test_match(matching_type: MatchingType) {
        let mut state = TestStruct::with_config(1, 0, matching_type);
        for _ in 0..MAX_BUFFER_LENGTH - 4 {
            assert!(state.writer.write_literal(0) == BufferStatus::NotFull);
        }
        state.compress_block(&[1, 2, 3, 1, 2, 3, 4], true);
        assert!(*state.writer.get_buffer().last().unwrap() == LZValue::length_distance(3, 3));

    }

    // Test buffer fill for the lazy match algorithm when adding a pending byte at the end.
    fn buffer_test_add_end(data: &[u8]) {
        let mut state = TestStruct::with_config(DEFAULT_MAX_HASH_CHECKS,
                                                DEFAULT_LAZY_IF_LESS_THAN,
                                                MatchingType::Lazy);
        // For the test file, this is how much data needs to be added to get the buffer
        // full at the right spot to test that this buffer full exit is workong correctly.
        for _ in 0..33583 {
            assert!(state.writer.write_literal(0) == BufferStatus::NotFull)
        }

        state.compress_block(data, false);

        let dec = decompress_lz77(&state.writer.get_buffer()[33583..]);
        assert!(dec.len() > 0);
        assert!(dec[..] == data[..dec.len()]);
    }

    fn lit(l: u8) -> LZValue {
        LZValue::literal(l)
    }

    fn ld(l: u16, d: u16) -> LZValue {
        LZValue::length_distance(l, d)
    }

    /// Check that decompressing lz77-data that refers to the back-buffer works.
    #[test]
    fn test_decompress_with_backbuffer() {
        let bb = vec![5; WINDOW_SIZE];
        let lz_compressed = [lit(2), lit(4), ld(4, 20), lit(1), lit(1), ld(5, 10)];
        let dec = decompress_lz77_with_backbuffer(&lz_compressed, &bb);

        // ------------l2 l4  <-ld4,20-> l1 l1  <---ld5,10-->
        assert!(dec == [2, 4, 5, 5, 5, 5, 1, 1, 5, 5, 2, 4, 5]);
    }

}
