extern crate deflate;
extern crate miniz_oxide;

use deflate::CompressionOptions;
use std::io::{Read, Write};

fn get_test_file_data(name: &str) -> Vec<u8> {
    use std::fs::File;
    let mut input = Vec::new();
    let mut f = File::open(name).unwrap();

    f.read_to_end(&mut input).unwrap();
    input
}

fn get_test_data() -> Vec<u8> {
    use std::env;
    let path = env::var("TEST_FILE").unwrap_or_else(|_| "tests/pg11.txt".to_string());
    get_test_file_data(&path)
}

fn roundtrip(data: &[u8]) {
    roundtrip_conf(data, CompressionOptions::default())
}

fn roundtrip_conf(data: &[u8], level: CompressionOptions) {
    let compressed = deflate::deflate_bytes_zlib_conf(data, level);
    println!("Compressed len: {}, level: {:?}", compressed.len(), level);
    let decompressed =
        miniz_oxide::inflate::decompress_to_vec_zlib(&compressed).expect("Decompression failed!");
    assert!(decompressed.as_slice() == data);
}

// A test comparing the compression ratio of the library with flate2
#[test]
fn file_zlib_compare_output() {
    let test_data = get_test_data();
    let flate2_compressed = miniz_oxide::deflate::compress_to_vec_zlib(&test_data, 10);

    // {
    //     use std::fs::File;
    //     use std::io::Write;
    //     {
    //         let mut f = File::create("out.deflate").unwrap();
    //         f.write_all(&deflate_compressed).unwrap();
    //     }
    //     {
    //         let mut f = File::create("out.flate2").unwrap();
    //         f.write_all(&flate2_compressed).unwrap();
    //     }
    // }

    println!("mz_oxide len: {}", flate2_compressed.len(),);

    roundtrip_conf(&test_data, CompressionOptions::high());
}

#[test]
fn block_type() {
    let test_file = "tests/short.bin";
    let test_data = get_test_file_data(test_file);
    let compressed = deflate::deflate_bytes_zlib(&test_data);
    assert_eq!(compressed.len(), 30);

    roundtrip(b"test");
}

#[test]
fn issue_17() {
    // This is window size + 1 bytes long which made the hash table
    // slide when there was only the two end-bytes that don't need to be hashed left
    // and triggered an assertion.
    let data = vec![0; 65537];

    roundtrip(&data);
}

#[ignore]
#[test]
fn issue_44() {
    // Data that results in overlap after non-first window.
    // Triggered the debug check due to overlap being added to
    // current_block_input_bytes when it should not have.
    // Test file is compressed to avoid wasting space,
    // and ignored by default due to slowness/memory use.
    let compr = get_test_file_data("tests/issue_44.zlib");
    let data = miniz_oxide::inflate::decompress_to_vec_zlib(&compr)
        .expect("failed to decompress test file");

    roundtrip(&data);
}

#[test]
fn fast() {
    let test_data = get_test_data();
    roundtrip_conf(&test_data, CompressionOptions::fast());
}

#[test]
fn rle() {
    use deflate::{deflate_bytes_conf, CompressionOptions};
    let test_data = get_test_data();
    let compressed = deflate_bytes_conf(&test_data, CompressionOptions::rle());
    let decompressed =
        miniz_oxide::inflate::decompress_to_vec(&compressed).expect("Decompression failed!");

    println!("Input size: {}", test_data.len());
    println!("Rle compressed len: {}", compressed.len());

    assert!(test_data == decompressed);
}

#[test]
fn issue_26() {
    use deflate::write::ZlibEncoder;
    let fp = Vec::new();
    let mut fp = ZlibEncoder::new(fp, CompressionOptions::default());

    fp.write(&[0]).unwrap();
    fp.flush().unwrap();
    fp.write(&[0]).unwrap();
    fp.write(&[0, 0]).unwrap();
}

#[cfg(feature = "gzip")]
#[test]
fn issue_26_gzip() {
    use deflate::write::DeflateEncoder;
    let fp = Vec::new();
    let mut fp = DeflateEncoder::new(fp, CompressionOptions::default());

    fp.write(&[0]).unwrap();
    fp.flush().unwrap();
    fp.write(&[0]).unwrap();
    fp.write(&[0, 0]).unwrap();
}

#[test]
fn issue_18_201911() {
    let test_file = "tests/issue_18_201911.bin";
    let test_data = get_test_file_data(test_file);
    // This was the failing compression mode.
    roundtrip_conf(&test_data, deflate::Compression::Fast.into());
    roundtrip_conf(&test_data, CompressionOptions::default());
}

#[test]
fn afl_regressions_default_compression() {
    for entry in std::fs::read_dir("tests/afl/default").unwrap() {
        let entry = entry.unwrap();
        let test_file = entry.path();
        if test_file.is_file() {
            let test_filename = test_file.to_str().unwrap();
            println!("{}", test_filename);
            let test_data = get_test_file_data(test_filename);
            // This was the failing compression mode.
            roundtrip_conf(&test_data, CompressionOptions::default());
            roundtrip_conf(&test_data, deflate::Compression::Fast.into());
        }
    }
}

mod issue_47 {
    use std::io::{self, Write};

    #[test]
    fn issue_47() {
        let _ = deflate::write::ZlibEncoder::new(
            SmallWriter::new(vec![], 2),
            deflate::Compression::Fast,
        )
        .flush();
    }

    struct SmallWriter<W: Write> {
        writer: W,
        small: usize,
    }

    impl<W: Write> SmallWriter<W> {
        fn new(writer: W, buf_len: usize) -> SmallWriter<W> {
            SmallWriter {
                writer,
                small: buf_len,
            }
        }
    }

    impl<W: Write> Write for SmallWriter<W> {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            // Never write more than `small` bytes at a time.
            let small = buf.len().min(self.small);
            self.writer.write(&buf[..small])
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
}
