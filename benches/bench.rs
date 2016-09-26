#![feature(test)]
#![feature(rustc_private)]

extern crate deflate;
extern crate test;
extern crate flate;
extern crate flate2;
use test::Bencher;

const FILENAME: &'static str = "tests/pg11.txt";

fn get_test_file_data(name: &str) -> Vec<u8> {
    use std::fs::File;
    use std::io::Read;
    let mut input = Vec::new();
    let mut f = File::open(name).unwrap();

    f.read_to_end(&mut input).unwrap();
    input
}

#[bench]
fn test_file_zlib(b: &mut Bencher) {
    let test_data = get_test_file_data(FILENAME);

    let _ = b.iter(|| deflate::deflate_bytes_zlib(&test_data));
}

fn deflate_bytes_flate2_zlib(input: &[u8]) -> Vec<u8> {
    use flate2::Compression;
    use flate2::write::ZlibEncoder;
    use std::io::Write;

    let mut e = ZlibEncoder::new(Vec::new(), Compression::Best);
    e.write_all(input).unwrap();
    e.finish().unwrap()
}

#[bench]
fn test_file_zlib_flate(b: &mut Bencher) {
    let test_data = get_test_file_data(FILENAME);
    let _ = b.iter(|| flate::deflate_bytes_zlib(&test_data));
}

#[bench]
fn test_file_zlib_flate2(b: &mut Bencher) {
    let test_data = get_test_file_data(FILENAME);
    b.iter(|| deflate_bytes_flate2_zlib(&test_data));
}
