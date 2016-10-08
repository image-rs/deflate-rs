# deflate-rs
An rust implementation of a [DEFLATE](http://www.gzip.org/zlib/rfc-deflate.html) encoder. Not a direct port, but does take some inspiration from [zlib](http://www.zlib.net/) and [zopfli](https://github.com/google/zopfli)(for huffman code length generation).

So far, in-memory deflate encoding with and without zlib metadata (no zlib dictionary or gzip support yet) has been is implemented. No unsafe code has been used. Speed-wise it's not quite up to zlib-levels yet.
# Usage:
```rust
let data = ...;
let compressed = deflate_bytes(&data);
```
# Other deflate/zlib rust projects from various people
* [libflate](https://github.com/rust-lang/rust/tree/master/src/libflate) Bindings to [miniz.c](https://github.com/richgel999/miniz) that are part of the rust distribution.
* [flate2](http://alexcrichton.com/flate2-rs/flate2/index.html) FLATE, Gzip, and Zlib bindings for Rust
* [Zopfli in Rust](https://github.com/carols10cents/zopfli) Rust port of zopfli
* [inflate](https://github.com/PistonDevelopers/inflate) DEFLATE decoder implemented in rust
* [miniz-rs](https://github.com/alexchandel/miniz-rs) Direct rust translation of miniz.c

# License
deflate is distributed under the terms of both the MIT and Apache 2.0 licences.

bitstream.rs is © @nwin and was released under both MIT and Apache 2.0

The test data (src/pg11.txt) is borrowed from [Project Gutenberg](https://www.gutenberg.org/ebooks/11) and is available under public domain, or the Project Gutenberg Licence
