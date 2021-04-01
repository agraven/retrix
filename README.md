# retrix

Retrix is a lightweight matrix client built with [iced] and [matrix-rust-sdk].

The project is currently in early stages, and is decidedly not feature complete. Also note that both iced and matrix-sdk are somewhat unstable and under very rapid development, which means that there might be functionality that's broken or can't be implemented that I don't have direct influence over.

## Features
- [x] Rooms
	- [x] List rooms
	- [ ] Join rooms
	- [ ] Explore public room list
	- [ ] Create room
- [ ] Communities
- [x] Messages
	- [x] Plain text
	- [ ] Formatted text (waiting on iced, markdown will be shown raw)
	- [ ] Stickers
	- [x] Images (in unencrypted rooms)
	- [ ] Audio
	- [ ] Video
	- [ ] Location
- [x] E2E Encryption
	- [x] Import key export
	- [x] Receiving verification start
	- [ ] Receiving verification request (waiting on matrix-sdk)
- [ ] Account settings
	- [ ] Device management
	- [ ] Change password
- [x] Profile settings
	- [x] Display name
	- [ ] Avatar

### Things I (currently) don't intend to implement
- VoIP Calls

## Building
Retrix can be compiled with
```bash
cargo build --release
```
Be warned that retrix is very heavy to build due to the dependencies it uses. On the less powerful of my laptops, it takes on average 6 minutes to build in release mode.

## Installing
You can put the compiled binary wherever binaries go. Retrix keeps its configuration and caching data in `~/.config/retrix` on linux systems, and in `%APPDATA%\retrix` on windows systems. It will automatically create the needed folder if it does not exist.

[iced]: https://github.com/hecrj/iced
[matrix-rust-sdk]: https://github.com/matrix-org/matrix-rust-sdk
