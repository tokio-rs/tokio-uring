# 0.2.0 (January 8th, 2021)

### Fixed
fs: fix error handling related to changes in rustc ([#69])
op: fix 'already borrowed' panic ([#39])

### Added
fs: add fs::remove_file ([#66])
fs: implement Debug for File ([#65])
fs: add remove_dir and unlink ([#63])
buf: impl IoBuf/IoBufMut for bytes::Bytes/BytesMut ([#43])

[#69]: https://github.com/tokio-rs/tokio-uring/pull/69
[#66]: https://github.com/tokio-rs/tokio-uring/pull/66
[#65]: https://github.com/tokio-rs/tokio-uring/pull/65
[#63]: https://github.com/tokio-rs/tokio-uring/pull/63
[#39]: https://github.com/tokio-rs/tokio-uring/pull/39
[#43]: https://github.com/tokio-rs/tokio-uring/pull/43