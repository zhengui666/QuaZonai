from __future__ import annotations

from pathlib import Path

path = Path(__file__).with_name("apply_issue22_core_closure.py")
content = path.read_text(encoding="utf-8")
old = '''replace_once(
    "backend/src/quant_runtime/contracts.py",
    \'\'\'    risk_config: dict[str, Any] = Field(default_factory=dict)
    tags: dict[str, str] = Field(default_factory=dict)


class OrderEvidence(StrictModel):
\'\'\',
    \'\'\'    risk_config: dict[str, Any] = Field(default_factory=dict)
    tags: dict[str, str] = Field(default_factory=dict)

    @model_validator(mode="after")
    def validate_v1_configuration(self) -> BacktestExperimentRequest:
        if self.start_time is not None:
            _require_aware_datetime(self.start_time, field_name="start_time")
        if self.end_time is not None:
            _require_aware_datetime(self.end_time, field_name="end_time")
        if (
            self.start_time is not None
            and self.end_time is not None
            and self.start_time >= self.end_time
        ):
            raise ValueError("start_time must precede end_time")
        if self.data_config:
            raise ValueError(
                "data_config is reserved until protocol v1 explicitly applies its fields; "
                "use the top-level catalog/instrument/time contract instead"
            )
        if self.risk_config:
            raise ValueError(
                "risk_config is reserved until protocol v1 explicitly applies a Nautilus "
                "RiskEngine configuration"
            )
        return self


class OrderEvidence(StrictModel):
\'\'\',
)
'''
new = '''replace_once(
    "backend/src/quant_runtime/contracts.py",
    \'\'\'    @model_validator(mode="after")
    def reject_unapplied_configuration(self) -> BacktestExperimentRequest:
        if self.data_config:
            raise ValueError(
                "data_config is reserved until protocol v1 explicitly applies its fields; use the "
                "top-level catalog/instrument/time contract instead"
            )
        if self.risk_config:
            raise ValueError(
                "risk_config is reserved until protocol v1 explicitly applies a Nautilus RiskEngine "
                "configuration"
            )
        return self
\'\'\',
    \'\'\'    @model_validator(mode="after")
    def validate_v1_configuration(self) -> BacktestExperimentRequest:
        if self.start_time is not None:
            _require_aware_datetime(self.start_time, field_name="start_time")
        if self.end_time is not None:
            _require_aware_datetime(self.end_time, field_name="end_time")
        if (
            self.start_time is not None
            and self.end_time is not None
            and self.start_time >= self.end_time
        ):
            raise ValueError("start_time must precede end_time")
        if self.data_config:
            raise ValueError(
                "data_config is reserved until protocol v1 explicitly applies its fields; use the "
                "top-level catalog/instrument/time contract instead"
            )
        if self.risk_config:
            raise ValueError(
                "risk_config is reserved until protocol v1 explicitly applies a Nautilus RiskEngine "
                "configuration"
            )
        return self
\'\'\',
)
'''
if content.count(old) != 1:
    raise RuntimeError(f"expected one driver block, found {content.count(old)}")
path.write_text(content.replace(old, new, 1), encoding="utf-8")
