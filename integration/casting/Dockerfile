# Build the official release against the distribution's complete decoder library.
# Upstream's classic Docker image omits the PCM decoder used by some RAOP senders.
FROM alpine:3.23.4 AS builder
RUN apk add --no-cache build-base autoconf automake libtool pkgconf curl \
        openssl-dev avahi-dev libconfig-dev popt-dev pulseaudio-dev soxr-dev ffmpeg-dev \
    && curl --fail --location --retry 3 https://github.com/mikebrady/shairport-sync/archive/refs/tags/5.6.1.tar.gz -o /source.tar.gz \
    && echo '48d2ac1afa52d86766bc328694fa5d2d5f13a674f3e980f00d95d957453e5ee1  /source.tar.gz' | sha256sum -c - \
    && mkdir /source \
    && tar -xzf /source.tar.gz -C /source --strip-components=1
WORKDIR /source
RUN autoreconf -fi \
    && ./configure --sysconfdir=/etc --with-ssl=openssl --with-avahi \
        --with-pulseaudio --with-soxr --with-ffmpeg --with-dummy --with-stdout \
    && make -j2 \
    && make DESTDIR=/install install

FROM alpine:3.23.4
RUN apk add --no-cache avahi dbus libconfig libcrypto3 libpulse popt soxr ffmpeg-libs su-exec pulseaudio pulseaudio-utils python3 mpv openssl \
    && mkdir -p /run/dbus \
    && rm -f /etc/avahi/services/*.service
COPY --from=builder /install/usr/local/bin/shairport-sync /usr/local/bin/shairport-sync
COPY --from=builder /source/LICENSES /usr/share/licenses/shairport-sync/LICENSES
COPY --from=builder /source/COPYING /usr/share/licenses/shairport-sync/COPYING
COPY casting/start.py /usr/local/bin/start.py
COPY casting/cast_receiver.py /usr/local/bin/cast_receiver.py
COPY snapcast/music-fixture.py /usr/local/bin/music-fixture.py
COPY snapcast/tone.flac /usr/local/share/sonicfast-tone.flac
ENTRYPOINT ["python3", "/usr/local/bin/start.py"]
