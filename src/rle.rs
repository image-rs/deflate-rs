use lz77::{ProcessStatus,buffer_full};
use output_writer::{OutputWriter, BufferStatus};
use matching::get_match_length;
use huffman_table;

use std::ops::Range;
use std::cmp;

const MIN_MATCH: usize = huffman_table::MIN_MATCH as usize;

pub fn process_chunk_greedy_rle<W: OutputWriter>(data: &[u8],
                                       iterated_data: &Range<usize>,
                                       writer: &mut W)
                                           -> (usize, ProcessStatus) {
    let end = cmp::min(data.len(), iterated_data.end);
    // Start on at least byte 1.
    let start = cmp::max(iterated_data.start,1);
    // Iterate through the requested range, but avoid going off the end.
    let current_chunk = &data[cmp::min(start,end)..end];
    let mut insert_it = current_chunk.iter().enumerate();
    let mut overlap = 0;
    // Make sure to output the first byte
    if iterated_data.start == 0 && data.len() > 0  {
        write_literal!(writer,data[0],1);
    }

    while let Some((n,&b)) = insert_it.next() {
        let position = n + start;
        let match_len = get_match_length(&data,position,position-1);
        if match_len >= MIN_MATCH {
            if position + match_len > end {
                // We need to subtract 1 since the byte at pos is also included.
                overlap = position + match_len - end;
            };
            let b_status = writer.write_length_distance(match_len as u16, 1);
            if b_status == BufferStatus::Full {
                return (overlap, buffer_full(position + match_len));
            }
            insert_it.nth(match_len - 2);
        } else {
            write_literal!(writer,b,position+1);
        }
    }

    (overlap,ProcessStatus::Ok)
}

#[cfg(test)]
mod test {
    use super::*;
    use output_writer::FixedWriter;
    use lzvalue::{LZValue, lit, ld};

    fn l(c: char) -> LZValue {
        lit(c as u8)
    }

    #[test]
    fn rle_compress() {
        let input = b"textaaaaaaaaatext";
        let mut w = FixedWriter::new();
        let r = 0..input.len();
        let (overlap,_) = process_chunk_greedy_rle(input, &r, &mut w);
        let expected = [l('t'),l('e'),l('x'),l('t'),l('a'),ld(8,1),l('t'),l('e'),l('x'),l('t')];
        println!("expected: {:?}", expected);
        println!("actual: {:?}", w.get_buffer());
        assert!(w.get_buffer() == expected);
        assert_eq!(overlap,0);
    }
}
