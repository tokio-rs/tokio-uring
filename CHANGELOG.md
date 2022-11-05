# 0.4.0 (November 5th, 2022)

### Fixed

- Fix panic in Deref/DerefMut for Slice extending into uninitialized part of the buffer ([#52])
- by mzabaluev

- chore: fix clippy lints ([#99])
- by Noah-Kennedy

- fix fs unit tests to avoid parallelism ([#121])
- by FrankReh

- Box the socket address to allow moving the Connect future ([#126])
- by fasterthanlime

- rt: Fix data race ([#146])
- by ollie-etl


### Added

- Tokio-uring design proposal ([#1])
- by carllerche

- driver: batch submit requests and add benchmark ([#78])
- by Noah-Kennedy

- Implement fs::File::readv_at()/writev_at() ([#87])
- by jiangliu

- fs: implement FromRawFd for File ([#89])
- by jiangliu

- Implement `AsRawFd` for `TcpStream` ([#94])
- by dpc

- net: add TcpListener.local_addr method ([#107])
- by FrankReh

- write all ([#111])
- by FrankReh

- driver: add Builder API as an option to start ([#113])
- by FrankReh

- Socket and TcpStream shutdown ([#124])
- by FrankReh

- fs: implement fs::File::from_std ([#131])
- by littledivy

- net: implement FromRawFd for TcpStream ([#132])
- by littledivy

- fs: implement OpenOptionsExt for OpenOptions ([#133])
- by littledivy

- Add NoOp support ([#134])
- by ollie-etl

- Add writev to TcpStream ([#136])
- by fasterthanlime

- sync TcpStream, UnixStream and UdpSocket functionality ([#141])
- by FrankReh

- Add benchmarks for no-op submission ([#144])
- by ollie-etl

- Expose runtime structure ([#148])
- by ollie-etl

- Depend on io-uring version ^0.5.8 ([#153])
- by FrankReh

### Refactors

- io: refactor post-op logic in ops into Completable ([#116])
- by Noah-Kennedy

- Support multi completion events: v2 ([#130])
- by ollie-etl

- simplify driver operation futures ([#139])
- by FrankReh

- rt: refactor runtime to avoid Rc&lt;RefCell&lt;...&gt;&gt; ([#142])
- by Noah-Kennedy

- Remove unused dev-dependencies ([#143])
- by ollie-etl

- chore: types and fields explicitly named ([#149])
- by ollie-etl

- Ignore errors from uring while cleaning up ([#154])
- by FrankReh

- rt: drop runtime before driver during shutdown ([#155])
- by Noah-Kennedy

- rt: refactor drop logic ([#157])
- by Noah-Kennedy

- rt: fix error when calling block_on twice ([#162])
- by Noah-Kennedy

### CI changes

- chore: update actions/checkout action to v3 ([#90])
- by taiki-e

- chore: add all-systems-go ci check ([#98])
- by Noah-Kennedy

- chore: add clippy to ci ([#100])
- by Noah-Kennedy

- ci: run cargo test --doc ([#135])
- by ollie-etl

### Unclassified so far

- docs: all-features = true ([#84])
by AaronO

[#1]: https://github.com/tokio-rs/tokio-uring/pull/1
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
