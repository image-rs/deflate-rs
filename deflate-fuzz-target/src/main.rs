use afl::fuzz;
use deflate::CompressionOptions;

fn roundtrip(data: &[u8]) {
    roundtrip_conf(data, CompressionOptions::default());
    roundtrip_conf(data, CompressionOptions::fast());
}

fn roundtrip_conf(data: &[u8], level: CompressionOptions) {
    let compressed = deflate::deflate_bytes_zlib_conf(data, level);
    let decompressed =
        miniz_oxide::inflate::decompress_to_vec_zlib(&compressed).expect("Decompression failed!");
    assert!(decompressed.as_slice() == data);
}

fn main() {
    fuzz!(|data: &[u8]| {
        roundtrip(data)
    });
}

