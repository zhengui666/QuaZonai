//! QZ policy classification over the existing native metric gate, not a statistical engine.
use crate::{evidence::evaluate_metrics, DomainError};
use contracts::{
    evidence::{Decision, EvidenceStatus, MetricRequirementV1, MetricValueV1},
    Id,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Classification {
    Healthy,
    Watch,
    Degraded,
    InsufficientData,
}
impl Classification {
    pub fn code(self) -> &'static str {
        match self {
            Self::Healthy => "HEALTHY",
            Self::Watch => "WATCH",
            Self::Degraded => "DEGRADED",
            Self::InsufficientData => "INSUFFICIENT_DATA",
        }
    }
}
pub struct Observation {
    pub classification: Classification,
    pub reason_codes: Vec<String>,
}
pub fn classify(
    evaluation: Id,
    current: bool,
    maintenance: &[MetricRequirementV1],
    promotion: &[MetricRequirementV1],
    metrics: &[MetricValueV1],
) -> Result<Observation, DomainError> {
    crate::research::metric_requirements(maintenance, "degradation_metric_requirements")?;
    crate::research::metric_requirements(promotion, "promotion_metric_requirements")?;
    if !current {
        return Ok(Observation {
            classification: Classification::InsufficientData,
            reason_codes: vec!["FORWARD_MEASUREMENT_NOT_CURRENT".into()],
        });
    }
    let capabilities = super::evaluation::capabilities();
    for (group, requirements, rejected) in [
        ("MAINTENANCE", maintenance, Classification::Degraded),
        ("PROMOTION", promotion, Classification::Watch),
    ] {
        let gate = evaluate_metrics(evaluation, requirements, metrics, &capabilities)?;
        let classification = if gate.evidence_status != EvidenceStatus::Valid {
            Some(Classification::InsufficientData)
        } else if gate.decision == Decision::Reject {
            Some(rejected)
        } else {
            None
        };
        if let Some(classification) = classification {
            return Ok(Observation {
                classification,
                reason_codes: gate
                    .reasons
                    .into_iter()
                    .map(|r| format!("{group}:{r}"))
                    .collect(),
            });
        }
    }
    Ok(Observation {
        classification: Classification::Healthy,
        reason_codes: vec!["FORWARD_REQUIREMENTS_MET".into()],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn maintenance_promotion_missing_and_original_native_method_have_distinct_meanings() {
        let evaluation = Id::new();
        let requirement = |threshold: &str| {
            serde_json::from_value::<MetricRequirementV1>(serde_json::json!({"schema_version":1,"metric_code":"FORWARD_DAILY_RETURN_MEAN","scope":"forward","comparator":"GE","threshold_low":threshold,"threshold_high":null,"required":true,"minimum_observations":"2","method_allowlist":["nautilus-analysis.ReturnsAverage"]})).unwrap()
        };
        let mut metric:MetricValueV1=serde_json::from_value(serde_json::json!({"schema_version":1,"evaluation_id":evaluation,"metric_code":"FORWARD_DAILY_RETURN_MEAN","scope":"forward","value":0.1,"status":"OK","reason_code":null,"unit":"RETURN_PER_DAY","period_start":"2026-09-10T00:00:00Z","period_end":"2026-09-12T00:00:00Z","observation_count":"2","frequency":"UTC_DAY","annualization_factor":null,"method_id":"nautilus-analysis.ReturnsAverage","method_version":"0.63.0","source_artifact_id":Id::new(),"higher_is_better":true})).unwrap();
        for (floor, target, expected) in [
            ("0", "0.01", Classification::Healthy),
            ("0", "0.2", Classification::Watch),
            ("0.2", "0.3", Classification::Degraded),
        ] {
            let result = classify(
                evaluation,
                true,
                &[requirement(floor)],
                &[requirement(target)],
                &[metric.clone()],
            )
            .unwrap();
            assert_eq!(result.classification, expected);
            assert!(!result.reason_codes.is_empty());
        }
        assert_eq!(
            classify(
                evaluation,
                false,
                &[requirement("0.2")],
                &[requirement("0.3")],
                &[metric.clone()]
            )
            .unwrap()
            .classification,
            Classification::InsufficientData
        );
        assert_eq!(
            classify(
                evaluation,
                true,
                &[requirement("0")],
                &[requirement("0")],
                &[]
            )
            .unwrap()
            .classification,
            Classification::InsufficientData
        );
        metric.method_version = "unregistered".into();
        assert_eq!(
            classify(
                evaluation,
                true,
                &[requirement("0")],
                &[requirement("0")],
                &[metric]
            )
            .unwrap()
            .classification,
            Classification::InsufficientData
        );
    }
}
