# deflate-rs
An pure rust implementation of a [DEFLATE](http://www.gzip.org/zlib/rfc-deflate.html) encoder. Not a direct port, but does take some inspiration from [zlib](http://www.zlib.net/) and [zopfli](https://github.com/google/zopfli)(for huffman code length generation).

So far, in-memory deflate encoding (without [lazy matching](http://www.gzip.org/zlib/rfc-deflate.html#algorithm), and no zlib dictionary or gzip support yet) has been is implemented. No unsafe code has been used. Currently not very optimised.
# Usage:
```rust
let data = ...;
let compressed = deflate_bytes(&data);
```
# Other deflate/zlib rust projects
* [libflate](https://github.com/rust-lang/rust/tree/master/src/libflate) Bindings to [miniz.c](https://github.com/richgel999/miniz) that are part of the rust distribution.
* [flate2](http://alexcrichton.com/flate2-rs/flate2/index.html) FLATE, Gzip, and Zlib bindings for Rust 
* [Zopfli in Rust](https://github.com/carols10cents/zopfli) Rust port of zopfli
* [inflate](https://github.com/PistonDevelopers/inflate) Pure rust DEFLATE decoder
* [miniz-rs](https://github.com/alexchandel/miniz-rs) Direct rust translation of miniz.c

# License
Both MIT and Apache 2.0
