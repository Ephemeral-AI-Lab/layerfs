# Build with the owned local tag of the pinned base image, --pull=false.
# Base identity: sha256:8ea0250bb24d7b184dd473da3915162c2ac8e8669867c6d55f91be278b67cf9f
ARG BASE=phase6-live-provider-base:local
FROM ${BASE}
COPY phase6-live-probe /layerfs-daemon
ENTRYPOINT ["/layerfs-daemon"]
