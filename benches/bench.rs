#![feature(test)]
#![feature(rustc_private)]

extern crate deflate;
extern crate test;
extern crate flate;
use test::Bencher;

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
    let test_data = get_test_file_data("tests/pg11.txt");

    b.iter(|| deflate::deflate_bytes_zlib(&test_data));
}

#[bench]
fn test_file_zlib_flate(b: &mut Bencher) {
    let test_data = get_test_file_data("tests/pg11.txt");
    b.iter(|| flate::deflate_bytes_zlib(&test_data));
}
