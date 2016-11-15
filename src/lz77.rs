use std::cmp;

use input_buffer::InputBuffer;
use matching::longest_match;
use lzvalue::LZValue;
use huffman_table;
use chained_hash_table::ChainedHashTable;
use compression_options::HIGH_MAX_HASH_CHECKS;
use output_writer::{OutputWriter, FixedWriter};

const MAX_MATCH: usize = huffman_table::MAX_MATCH as usize;
const MIN_MATCH: usize = huffman_table::MIN_MATCH as usize;

/// A struct that contains the hash table, and keeps track of where we are in the input data
pub struct LZ77State {
    hash_table: ChainedHashTable,
    // The current position in the input slice
    pub current_start: usize,
    // True if this is the first window
    is_first_window: bool,
    // True if the last block has been output
    is_last_block: bool,
    // How many bytes the last match in the previous window extended into the current one
    overlap: usize,
    // The maximum number of hash entries to search
    max_hash_checks: u16,
}

impl LZ77State {
    fn from_starting_values(b0: u8, b1: u8, max_hash_checks: u16) -> LZ77State {
        LZ77State {
            hash_table: ChainedHashTable::from_starting_values(b0, b1),
            current_start: 0,
            is_first_window: true,
            is_last_block: false,
            overlap: 0,
            max_hash_checks: max_hash_checks,
        }
    }

    /// Creates a new LZ77 state, adding the first to bytes to the hash table
    /// to warm it up
    pub fn new(data: &[u8], max_hash_checks: u16) -> LZ77State {
        LZ77State::from_starting_values(data[0], data[1], max_hash_checks)
    }

    pub fn set_last(&mut self) {
        self.is_last_block = true;
    }

    pub fn is_last_block(&self) -> bool {
        self.is_last_block
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
                                  start: usize,
                                  end: usize,
                                  hash_table: &mut ChainedHashTable,
                                  writer: &mut W,
                                  max_hash_checks: u16)
                                  -> usize {
    let end = cmp::min(data.len(), end);
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

    let mut prev_byte = 0u8;
    let mut prev_length = NO_LENGTH;
    let mut prev_distance = 0;
    let mut add = false;
    let mut overlap = 0;

    // Iterate through the slice, adding literals or length/distance pairs
    while let Some((n, &b)) = insert_it.next() {
        if let Some(&hash_byte) = hash_it.next() {
            let position = n + start;
            hash_table.add_hash_value(position, hash_byte);
            // rolling_checksum.update(hash_byte);

            let (match_len, match_dist) =
                longest_match(data, hash_table, position, prev_length, max_hash_checks);

            if prev_length >= match_len && prev_length >= MIN_MATCH as usize {
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
                        // rolling_checksum.update(i_hash_byte);
                        hash_table.add_hash_value(ipos + start, i_hash_byte);
                    }
                }

                if position + prev_length > end {
                    // We need to subtract 1 since the byte at pos is also included
                    overlap = position + prev_length - end - 1;
                };

                add = false;

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

#[derive(Eq, PartialEq, Clone, Copy, Debug)]
pub enum LZ77Status {
    NeedInput,
    EndBlock,
    Finished,
}

/// Compress a slice
/// Will return err on failure eventually, but for now allways succeeds or panics
pub fn lz77_compress_block<W: OutputWriter>(data: &[u8],
                                            state: &mut LZ77State,
                                            buffer: &mut InputBuffer,
                                            mut writer: &mut W,
                                            finish: bool)
                                            -> (usize, LZ77Status) {
    // Currently we use window size as block length, in the future we might want to allow
    // differently sized blocks
    let window_size = DEFAULT_WINDOW_SIZE;

    let mut status = LZ77Status::EndBlock;
    let mut remaining_data = buffer.add_data(data);

    while writer.buffer_length() < (window_size * 2) {
        if state.is_first_window {
            if buffer.current_end() >= window_size + MAX_MATCH || finish {


                let first_chunk_end = if finish && remaining_data.is_none() {
                    // If we are finishing, make sure we include data in the lookahead area
                    buffer.current_end()
                } else {
                    cmp::min(window_size, buffer.current_end())
                };

                state.overlap = process_chunk::<W>(buffer.get_buffer(),
                                                   0,
                                                   first_chunk_end,
                                                   &mut state.hash_table,
                                                   &mut writer,
                                                   state.max_hash_checks);

                // We are at the first block so we don't need to slide the hash table
                state.current_start += first_chunk_end;

                if first_chunk_end >= data.len() {
                    state.set_last();
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
        } else {
            if buffer.current_end() >= (window_size * 2) + MAX_MATCH || finish {
                let start = window_size + state.overlap;

                // Determine where we have to stop iterating to slide the buffer and hash,
                // or stop because we are at the end of the input data.
                let end = if remaining_data.is_none() && finish {
                    // If we are finishing, make sure we include the lookahead data
                    buffer.current_end()
                } else {
                    cmp::min(window_size * 2, buffer.current_end())
                };
                // Limit the length of the input buffer slice so we don't go off the end
                // and read garbage data when checking match lengths.
                // let buffer_end = cmp::min(window_size * 2 + MAX_MATCH, slice.len());

                state.overlap = process_chunk::<W>(&buffer.get_buffer(), // [..buffer_end]
                                                   start,
                                                   end,
                                                   &mut state.hash_table,
                                                   &mut writer,
                                                   state.max_hash_checks);
                if remaining_data.is_none() {
                    // We stopped before or at the window size, so we are at the end.
                    state.set_last();
                    status = LZ77Status::Finished;
                    break;
                } else {
                    // We are not at the end, so slide and continue
                    state.current_start += end - start + state.overlap;
                    // let start = state.current_start;
                    // We slide the hash table back to make space for new hash values
                    // We only need to remember 32k bytes back (the maximum distance allowed by the
                    // deflate spec)
                    state.hash_table.slide(window_size);
                    // let end = cmp::min(start + window_size + MAX_MATCH, data.len());
                    remaining_data = buffer.slide(// &data[start..end]
                                                  remaining_data.unwrap());

                    status = LZ77Status::EndBlock;
                }
            } else {
                status = LZ77Status::NeedInput;
                break;
            }
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
    fn new(data: &[u8]) -> TestStruct {
        TestStruct {
            state: LZ77State::new(data, HIGH_MAX_HASH_CHECKS),
            buffer: InputBuffer::empty(),
            writer: FixedWriter::new(),
        }
    }

    fn compress_block(&mut self, data: &[u8], flush: bool) -> (usize, LZ77Status) {
        lz77_compress_block(data,
                            &mut self.state,
                            &mut self.buffer,
                            &mut self.writer,
                            flush)
    }
}

/// Compress a slice, not storing frequency information
///
/// This is a convenience function for compression with fixed huffman values
/// Only used in tests for now
#[allow(dead_code)]
pub fn lz77_compress(data: &[u8]) -> Option<Vec<LZValue>> {
    let mut test_boxed = Box::new(TestStruct::new(data));
    let mut out = Vec::<LZValue>::with_capacity(data.len() / 3);
    {
        let mut test = test_boxed.as_mut();
        let mut slice = data;

        while !test.state.is_last_block {
            let bytes_written = lz77_compress_block(&slice,
                                                    &mut test.state,
                                                    &mut test.buffer,
                                                    &mut test.writer,
                                                    true)
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
        let mut state = LZ77State::new(data, 4096);
        let status = lz77_compress_block(data, &mut state, &mut buffer, &mut writer, true);
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
        let mut state = LZ77State::new(&data, 0);
        let (bytes_consumed, status) =
            lz77_compress_block(&data, &mut state, &mut buffer, &mut writer, true);
        assert_eq!(buffer.get_buffer().len(),
                   (WINDOW_SIZE * 2) + super::MAX_MATCH);
        assert_eq!(status, LZ77Status::EndBlock);
        let buf_len = buffer.get_buffer().len();
        assert!(buffer.get_buffer()[..] == data[..buf_len]);
        // print_output(writer.get_buffer());
        writer.clear_buffer();
        let (_, status) = lz77_compress_block(&data[bytes_consumed..],
                                              &mut state,
                                              &mut buffer,
                                              &mut writer,
                                              true);
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
            let mut state = TestStruct::new(first_part);
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
