<a name="1.0.0"></a>
## 1.0.0 (2021-11-10)

* This release does not add any new features or change the API, only some minor refactor and doc fixes. It's mainly for indicating crate stability.

#### Bug Fixes

*   fix warnings from rustc and clippy, update dependencies  ([a5fcb05e](https://github.com/image-rs/deflate-rs/commit/a5fcb05e74cf6529ebb98b46701f666f1039c5e3))
*   Fix test data location and capitalize Rust, update various links, see pr #53 (thanks @atouchet) ([34fecd18](https://github.com/image-rs/deflate-rs/commit/34fecd18cafaf40ea71abfdcbf054db90e7ce5fa))

<a name="0.9.1"></a>
### 0.9.1 (2021-03-24)

#### Bug Fixes

*   Fix gzip feature that was broken in 0.9 (thanks @oheralla) ([49ac5cfe](https://github.com/image-rs/deflate-rs/commit/49ac5cfec5e1a6c4398a8753309e1f7d66108c41))


<a name="0.9.0"></a>
## 0.9.0 (2021-01-21)

#### Bug Fixes

*   Use std functions instead of byteorder (bumps minimum version to 1.32.0 ([d217fbd9](https://github.com/image-rs/deflate-rs/commit/d217fbd956597706d80efc1de93c65f4fbe957fd))

<a name="0.8.6"></a>
### 0.8.6 (2020-07-06)


#### Bug Fixes

*   try to fix issues with sync flush behaviour ([6c97e514](https://github.com/image-rs/deflate-rs/commit/6c97e5143df139af578cdd884e0dee9940414ea1), closes [#48](https://github.com/image-rs/deflate-rs/issues/48))
*   add #!forbid(unsafe_code) to crate root ([fcbe4206](https://github.com/image-rs/deflate-rs/commit/fcbe4206c45cf55d80ae8feb94f0613fe795659f))



<a name="0.8.5"></a>
### 0.8.5 (2020-07-04)


#### Bug Fixes

*   Avoid infinitely looping on sync flush with short buffer writers ([99a1a75f](99a1a75f), closes [#47](https://github.com/image-rs/deflate-rs/issues/47))
*   Remove unsafe in write_length_rle ([77227c8b](77227c8b), closes [#46](https://github.com/image-rs/deflate-rs/issues/46))



<a name="0.8.4"></a>
### 0.8.4 (2020-04-04)


#### Bug Fixes

*   Fix block size counter bug [#44](https://github.com/image-rs/deflate-rs/issues/44) (probably introduced in 1b70be)
that triggered a debug assertion and that could possibly in theory cause stored block to start at the wrong input position at a block split with low entropy data followed by uncompressible data.
