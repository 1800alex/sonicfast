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
