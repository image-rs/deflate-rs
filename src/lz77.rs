use std::cmp;

use huffman_table::{MAX_MATCH, MIN_MATCH};
use chained_hash_table::{WINDOW_SIZE, ChainedHashTable};

/// A structure representing values in a compressed stream of data before being huffman coded
/// We might want to represent this differently eventually to save on memory usage
#[derive(Debug, PartialEq, Eq)]
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
        .count() as u16
}

/// Try finding the position and length of the longest match in the input data.
fn longest_match(data: &[u8], hash_table: &ChainedHashTable, position: usize) -> (u16, u16) {
    if position == 0 {
        return (0, 0);
    }

    let limit = if position > WINDOW_SIZE {
        position - WINDOW_SIZE
    } else {
        0
    };

    let max_length = cmp::min((data.len() - position) as u16, MAX_MATCH);

    let mut current_head = hash_table.get_prev(hash_table.current_head() as usize);
    let starting_head = current_head;

    let mut best_length = MIN_MATCH - 1;
    let mut best_distance = 0;

    loop {
        if (current_head as usize) < limit {
            break;
        }


        if current_head == 0 {
            break;
        }

        let distance = position - current_head as usize;

        if distance > 0 {
            let length = get_match_length(data, position, current_head as usize);

            if length < MIN_MATCH {
                // If the length is < than MIN_MATCH we are probably at the end of
                // the chain.
                // Zlib continues rather than stops at this point, so it might be possible
                // that there are further matches after this, however continuing here
                // will currently generate an infinate loop, and adding a loop limit makes
                // things very slow, so we break for now (which in the worst case would mean
                // less efficient compression).
                // TODO: This may not be needed anymore
                break;
            }

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
        current_head = hash_table.get_prev(current_head as usize);
        if current_head == starting_head {
            // We've gone through one cycle.
            break;
        }
    }
    (best_length, best_distance as u16)
}

/// Get the longest match from the current position of the hash table
fn longest_match_current(data: &[u8], hash_table: &ChainedHashTable) -> (u16, u16) {
    longest_match(data, hash_table, hash_table.current_position())
}

const DEFAULT_WINDOW_SIZE: usize = 32768;

fn process_chunk(data: &[u8],
                 start: usize,
                 end: usize,
                 hash_table: &mut ChainedHashTable,
    output: &mut Vec<LDPair>) {
let end = cmp::min(data.len(), end);
    let current_chunk = &data[start..end];
    let mut insert_it = current_chunk.iter().enumerate();
    let mut hash_it = current_chunk[2..].iter();

    // Iterate through the slice, adding literals or length/distance pairs
    while let Some((n, b)) = insert_it.next() {
        if let Some(hash_byte) = hash_it.next() {
            let position = n + start;
            hash_table.add_hash_value(position, *hash_byte);
            // TODO: Currently, we only check for matches up to the end of the chunk, but ideally
            // we should be checking max_match bytes further to achieve the best possible compression.
            let (match_len, match_dist) = longest_match_current(&data[..end], hash_table);
            if match_len >= MIN_MATCH {
                // TODO: Add heuristic checking if outputting a length/distance pair will actually be shorter than adding the literal bytes

                output.push(LDPair::LengthDistance {
                    length: match_len,
                    distance: match_dist,
                });
                let taker = insert_it.by_ref().take(match_len as usize - 1);
                let mut hash_taker = hash_it.by_ref().take(match_len as usize - 1);

                for (ipos, _) in taker {
                    if let Some(i_hash_byte) = hash_taker.next() {

                        hash_table.add_hash_value(ipos + start, *i_hash_byte);
                    }
                }
            } else {
                output.push(LDPair::Literal(*b));
            }
        } else {
            output.push(LDPair::Literal(*b));
        }
    }

}

/// Compress a slice
/// Returns a vector of `LDPair` values on success
pub fn lz77_compress(data: &[u8], window_size: usize) -> Option<Vec<LDPair>> {
    if window_size != DEFAULT_WINDOW_SIZE {
        // This different window sizes are not supported for now.
        return None;
    }

    let mut output = Vec::new();
    // Reserve some extra space in the output vector to prevent excessive allocation
    output.reserve(data.len() / 2);

    let mut hash_table = ChainedHashTable::from_starting_values(data[0], data[1]);

    let first_chunk_end = cmp::min(window_size, data.len());

    process_chunk(data, 0, first_chunk_end, &mut hash_table, &mut output);

    let mut current_start = window_size;
    if data.len() > window_size {
        loop {
            let start = current_start;
            let slice = &data[start - window_size..];
            let end = cmp::min(window_size * 2, slice.len());
            process_chunk(slice,
                          window_size,
                          end,
                          &mut hash_table,
                          &mut output);
            if end >= slice.len() {
                break;
            }
            current_start += window_size;
            hash_table.slide(window_size);
        }
    }

    output.shrink_to_fit();

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

    /// Test that a short string from an example on SO compresses correctly
    #[test]
    fn test_lz77_short() {
        use std::str;
        use chained_hash_table::WINDOW_SIZE;

        let test_bytes = String::from("Deflate late").into_bytes();
        let res = super::lz77_compress(&test_bytes, WINDOW_SIZE).unwrap();
        // println!("{:?}", res);
        // TODO: Check that compression is correct
        print_output(&res);
        let decompressed = decompress_lz77(&res);
        let d_str = str::from_utf8(&decompressed).unwrap();
        println!("{}", d_str);
        assert_eq!(test_bytes, decompressed);
        assert_eq!(res[8],
                   super::LDPair::LengthDistance {
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
        use chained_hash_table::WINDOW_SIZE;
        let mut input = Vec::new();

        let mut f = File::open("src/pg11.txt").unwrap();
        f.read_to_end(&mut input).unwrap();
        let compressed = super::lz77_compress(&input, WINDOW_SIZE).unwrap();
        assert!(compressed.len() < input.len());
        let decompressed = decompress_lz77(&compressed);
        //println!("{}", str::from_utf8(&decompressed).unwrap());
        assert_eq!(input.len(), decompressed.len());
        assert!(decompressed == input);
    }

    /// Test that matches at the window border are working correctly
    #[test]
    fn test_lz77_border() {
        use chained_hash_table::WINDOW_SIZE;
        let data = vec![0; 34000];
        let compressed = super::lz77_compress(&data, WINDOW_SIZE).unwrap();
        assert!(compressed.len() < data.len());
        let decompressed = decompress_lz77(&compressed);
        assert!(decompressed == data);
    }
}
