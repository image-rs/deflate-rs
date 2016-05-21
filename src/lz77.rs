use std::cmp;

use huffman_table::{MAX_MATCH, MIN_MATCH};
use chained_hash_table::{WINDOW_SIZE, ChainedHashTable};

///A structure representing values in a compressed stream of data before being huffman coded
///We might want to represent this differently eventually to save on memory usage
#[derive(Debug)]
pub enum LDPair {
    Literal(u8),
    LengthDistance {
        length: u16,
        distance: u16,
    },
}

/// Get the length of the checked match
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

fn distance_from_chain(head: u16, prev: u16) -> u16 {
    if prev < head {
        head - prev
    } else {
        // (WINDOW_SIZE as u16).wrapping_sub(prev).wrapping_add(head)
        (WINDOW_SIZE as u16 - prev) + head
    }
}

fn longest_match(data: &[u8], hash_table: &ChainedHashTable, position: usize) -> (u16, u16) {
    if data.len() - position < MIN_MATCH as usize {
        return (0, 0);
    }

    let limit = cmp::min(position, MAX_MATCH as usize);

    let mut current_head = hash_table.current_head();//hash_table.get_head(hash_table.current_hash() as usize);
    let mut current_prev = hash_table.get_prev(current_head as usize);

    // assert!(current_head as usize == test_position);

    let mut best_length = 1;
    let mut best_distance = 0;

    let mut distance = distance_from_chain(current_head, current_prev);
    while distance < WINDOW_SIZE as u16 {
        if distance > 0 {
            let length = get_match_length(data, position, position - (distance as usize));
            if length > best_length {
                best_length = length;
                best_distance = distance;
            }
        }
        current_head = current_prev;
        current_prev = hash_table.get_prev(current_head as usize);
        distance += distance_from_chain(current_head, current_prev);
        println!("Distance: {}, Limit: {}", distance, limit);
        if distance as usize > limit || current_head == 0 {
            break;
        }
    }
    // println!("-");
    (best_length, best_distance)
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

    while let Some((n, b)) = (&mut data_iterator).next() {
        let position = n + 2;
        hash_table.add_hash_value(position, *b);
        let (match_len, match_dist) = longest_match_current(data, hash_table);
        if match_len >= MIN_MATCH && match_dist >= MIN_MATCH {
            output.push(LDPair::LengthDistance {
                length: match_len,
                distance: match_dist,
            });
            let taker = (&mut data_iterator).take(match_len as usize - 1);
            for (ipos, ibyte) in taker {
                // println!("ipos: {}", ipos + 2);
                hash_table.add_hash_value(ipos + 2, *ibyte);
            }
        } else {
            output.push(LDPair::Literal(*b));
        }
    }
}

/// Compress a slice
/// Returns a vector of `LDPair` values on success
pub fn lz77_compress(data: &[u8], window_size: usize) -> Option<Vec<LDPair>> {
    if window_size > DEFAULT_WINDOW_SIZE {
        return None;
    }

    if data.len() > DEFAULT_WINDOW_SIZE {
        panic!("Compressing data longer than {} bytes not properly implemented yet!", DEFAULT_WINDOW_SIZE);
    }

    let mut output = Vec::new();

    let mut hash_table = ChainedHashTable::from_starting_values(data[0], data[1]);
    output.push(LDPair::Literal(data[0]));
    output.push(LDPair::Literal(data[1]));

    //    let buffer_len = if data.len() < window_size {
    // data.len
    // } else {
    // window_size
    // };
    //
    // let mut buffer = Vec::from(&data[0..(buffer_len * 2) - 1]);

    // let mut first = true;

    let first_chunk_end = cmp::min(window_size, data.len());// - 1;

    // let mut data_iterator = data[2..first_chunk_end].iter().enumerate();
    process_chunk(data, 2, first_chunk_end, &mut hash_table, &mut output);
    // while let Some((n, b)) = (&mut data_iterator).next() {
    // let position = n + 2;
    // hash_table.add_hash_value(position, *b);
    // let (match_len, match_dist) = longest_match(data, &hash_table, position);
    // if match_len >= MIN_MATCH && match_dist >= MIN_MATCH {
    // output.push(LDPair::LengthDistance{length: match_len, distance: match_dist});
    // let mut taker = (&mut data_iterator).take(match_len as usize - 1);
    // while let Some((ipos, ibyte)) = taker.next() {
    //                println!("ipos: {}", ipos + 2);
    // hash_table.add_hash_value(ipos + 2, *ibyte);
    // }
    // } else {
    // output.push(LDPair::Literal(*b));
    // }
    // }
    //
    //
    if data.len() > window_size {
        for chunk in data.chunks(window_size * 2) {

            let loop_start = window_size - 1;
            let loop_end = cmp::min(chunk.len(), window_size * 2) - 1;

            println!("Loop length: {}", loop_end - loop_start);

//            hash_table.slide(window_size);

            /* let loop_end = if chunk.len() < window_size {
            chunk.len()
        } else {
            window_size
        }*/
/*
            for (n, b) in chunk[loop_start..loop_end].iter().enumerate() {
                let position = n + loop_start;
                hash_table.add_hash_value(position, *b);
                let (match_len, match_dist) = longest_match(chunk, &hash_table, position);
                if match_len > MIN_MATCH && match_dist > MIN_MATCH {
                    output.push(LDPair::LengthDistance{length: match_len, distance: match_dist});
                } else {
                    output.push(LDPair::Literal(*b));
                }
            }
             */
            process_chunk(chunk, loop_start, loop_end, &mut hash_table, &mut output);
        }

    }

    // println!("{}", String::from_utf8(hash_table.data).unwrap());

    Some(output)
}



#[cfg(test)]
mod test {
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

        let test_data = b"Test data, Test_data, Test data";
        let hash_table = filled_hash_table(&test_data[..22 - 1 - 1 + HASH_BYTES]);

        println!("Bytes: {}", from_utf8(&test_data[..22 - 1 - 1 + HASH_BYTES]).unwrap());
        let (length, distance) = super::longest_match_current(test_data, &hash_table);
        assert!(distance > 11);
        assert_eq!(length, 9);
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
        use chained_hash_table::WINDOW_SIZE;
        let test_bytes = String::from("Test data, test data, test dota, data test, test data.")
            .into_bytes();
        let res = super::lz77_compress(&test_bytes, WINDOW_SIZE).unwrap();
        // println!("{:?}", res);
        print_output(&res);
    }
}
