# Official Snapcast binaries; never build from a moving branch or third-party image.
FROM debian:bookworm-slim
ARG TARGETARCH
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl python3 pulseaudio pulseaudio-utils \
    && printf '#!/bin/sh\nexit 101\n' > /usr/sbin/policy-rc.d && chmod +x /usr/sbin/policy-rc.d \
    && case "$TARGETARCH" in \
      amd64) server_sha=afb012318bb8cbf82c63c2084de3b290f53108c49976ff364100882cd8816b41; client_sha=8b9ee34dbf77e89172903b95100063dc88f1cf7bfa0e41a8caf3d6a78856a60b ;; \
      arm64) server_sha=526cded407fad5940e834d77cf66a3b7ebac68931a530be55179f60d327044fd; client_sha=4e0445eb1cf792da209f18f3d0aecb982595bd9b2530a313617be4321f43406d ;; \
      *) echo 'Supported fixture architectures: amd64, arm64' >&2; exit 1 ;; esac \
    && for package in snapserver snapclient; do \
      suffix=""; if [ "$package" = snapclient ]; then suffix="_with-pulse"; fi; \
      curl --fail --location --retry 3 "https://github.com/snapcast/snapcast/releases/download/v0.35.0/${package}_0.35.0-1_${TARGETARCH}_bookworm${suffix}.deb" -o "/${package}.deb"; \
    done \
    && echo "$server_sha  /snapserver.deb" | sha256sum -c - \
    && echo "$client_sha  /snapclient.deb" | sha256sum -c - \
    && apt-get install -y --no-install-recommends /snapserver.deb /snapclient.deb \
    && rm -f /snapserver.deb /snapclient.deb && rm -rf /var/lib/apt/lists/* \
    && mkdir -p /captures /state && chmod 777 /captures /state
COPY snapserver.conf /etc/sonicfast-snapserver.conf
COPY healthcheck.py /usr/local/bin/fixture-healthcheck
COPY tone.flac /usr/local/share/sonicfast-tone.flac
COPY music-fixture.py /usr/local/bin/
COPY start-server.py tone.py record-pulse.py /usr/local/bin/
CMD ["python3", "/usr/local/bin/start-server.py"]
