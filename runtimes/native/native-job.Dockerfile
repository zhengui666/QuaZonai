# The build context contains only native job/runtime files selected by build-native-image.mjs.
# No package manager, model profile, project checkout, database or credential enters the image.
FROM scratch
COPY native-root/ /
ARG VERSION=development
ARG REVISION=unknown
LABEL org.opencontainers.image.title="QuaZonai scientific Runtime" \
      org.opencontainers.image.source="https://github.com/zhengui666/QuaZonai" \
      org.opencontainers.image.version=$VERSION \
      org.opencontainers.image.revision=$REVISION \
      io.quazonai.native-job="1" \
      io.quazonai.native-manifest="/usr/share/doc/quazonai/native-manifest.json"
USER 65532:65532
WORKDIR /tmp
ENTRYPOINT ["/usr/local/bin/job"]
CMD ["--version"]
