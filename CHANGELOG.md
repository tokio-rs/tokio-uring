# 0.4.0 (November 5th, 2022)

### Fixed

- Fix panic in Deref/DerefMut for Slice extending into uninitialized part of the buffer ([#52])
- docs: all-features = true ([#84])
- fix fs unit tests to avoid parallelism ([#121])
- Box the socket address to allow moving the Connect future ([#126])
- rt: Fix data race ([#146])

### Added

- Implement fs::File::readv_at()/writev_at() ([#87])
- fs: implement FromRawFd for File ([#89])
- Implement `AsRawFd` for `TcpStream` ([#94])
- net: add TcpListener.local_addr method ([#107])
- net: add TcpStream.write_all ([#111])
- driver: add Builder API as an option to start ([#113])
- Socket and TcpStream shutdown ([#124])
- fs: implement fs::File::from_std ([#131])
- net: implement FromRawFd for TcpStream ([#132])
- fs: implement OpenOptionsExt for OpenOptions ([#133])
- Add NoOp support ([#134])
- Add writev to TcpStream ([#136])
- sync TcpStream, UnixStream and UdpSocket functionality ([#141])
- Add benchmarks for no-op submission ([#144])
- Expose runtime structure ([#148])

### Changed

- driver: batch submit requests and add benchmark ([#78])
- Depend on io-uring version ^0.5.8 ([#153])

### Internal Improvements

- chore: fix clippy lints ([#99])
- io: refactor post-op logic in ops into Completable ([#116])
- Support multi completion events: v2 ([#130])
- simplify driver operation futures ([#139])
- rt: refactor runtime to avoid Rc\<RefCell\<...>> ([#142])
- Remove unused dev-dependencies ([#143])
- chore: types and fields explicitly named ([#149])
- Ignore errors from uring while cleaning up ([#154])
- rt: drop runtime before driver during shutdown ([#155])
- rt: refactor drop logic ([#157])
- rt: fix error when calling block_on twice ([#162])

### CI changes

- chore: update actions/checkout action to v3 ([#90])
- chore: add all-systems-go ci check ([#98])
- chore: add clippy to ci ([#100])
- ci: run cargo test --doc ([#135])


[#52]: https://github.com/tokio-rs/tokio-uring/pull/52
[#78]: https://github.com/tokio-rs/tokio-uring/pull/78
[#84]: https://github.com/tokio-rs/tokio-uring/pull/84
[#87]: https://github.com/tokio-rs/tokio-uring/pull/87
[#89]: https://github.com/tokio-rs/tokio-uring/pull/89
[#90]: https://github.com/tokio-rs/tokio-uring/pull/90
[#94]: https://github.com/tokio-rs/tokio-uring/pull/94
[#98]: https://github.com/tokio-rs/tokio-uring/pull/98
[#99]: https://github.com/tokio-rs/tokio-uring/pull/99
[#100]: https://github.com/tokio-rs/tokio-uring/pull/100
[#107]: https://github.com/tokio-rs/tokio-uring/pull/107
[#111]: https://github.com/tokio-rs/tokio-uring/pull/111
[#113]: https://github.com/tokio-rs/tokio-uring/pull/113
[#116]: https://github.com/tokio-rs/tokio-uring/pull/116
[#121]: https://github.com/tokio-rs/tokio-uring/pull/121
[#124]: https://github.com/tokio-rs/tokio-uring/pull/124
[#126]: https://github.com/tokio-rs/tokio-uring/pull/126
[#130]: https://github.com/tokio-rs/tokio-uring/pull/130
[#131]: https://github.com/tokio-rs/tokio-uring/pull/131
[#132]: https://github.com/tokio-rs/tokio-uring/pull/132
[#133]: https://github.com/tokio-rs/tokio-uring/pull/133
[#134]: https://github.com/tokio-rs/tokio-uring/pull/134
[#135]: https://github.com/tokio-rs/tokio-uring/pull/135
[#136]: https://github.com/tokio-rs/tokio-uring/pull/136
[#139]: https://github.com/tokio-rs/tokio-uring/pull/139
[#141]: https://github.com/tokio-rs/tokio-uring/pull/141
[#142]: https://github.com/tokio-rs/tokio-uring/pull/142
[#143]: https://github.com/tokio-rs/tokio-uring/pull/143
[#144]: https://github.com/tokio-rs/tokio-uring/pull/144
[#146]: https://github.com/tokio-rs/tokio-uring/pull/146
[#148]: https://github.com/tokio-rs/tokio-uring/pull/148
[#149]: https://github.com/tokio-rs/tokio-uring/pull/149
[#153]: https://github.com/tokio-rs/tokio-uring/pull/153
[#154]: https://github.com/tokio-rs/tokio-uring/pull/154
[#155]: https://github.com/tokio-rs/tokio-uring/pull/155
[#157]: https://github.com/tokio-rs/tokio-uring/pull/157
[#162]: https://github.com/tokio-rs/tokio-uring/pull/162

# 0.3.0 (March 2nd, 2022)
### Added
- net: add unix stream & listener ([#74])
- net: add tcp and udp support ([#40])

[#74]: https://github.com/tokio-rs/tokio-uring/pull/74
[#40]: https://github.com/tokio-rs/tokio-uring/pull/40

# 0.2.0 (January 9th, 2022)

### Fixed
- fs: fix error handling related to changes in rustc ([#69])
- op: fix 'already borrowed' panic ([#39])

### Added
- fs: add fs::remove_file ([#66])
- fs: implement Debug for File ([#65])
- fs: add remove_dir and unlink ([#63])
- buf: impl IoBuf/IoBufMut for bytes::Bytes/BytesMut ([#43])

[#69]: https://github.com/tokio-rs/tokio-uring/pull/69
[#66]: https://github.com/tokio-rs/tokio-uring/pull/66
[#65]: https://github.com/tokio-rs/tokio-uring/pull/65
[#63]: https://github.com/tokio-rs/tokio-uring/pull/63
[#39]: https://github.com/tokio-rs/tokio-uring/pull/39
[#43]: https://github.com/tokio-rs/tokio-uring/pull/43
