#![feature(test)]

extern crate deflate;
extern crate test;
extern crate flate2;
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
    let test_data = get_test_file_data("src/pg11.txt");

    b.iter(||
           for _ in 1..5 {
               deflate::deflate_bytes_zlib(&test_data);
           }
    );
}
