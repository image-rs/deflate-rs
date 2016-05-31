use std::cmp;

use huffman_table::{MAX_MATCH, MIN_MATCH};
use chained_hash_table::{WINDOW_SIZE, ChainedHashTable};

/// A structure representing values in a compressed stream of data before being huffman coded
/// We might want to represent this differently eventually to save on memory usage
#[derive(Debug)]
pub enum LDPair {
    Literal(u8),
    LengthDistance {
        length: u16,
        distance: u16,
    },
}

/// Get the length of the checked match (assuming the two bytes preceeding current_pos match)
/// The function returns how many bytes after and including current_pos match + 2
fn get_match_length(data: &[u8], current_pos: usize, pos_to_check: usize) -> u16 {
    // TODO: This can be optimised by checking multiple bytes at once and not checking the
    // first 3 bytes since we already know they match
    data[current_pos..]
        .iter()
        .zip(data[pos_to_check..].iter())
        .enumerate()
        .take_while(|&(n, (&a, &b))| n < MAX_MATCH as usize && a == b)
        .count() as u16// + 2
}

fn longest_match(data: &[u8], hash_table: &ChainedHashTable, position: usize) -> (u16, u16) {
    if data.len() - position < MIN_MATCH as usize {
        return (0, 0);
    }

    let limit = cmp::min(position, WINDOW_SIZE as usize);

    let mut current_head = hash_table.current_head();
    let mut current_prev = hash_table.get_prev(current_head as usize);

    // assert!(current_head as usize == test_position);

    let mut best_length = MIN_MATCH - 1;
    let mut best_distance = 0;

    while current_head >= 2 && current_head != current_prev {
        let distance = position - current_head as usize;
        if distance as usize > limit {
            break;
        }

        if distance > 0 {
            let length = get_match_length(data, position - 2, current_head as usize - 2);

            if length < MIN_MATCH {
                break;
            }

            if length > best_length {
                best_length = length;
                best_distance = distance;
            }
        }
        // {
        //
        // println!("Distance: {}, Limit: {}, current_head: {}, current_prev: {}, best_length: \
        // {}, position {}",
        // distance,
        // limit,
        // current_head,
        // current_prev,
        // best_length,
        // position);
        //
        // }
        current_head = current_prev;
        current_prev = hash_table.get_prev(current_head as usize);
    }
    (best_length - 2, best_distance as u16)
}

fn longest_match_current(data: &[u8], hash_table: &ChainedHashTable) -> (u16, u16) {
    longest_match(data, hash_table, hash_table.current_position())
}

const DEFAULT_WINDOW_SIZE: usize = 32768;

fn process_chunk(data: &[u8],
                 start: usize,
                 end: usize,
                 hash_table: &mut ChainedHashTable,
                 output: &mut Vec<LDPair>) {
    let mut data_iterator = data[start..end].iter().enumerate();

    println!("Starting chunk!");
    while let Some((n, b)) = data_iterator.by_ref().next() {
        let position = n + start;
        hash_table.add_hash_value(position, *b);
        let (match_len, match_dist) = longest_match_current(data, hash_table);
        if match_len >= MIN_MATCH {
            output.push(LDPair::LengthDistance {
                length: match_len,
                distance: match_dist,
            });
            let taker = data_iterator.by_ref().take(match_len as usize - 1);
            for (ipos, ibyte) in taker {
                // println!("ipos: {}", ipos + 2);
                hash_table.add_hash_value(ipos + start, *ibyte);
            }
        } else {
            output.push(LDPair::Literal(*b));
        }
    }
    println!("Finishing chunk");
}

/// Compress a slice
/// Returns a vector of `LDPair` values on success
pub fn lz77_compress(data: &[u8], window_size: usize) -> Option<Vec<LDPair>> {
    if window_size != DEFAULT_WINDOW_SIZE {
        return None;
    }
    // if data.len() > DEFAULT_WINDOW_SIZE {
    // panic!("Compressing data longer than {} bytes not properly implemented yet!",
    // DEFAULT_WINDOW_SIZE);
    // }

    let mut output = Vec::new();

    let mut hash_table = ChainedHashTable::from_starting_values(data[0], data[1]);
    output.push(LDPair::Literal(data[0]));
    output.push(LDPair::Literal(data[1]));

    let first_chunk_end = cmp::min(window_size, data.len());// - 1;

    process_chunk(data, 2, first_chunk_end, &mut hash_table, &mut output);
    println!("First bit done!");
    let mut current_start = window_size;
    if data.len() > window_size {
        loop {
            let start = current_start;
            let end = cmp::min(current_start + window_size, data.len());
            process_chunk(&data[start - window_size..],
                          start,
                          end,
                          &mut hash_table,
                          &mut output);
            if end >= data.len() {
                break;
            }
            current_start += window_size;
            hash_table.slide(window_size);
        }
    }

    // println!("{}", String::from_utf8(hash_table.data).unwrap());

    Some(output)
}

#[cfg(test)]
mod test {

    fn decompress_lz77(input: &[super::LDPair]) -> Vec<u8> {
        let mut output = Vec::new();
        for p in input {
            match *p {
                super::LDPair::Literal(l) => output.push(l),
                super::LDPair::LengthDistance { distance: d, length: l } => {
                    let start = output.len() - d as usize;
                    //                    let it = output[start..].iter().enumerate().by_ref();
                    let mut n = 0;
                    while n < l as usize {
                        let b = output[start + n];
                        output.push(b);
                        n += 1;
                    }
                }
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

    #[test]
    fn test_longest_match() {
        use chained_hash_table::{filled_hash_table, HASH_BYTES};
        use std::str::from_utf8;

        let test_data = b"xTest data, Test_data,zTest data";
        let hash_table = filled_hash_table(&test_data[..22 + 1 + HASH_BYTES]);

        println!("Bytes: {}",
                 from_utf8(&test_data[..22 + 1 + HASH_BYTES]).unwrap());
        let (length, distance) = super::longest_match_current(test_data, &hash_table);
        println!("Distance: {}", distance);
        // We check that we get the longest match, rather than the shorter, but closer one.
        assert_eq!(distance, 22);
        assert_eq!(length, 9);
        let test_arr2 = [10u8, 10, 10, 10, 10, 10, 10, 10, 2, 3, 5, 10, 10, 10, 10, 10];
        let hash_table = filled_hash_table(&test_arr2[..HASH_BYTES + 1 + 1]);
        let (length, distance) = super::longest_match_current(&test_arr2, &hash_table);
        println!("Distance: {}, length: {}", distance, length);
        assert_eq!(distance, 1);
        assert_eq!(length, 6);
    }

    fn print_output(input: &[super::LDPair]) {
        let mut output = vec![];
        for l in input {
            match *l {
                super::LDPair::Literal(l) => output.push(l),
                super::LDPair::LengthDistance { distance: d, length: l } => {
                    output.extend(format!("<Distance: {}, Length: {}>", d, l).into_bytes())
                }
            }
        }

        println!("{}", String::from_utf8(output).unwrap());
    }

    #[test]
    fn test_lz77_short() {
        use std::str;
        use chained_hash_table::WINDOW_SIZE;
//        let test_bytes = String::from("Test data, test data, test dota, data test, test data.")
//            .into_bytes();
        let test_bytes = String::from("                    GNU GENERAL PUBLIC LICENSE
                       Version 3, 29 June 2007

 Copyright (C) 2007 Free Software Foundation, Inc. <http://fsf.org/>
 Everyone is permitted to copy and distribute verbatim copies
 of this license document, but changing it is not allowed.

                            Preamble

  The GNU General Public License is a free, copyleft license for
software and other kinds of works.

  The licenses for most software and other practical works are designed
to take away your freedom to share and change the works.  By contrast,
the GNU General Public License is intended to guarantee your freedom to
share and change all versions of a program--to make sure it remains free
software for all its users.")
            .into_bytes();
        let res = super::lz77_compress(&test_bytes, WINDOW_SIZE).unwrap();
        // println!("{:?}", res);
        // TODO: Check that compression is correct
        print_output(&res);
        let decompressed = decompress_lz77(&res);
        let d_str = str::from_utf8(&decompressed).unwrap();
        println!("{}", d_str);
        assert_eq!(test_bytes, decompressed);
    }

    #[test]
    fn test_lz77_long() {
        use std::fs::File;
        use std::io::Read;
        use std::str;
        use chained_hash_table::WINDOW_SIZE;
        let mut input = Vec::new();

        let mut f = File::open("src/gpl-3.0.txt").unwrap();
        f.read_to_end(&mut input).unwrap();
        let compressed = super::lz77_compress(&input[..WINDOW_SIZE], WINDOW_SIZE).unwrap();
        let decompressed = decompress_lz77(&compressed);
        assert!(decompressed == &input[0..WINDOW_SIZE]);
    }
}
