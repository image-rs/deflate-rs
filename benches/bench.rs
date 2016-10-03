#![feature(test)]

extern crate deflate;
extern crate test;
extern crate flate2;
use test::Bencher;
use flate2::Compression;

fn load_from_file(name: &str) -> Vec<u8> {
    use std::fs::File;
    use std::io::Read;
    let mut input = Vec::new();
    let mut f = File::open(name).unwrap();

    f.read_to_end(&mut input).unwrap();
    input
}

fn get_test_data() -> Vec<u8> {
    use std::env;
    let path = env::var("TEST_FILE").unwrap_or("tests/pg11.txt".to_string());
    load_from_file(&path)
}

#[bench]
fn test_file_zlib(b: &mut Bencher) {
    let test_data = get_test_data();

    b.iter(|| deflate::deflate_bytes_zlib(&test_data));
}

fn deflate_bytes_flate2_zlib(level: Compression, input: &[u8]) -> Vec<u8> {
    use flate2::write::ZlibEncoder;
    use std::io::Write;

    let mut e = ZlibEncoder::new(Vec::new(), level);
    e.write_all(input).unwrap();
    e.finish().unwrap()
}

#[bench]
fn test_file_zlib_flate2_def(b: &mut Bencher) {
    let test_data = get_test_data();
    b.iter(|| deflate_bytes_flate2_zlib(Compression::Default, &test_data));
}

#[bench]
fn test_file_zlib_flate2_max(b: &mut Bencher) {
    let test_data = get_test_data();
    b.iter(|| deflate_bytes_flate2_zlib(Compression::Best, &test_data));
}
