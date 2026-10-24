# TODO

- [x] There is a weird bug where if you search and select songs and try to play a song it says  This track needs an unsupported codec; select MP3, FLAC or M4A (AAC/ALAC), but if I go to the album page and select the same song it works fine.
- [x] We probably should add a way to rate songs from the bottom player bar as well as to add the song to a playlist. Perhaps we just add the same 3 dot menu from the song track list to the bottom player bar?
- [x] On the track list can we animate "wav icon" to the left of currently played song? On the actual spotify app it renders a really smooth looking set of 5 vertical bars that animate up and down smoothly to better illustrate the track is playing.
- [x] The top search for Playlists does not work and always returns nothing.
- [x] The top search for Podcasts and episodes also does not work since this is not spotify
- [x] We need to do a global replacement of spotifast with sonicfast and update the README.md to call out that we forked from spotifast. The winamp theme also says spotifast in the title bar.
- [x] We need to update the github workflows and try out the actual packaging
- [ ] We need to remake the demo video

- [x] We need to remake the demo screenshot and add it to the readme
- [x] Remove the "By You" option since I don't think it matters
- [x] Add support for multiple subsonic servers with a connection list (only one at a time)
- [x] Add "Sync" now button refresh the subsonic api backend (if it's needed?)
- [x] We need to then update the docs for the connection list changes
- [ ] AirPlay now-playing metadata (title/artist/album/cover/progress) is written and unit-tested in code only: not compiled or run yet. Run `make test`, then check on Shairport Sync (with a metadata front end) and the Pioneer VSX-1123 / Roku that text, cover and progress appear and update on track change and seek.
- [ ] Cast receiver check + fingerprint dialog (2026-10-24) is unit-tested and screenshotted only. Pick a self-hosted Cast receiver (e.g. the Docker software target from `make casting-test`) in the output picker: confirm the Verify dialog opens, a pasted fingerprint connects and is reused next time, and a genuine Chromecast connects with no dialog. Consider adding the unpinned→dialog path to `integration/casting/test.py`.
- [ ] Flaky checks seen 2026-10-24 (both passed on rerun): one `make test` run had a single unidentified lib-test failure (output not kept; next 3 runs 1420/1420), and `make snapcast-test` once failed `Native clients disagree on playout` at 39 ms vs the 35 ms limit (rerun 0.05 ms). Capture the failing test name if the lib failure recurs.
- [ ] Snapcast output switch (v0.15.1): check on a real Snapserver that turning **Snapcast output** on lists it in the picker without starting to send, choosing it sends, and turning the switch off while sending returns to This device.
- [ ] MPD output (ADR 014): the owner saw rpi-mpd-office (MPD 0.24.0) discovered and playing (2026-10-24). Still to confirm on the Pi: pause/seek/volume/next song. MPD passwords are not supported yet (needs the credential store).
- [ ] Flaky: `make casting-test` once failed the AirPlay app proof at stage 1 ("did not complete at stage 1: Paused, 2000 ms" — the 2 s tone ended during the output switch); the rerun passed. A longer fixture tone would give the transitions more room.
- [ ] Cast trust-on-first-use (2026-10-24): proven with the Docker software receiver and app tests. Try it from the picker against the garage `rpi-mpd-office Cast` receiver: Trust receiver once, then reconnect without a prompt.
