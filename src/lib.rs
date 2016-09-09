#[cfg(test)]
extern crate flate2;
//#[cfg(test)]
//extern crate inflate;

extern crate adler32;

mod huffman_table;
mod lz77;
mod chained_hash_table;
mod length_encode;
mod output_writer;
#[cfg(test)]
mod stored_block;
mod huffman_lengths;
mod zlib;
mod checksum;
mod bit_reverse;
mod bitstream;
mod encoder_state;

use huffman_table::*;
use lz77::LDPair;
use huffman_lengths::{write_huffman_lengths, remove_trailing_zeroes};
use length_encode::huffman_lengths_from_frequency;
use checksum::RollingChecksum;
use std::io::{Write, Cursor};
use std::io;
use encoder_state::EncoderState;

pub enum BType {
    NoCompression = 0b00,
    FixedHuffman = 0b01,
    DynamicHuffman = 0b10,
    // Reserved = 0b11, //Error
}

#[cfg(test)]
fn compress_data_fixed(input: &[u8]) -> Vec<u8> {
    use lz77::lz77_compress;

    let mut writer = Cursor::new(Vec::new());
    {
    let mut state = EncoderState::fixed(&mut writer);
    let compressed = lz77_compress(input, chained_hash_table::WINDOW_SIZE).unwrap();

    //We currently don't split blocks, we should do this eventually
    state.write_start_of_block(true).expect("Write error!");
    for ld in compressed {
        //We ignore end of block here for now since there is no purpose of
        //splitting a full stream of data using fixed huffman data into blocks
        match ld {
            LDPair::BlockStart{..}|LDPair::EndOfBlock =>
            (),
            _ => {println!("{:?}", ld);
                  state.write_ldpair(ld).expect("Write error!")},
        }
    }

    state.write_end_of_block().expect("Write error");
    state.flush().expect("Writer error!");

    }
    writer.into_inner()
}

fn compress_data_dynamic<RC: RollingChecksum, W: Write>(input: &[u8], mut writer: &mut W, mut checksum: &mut RC) -> io::Result<()> {
    let mut state = EncoderState::new(huffman_table::HuffmanTable::empty(), &mut writer);

    let mut lz77_state = lz77::LZ77State::new(input);
    let mut lz77_writer = output_writer::DynamicWriter::new();

    checksum.update_from_slice(&input[..2]);

    while !lz77_state.is_last_block() {
        lz77::lz77_compress_block::<output_writer::DynamicWriter, RC>(input, &mut lz77_state, &mut lz77_writer, &mut checksum);
        try!(state.write_start_of_block(lz77_state.is_last_block()));

        let (l_lengths, d_lengths) = {
            let (l_freqs, d_freqs) = lz77_writer.get_frequencies();
            // The huffman spec allows us to exclude zeroes at the end of the table of
            // of huffman lengths. Since a frequency of 0 will give an huffman length of 0
            // we strip off the trailing zeroes before even generating the lengths to save
            // some work
            (huffman_lengths_from_frequency(remove_trailing_zeroes(l_freqs), MAX_CODE_LENGTH),
             huffman_lengths_from_frequency(remove_trailing_zeroes(d_freqs), MAX_CODE_LENGTH))
        };
        try!(write_huffman_lengths(&l_lengths, &d_lengths, &mut state.writer));
        let codes = HuffmanTable::from_length_tables(&l_lengths, &d_lengths).expect(
            "Error: Failed to create huffman table!"
        );
        state.set_huffman_table(codes);

        for ld in lz77_writer.get_buffer() {
            match *ld {
                LDPair::BlockStart{..} => (),
                _ =>  try!(state.write_ldpair(*ld)),
            }
        }

        // End of block is written in write_ldpair
        lz77_writer.clear();
    }

    state.flush().unwrap();

    Ok(())
}

pub fn deflate_bytes(input: &[u8]) -> Vec<u8> {
    let mut writer = Cursor::new(Vec::with_capacity(input.len() / 3));
    compress_data_dynamic(input, &mut writer, &mut checksum::NoChecksum::new()).unwrap();
    writer.into_inner()
}

pub fn deflate_bytes_zlib(input: &[u8]) -> Vec<u8> {
    let mut writer = Cursor::new(Vec::with_capacity(input.len() / 3));
    // Write header
    zlib::write_zlib_header(&mut writer, zlib::CompressionLevel::Default).unwrap();

    let mut checksum = checksum::Adler32Checksum::new();
    compress_data_dynamic(input, &mut writer, &mut checksum).unwrap();

    let hash = checksum.current_hash();
    println!("Hash {}", hash);
    writer.write_all(&[(hash >> 24) as u8, (hash >> 16) as u8, (hash >> 8) as u8, hash as u8]).unwrap();
    println!("last byte: {}", hash as u8);
    writer.into_inner()
}

#[cfg(test)]
mod test {
    use stored_block::compress_data_stored;
    use super::compress_data_fixed;

    /// Helper function to decompress into a `Vec<u8>`
    fn decompress_to_end(input: &[u8]) -> Vec<u8> {
        /*use std::str;
        let mut inflater = super::inflate::InflateStream::new();
        let mut out = Vec::new();
        let mut n = 0;
        println!("input len {}", input.len());
        while n < input.len() {
            let res = inflater.update(&input[n..]) ;
            if let Ok((num_bytes_read, result)) = res {
                println!("result len {}, bytes_read {}", result.len(), num_bytes_read);
                n += num_bytes_read;
                out.extend(result);
            } else {
                //println!("Output: `{}`", str::from_utf8(&out).unwrap());
                println!("Output decompressed: {}", out.len());
                res.unwrap();
            }

        }
        out*/

        use std::io::Read;
        use flate2::read::DeflateDecoder;

        let mut result = Vec::new();
        let i = &input[..];
        let mut e = DeflateDecoder::new(i);

        let res = e.read_to_end(&mut result);
        if let Ok(n) = res {
            println!("{} bytes read successfully", n);
        } else {
            println!("result size: {}", result.len());
            res.unwrap();
        }
        result
    }

    use super::*;

    fn get_test_file_data(name: &str) -> Vec<u8> {
        use std::fs::File;
        use std::io::Read;
        let mut input = Vec::new();
        let mut f = File::open(name).unwrap();

        f.read_to_end(&mut input).unwrap();
        input
    }


    #[test]
    fn test_no_compression_one_chunk() {
        let test_data = vec![1u8, 2, 3, 4, 5, 6, 7, 8];
        let compressed = compress_data_stored(&test_data);
        let result = decompress_to_end(&compressed);
        assert_eq!(test_data, result);
    }

    #[test]
    fn test_no_compression_multiple_chunks() {
        let test_data = vec![32u8; 40000];
        let compressed = compress_data_stored(&test_data);
        let result = decompress_to_end(&compressed);
        assert_eq!(test_data, result);
    }

    #[test]
    fn test_no_compression_string() {
        let test_data = String::from("This is some text, this is some more text, this is even \
                                      more text, lots of text here.")
            .into_bytes();
        let compressed = compress_data_stored(&test_data);
        let result = decompress_to_end(&compressed);
        assert_eq!(test_data, result);
    }

    #[test]
    fn test_fixed_string_mem() {
        use std::str;
        // let test_data = b".......................BB";
        let test_data = String::from("                    GNU GENERAL PUBLIC LICENSE").into_bytes();
        let compressed = compress_data_fixed(&test_data);

        let result = decompress_to_end(&compressed);
        println!("Output: `{}`", str::from_utf8(&result).unwrap());
        assert_eq!(test_data, result);
    }

    #[test]
    fn test_fixed_data() {

        let data = vec![190u8; 400];
        let compressed = compress_data_fixed(&data);
        let result = decompress_to_end(&compressed);

        println!("data len: {}, result len: {}", data.len(), result.len());
        for n in compressed {
            println!("{:#b}", n)
        }
        assert_eq!(data, result);
    }

    /// Test deflate example.
    ///
    /// Check if the encoder produces the same code as the example given by Mark Adler here:
    /// https://stackoverflow.com/questions/17398931/deflate-encoding-with-static-huffman-codes/17415203
    #[test]
    fn test_fixed_example() {
        let test_data = b"Deflate late";
        // let check =
        // [0x73, 0x49, 0x4d, 0xcb, 0x49, 0x2c, 0x49, 0x55, 0xc8, 0x49, 0x2c, 0x49, 0x5, 0x0];
        let check = [0x73, 0x49, 0x4d, 0xcb, 0x49, 0x2c, 0x49, 0x55, 0x00, 0x11, 0x00];
        let compressed = compress_data_fixed(test_data);
        assert_eq!(&compressed, &check);
        let decompressed = decompress_to_end(&compressed);
        assert_eq!(&decompressed, test_data)
    }

    #[test]
    fn test_fixed_string_file() {
        use std::fs::File;
        use std::io::Read;
        use std::str;
        let mut input = Vec::new();

        let mut f = File::open("src/pg11.txt").unwrap();

        f.read_to_end(&mut input).unwrap();
        let compressed = compress_data_fixed(&input);
        println!("Compressed len: {}", compressed.len());
        let result = decompress_to_end(&compressed);
        let out1 = str::from_utf8(&input).unwrap();
        let out2 = str::from_utf8(&result).unwrap();
        // println!("Orig:\n{}", out1);
        // println!("Compr:\n{}", out2);
        println!("Orig len: {}, out len: {}", out1.len(), out2.len());
        // Not using assert_eq here deliberately to avoid massive amounts of output spam
        assert!(input == result);
    }



    #[test]
    fn test_dynamic_string_mem() {
        use std::str;
        let test_data = String::from("                    GNU GENERAL PUBLIC LICENSE").into_bytes();
        let compressed = deflate_bytes(&test_data);

        let result = decompress_to_end(&compressed);
        assert_eq!(test_data, result);
    }

    #[test]
    fn test_dynamic_string_file() {
        use std::str;
        let input = get_test_file_data("src/pg11.txt");
        let compressed = deflate_bytes(&input);

        println!("Compressed len: {}", compressed.len());

        let result = decompress_to_end(&compressed);
        // Check that we actually managed to compress the input
        assert!(compressed.len() < input.len());
        // Not using assert_eq here deliberately to avoid massive amounts of output spam
        assert!(input == result);
    }

    #[test]
    fn test_dynamic_string_zlib() {
        use std::io::Read;
        use flate2::read::ZlibDecoder;

        let test_data = get_test_file_data("src/pg11.txt");
        //let test_data = String::from("foo zdsujghns aaaaaa hello hello eshtguiq3ayth932wa7tyh13a79hgqae78guh").into_bytes();
        let compressed = deflate_bytes_zlib(&test_data);

        {
            use std::fs::File;
            use std::io::Write;
            let mut f = File::create("out.zlib").unwrap();
            f.write_all(&compressed).unwrap();
        }

        println!("Last bytes: {:?}", &compressed[compressed.len() - 11..]);

        let mut e = ZlibDecoder::new(&compressed[..]);

        let mut result = Vec::new();
        e.read_to_end(&mut result).unwrap();
        assert!(&test_data == &result);
    }
}
