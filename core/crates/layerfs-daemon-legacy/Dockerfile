FROM scratch
ARG BINARY_SHA256
LABEL org.layerfs.binary-sha256=$BINARY_SHA256
COPY layerfs-daemon /layerfs-daemon
COPY --chown=65532:65532 --chmod=0700 logs /logs
USER 65532:65532
ENTRYPOINT ["/layerfs-daemon"]
