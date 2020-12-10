# retrix

Retrix is a lightweight matrix client built with [iced] and [matrix-rust-sdk].

The project is currently in early stages, and is decidedly not feature complete. Also note that both iced and matrix-sdk are somewhat unstable and under very rapid development, which means that there might be functionality that's broken or can't be implemented that I don't have direct influence over.

# Features
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
	- [ ] Images
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

## Things I (currently) don't intend to implement
- VoIP Calls

[iced]: https://github.com/hecrj/iced
[matrix-rust-sdk]: https://github.com/matrix-org/matrix-rust-sdk
