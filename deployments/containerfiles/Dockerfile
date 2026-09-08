# We use the latest stable version of the official Rust container,
# delegating to the `rust-toolchain.toml` file to pick a specific Rust toolchain.
FROM docker.io/rust:1-slim-bookworm AS build-env
ARG TARGETARCH

# Install build dependencies. These packages should match what's recommended on
# the pcli install guide.
# Repository assets are copied in from the build host.
# In order to build pcli with ledger support, add `libdbus-1-dev` & `libusb-1.0-0-dev`.
RUN apt-get update && apt-get install -y \
        build-essential \
        ca-certificates \
        curl \
        pkg-config \
        libssl-dev \
        clang \
        lld \
        protobuf-compiler

ARG GO_VERSION=1.25.4
ARG GO_AMD64_SHA256=9fa5ffeda4170de60f67f3aa0f824e426421ba724c21e133c1e35d6159ca1bec
ARG GO_ARM64_SHA256=a68e86d4b72c2c2fecf7dfed667680b6c2a071221bbdb6913cf83ce3f80d9ff0
RUN case "${TARGETARCH}" in \
      amd64) go_sha256="${GO_AMD64_SHA256}" ;; \
      arm64) go_sha256="${GO_ARM64_SHA256}" ;; \
      *) echo "unsupported TARGETARCH: ${TARGETARCH}" >&2; exit 1 ;; \
    esac \
    && curl -fsSL "https://go.dev/dl/go${GO_VERSION}.linux-${TARGETARCH}.tar.gz" -o /tmp/go.tgz \
    && echo "${go_sha256}  /tmp/go.tgz" | sha256sum -c - \
    && tar -C /usr/local -xzf /tmp/go.tgz \
    && rm /tmp/go.tgz
ENV PATH="/usr/local/go/bin:${PATH}"

WORKDIR /usr/src/shieldd
ARG BUILD_JOBS=2
ENV CARGO_BUILD_JOBS=${BUILD_JOBS} \
    RAYON_NUM_THREADS=${BUILD_JOBS} \
    GOMAXPROCS=${BUILD_JOBS} \
    GOFLAGS="-p=${BUILD_JOBS}"
COPY . .
RUN cargo build --locked --release -p pd -p pcli -p pclientd -p orbis-audit -p orbis-integration \
    --features pcli/bundled-proving-keys,pclientd/bundled-proving-keys
RUN mkdir -p /usr/lib/shieldd \
    && cd tools/gnark \
    && go build -o /usr/src/shieldd/target/release/proverdaemon ./cmd/proverdaemon \
    && go build -buildmode=c-shared -o /usr/lib/shieldd/libshieldd_gnark_transfer.so ./cmd/transferlib \
    && go build -buildmode=c-shared -o /usr/lib/shieldd/libshieldd_gnark_note_reshape.so ./cmd/note_reshapelib \
    && go build -buildmode=c-shared -o /usr/lib/shieldd/libshieldd_gnark_shielded_ics20_withdrawal.so ./cmd/shieldedics20withdrawallib

# Runtime image.
FROM docker.io/debian:bookworm-slim
ARG USERNAME=shieldd
ARG UID=1000
ARG GID=1000
# We add curl & jq so we can munge JSON during init steps for deployment.
RUN apt-get update && apt-get install -y \
        bash \
        curl \
        jq \
        libssl-dev

# Add normal user account
RUN groupadd --gid ${GID} ${USERNAME} \
        && useradd -m -d /home/${USERNAME} -g ${GID} -u ${UID} ${USERNAME}

# Install chain binaries
COPY --from=build-env \
            /usr/src/shieldd/target/release/orbis-audit \
            /usr/src/shieldd/target/release/orbis-integration \
            /usr/src/shieldd/target/release/pcli \
            /usr/src/shieldd/target/release/pclientd \
            /usr/src/shieldd/target/release/pd \
            /usr/src/shieldd/target/release/proverdaemon \
            /usr/bin/

COPY --from=build-env /usr/lib/shieldd/*.so /usr/lib/shieldd/

WORKDIR /home/${USERNAME}
USER ${USERNAME}
CMD [ "/usr/bin/pd" ]
