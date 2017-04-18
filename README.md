# deflate-rs

[![Build Status](https://travis-ci.org/oyvindln/deflate-rs.svg)](https://travis-ci.org/oyvindln/deflate-rs) [![Crates.io](https://img.shields.io/crates/v/deflate.svg)](https://crates.io/crates/deflate) [![Docs](https://docs.rs/deflate/badge.svg)](https://docs.rs/deflate)


An rust implementation of a [DEFLATE](http://www.gzip.org/zlib/rfc-deflate.html) encoder. Not a direct port, but does take some inspiration from [zlib](http://www.zlib.net/), [miniz](https://github.com/richgel999/miniz) and [zopfli](https://github.com/google/zopfli).

So far, deflate encoding with and without zlib metadata (no zlib dictionary or gzip support yet) has been is implemented. No unsafe code has been used. Speed-wise it's not quite up to miniz-levels yet.
# Usage:
## Simple compression function:
``` rust
use deflate::deflate_bytes;

let data = b"Some data";
let compressed = deflate_bytes(&data);
```

## Using a writer:

``` rust
use std::io::Write;

use deflate::Compression;
use deflate::write::ZlibEncoder;

let data = b"This is some test data";
let mut encoder = ZlibEncoder::new(Vec::new(), Compression::Default);
encoder.write_all(data).unwrap();
let compressed_data = encoder.finish().unwrap();
```

# Other deflate/zlib rust projects from various people
* [libflate](https://github.com/rust-lang/rust/tree/master/src/libflate) Bindings to [miniz.c](https://github.com/richgel999/miniz) that are part of the rust distribution.
* [flate2](http://alexcrichton.com/flate2-rs/flate2/index.html) FLATE, Gzip, and Zlib bindings for Rust
* [Zopfli in Rust](https://github.com/carols10cents/zopfli) Rust port of zopfli
* [inflate](https://github.com/PistonDevelopers/inflate) DEFLATE decoder implemented in rust
* [miniz-rs](https://github.com/alexchandel/miniz-rs) Direct rust translation of miniz.c
* [libflate](https://github.com/sile/libflate) (Not to be confused by libflate in the rust standard library) Another DEFLATE/Zlib/Gzip encoder and decoder written in Rust. (Only does some very light compression).

# License
deflate is distributed under the terms of both the MIT and Apache 2.0 licences.

bitstream.rs is © @nwin and was released under both MIT and Apache 2.0

Some code in length_encode.rs has been ported from the `miniz` library, which is public domain.

The test data (src/pg11.txt) is borrowed from [Project Gutenberg](https://www.gutenberg.org/ebooks/11) and is available under public domain, or the Project Gutenberg Licence
