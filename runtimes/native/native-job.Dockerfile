# The build context contains only native compiler/runtime files selected by build-native-image.mjs.
# No package manager, model profile, project checkout, database or credential enters the image.
FROM scratch
COPY native-root/ /
LABEL io.quazonai.native-job="1" \
      io.quazonai.native-stack="rust/1.98.1;nautilus/0.63.0;clarabel/0.11.1;wasmi/2.0.0;solow-cv/0.7.3;ndarray-stats/0.7.0;linregress/0.5.4;alpha-validation/1;alpha-sealed/1;portfolio-ensemble/1;portfolio-models/4;simulation-models/1;portfolio-weights/1;portfolio-variance-bound/1;portfolio-cvar/1;portfolio-risk-budget/1;portfolio-cvar-risk-budget/1;bar-notional/1;portfolio-liquidity/1;portfolio-cost-source/1;portfolio-slippage/1"
USER 65532:65532
WORKDIR /tmp
ENTRYPOINT ["/usr/local/bin/job"]
CMD ["--version"]
