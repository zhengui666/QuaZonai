# The build context contains only native compiler/runtime files selected by build-native-image.mjs.
# No package manager, model profile, project checkout, database or credential enters the image.
FROM scratch
COPY native-root/ /
LABEL io.quazonai.native-job="1" \
      io.quazonai.native-stack="rust/1.98.1;nautilus/0.63.0;clarabel/0.11.1;wasmi/2.0.0"
USER 65532:65532
WORKDIR /tmp
ENTRYPOINT ["/usr/local/bin/job"]
CMD ["--version"]
