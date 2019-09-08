#![cfg(test)]

#[cfg(feature = "gzip")]
use flate2::read::GzDecoder;

fn get_test_file_data(name: &str) -> Vec<u8> {
    use std::fs::File;
    use std::io::Read;
    let mut input = Vec::new();
    let mut f = File::open(name).unwrap();

    f.read_to_end(&mut input).unwrap();
    input
}

pub fn get_test_data() -> Vec<u8> {
    use std::env;
    let path = env::var("TEST_FILE").unwrap_or("tests/pg11.txt".to_string());
    get_test_file_data(&path)
}

/// Helper function to decompress into a `Vec<u8>`
pub fn decompress_to_end(input: &[u8]) -> Vec<u8> {
    use miniz_oxide::inflate::decompress_to_vec;

    decompress_to_vec(input).expect("Decompression failed!")
}

#[cfg(feature = "gzip")]
pub fn decompress_gzip(compressed: &[u8]) -> (GzDecoder<&[u8]>, Vec<u8>) {
    use std::io::Read;
    let mut e = GzDecoder::new(&compressed[..]).unwrap();

    let mut result = Vec::new();
    e.read_to_end(&mut result).unwrap();
    (e, result)
}

pub fn decompress_zlib(compressed: &[u8]) -> Vec<u8> {
    miniz_oxide::inflate::decompress_to_vec_zlib(&compressed).expect("Decompression failed!")
}
