//! Direct pinned Nautilus statistics on original complete daily feedback, never account emulation.
use anyhow::{ensure, Result};
use contracts::{
    forward::*,
    science::{NativeStatisticGroup, NativeStatisticV1},
    Id, SchemaV1,
};
use nautilus_analysis::{
    statistic::PortfolioStatistic,
    statistics::{
        returns_avg::ReturnsAverage, returns_volatility::ReturnsVolatility,
        sharpe_ratio::SharpeRatio,
    },
    Returns,
};
use nautilus_core::UnixNanos;

pub fn evaluate(
    request: &NativeForwardRequestV1,
    mut read: impl FnMut(Id) -> Result<Vec<u8>>,
) -> Result<NativeForwardResultV1> {
    domain::forward::evaluation::request(request)?;
    let started = chrono::Utc::now();
    ensure!(
        request.sources.iter().all(|s| s.received_at <= started),
        "FORWARD_SOURCE_TIME"
    );
    let mut total = 0usize;
    let mut points = 0usize;
    let mut sources = Vec::with_capacity(request.sources.len());
    for message in &request.sources {
        let bytes = read(message.report_artifact_id)?;
        ensure!(
            !bytes.is_empty() && bytes.len() <= 2 * 1024 * 1024,
            "FORWARD_REPORT_LIMIT"
        );
        total = total
            .checked_add(bytes.len())
            .ok_or_else(|| anyhow::anyhow!("FORWARD_REPORT_LIMIT"))?;
        ensure!(total <= 64 * 1024 * 1024, "FORWARD_REPORT_LIMIT");
        let report: ForwardReportV1 = serde_json::from_slice(&bytes)?;
        points += report.content.returns.len();
        ensure!(points <= 1_000_000, "FORWARD_REPORT_LIMIT");
        sources.push(domain::forward::ForwardWindowSource {
            message: message.clone(),
            report,
        });
    }
    let selected = domain::forward::window(
        request.window.handoff_id,
        &request.window.stream_id,
        &sources,
    )?;
    ensure!(selected.view == request.window, "FORWARD_SOURCE_CHANGED");
    let returns: Returns = selected
        .returns
        .iter()
        .map(|p| {
            Ok((
                UnixNanos::from(p.timestamp_ns.get()),
                p.value
                    .ok_or_else(|| anyhow::anyhow!("FORWARD_MISSING_RETURN"))?,
            ))
        })
        .collect::<Result<_>>()?;
    let mean = ReturnsAverage {};
    let volatility = ReturnsVolatility::new(Some(365));
    let sharpe = SharpeRatio::new(Some(365));
    let statistics = [
        (mean.name(), mean.calculate_from_returns(&returns)),
        (
            volatility.name(),
            volatility.calculate_from_returns(&returns),
        ),
        (sharpe.name(), sharpe.calculate_from_returns(&returns)),
    ]
    .into_iter()
    .map(|(native_key, value)| {
        let value = value.filter(|v| v.is_finite());
        NativeStatisticV1 {
            group: NativeStatisticGroup::Returns,
            native_key,
            currency: None,
            value,
            reason_code: value
                .is_none()
                .then(|| "NATIVE_STATISTIC_UNAVAILABLE".into()),
        }
    })
    .collect();
    let result = NativeForwardResultV1 {
        schema_version: SchemaV1,
        native_version: "0.63.0".into(),
        window: selected.view,
        statistics,
    };
    domain::forward::evaluation::binding(request, &result)?;
    Ok(result)
}
