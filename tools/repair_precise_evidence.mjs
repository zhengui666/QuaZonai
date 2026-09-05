// One-time executor-authored source transformation. No network, credentials,
// application execution, dependency resolution or git mutation.
import { readFileSync, writeFileSync, existsSync, mkdirSync } from 'node:fs';
import assert from 'node:assert/strict';

const read = (path) => readFileSync(path, 'utf8');
function replaceFunction(text, signature, replacement) {
  const start = text.indexOf(signature);
  assert(start >= 0, `Missing expected function: ${signature}`);
  const open = text.indexOf('{', start);
  let depth = 1;
  let end = open + 1;
  for (; depth && end < text.length; end++) {
    if (text[end] === '{') depth++;
    else if (text[end] === '}') depth--;
  }
  assert.equal(depth, 0, 'Unbalanced source function');
  return text.slice(0, start) + replacement + text.slice(end);
}
function replaceOnce(text, old, replacement) {
  assert.equal(text.split(old).length, 2, `Expected exactly one source occurrence: ${old}`);
  return text.replace(old, replacement);
}
const scalarPath = 'crates/contracts/src/scalars.rs';
let scalars = read(scalarPath);
if (!scalars.includes('pub fn compare_metric(')) {
  scalars = replaceFunction(scalars, 'pub fn metric_threshold(', `pub fn compare_metric(&self, value: f64) -> Result<std::cmp::Ordering, String> {
        if !value.is_finite() {
            return Err("non-finite metric".into());
        }
        // Preserve the observable JSON number and the complete frozen decimal.
        // Do not round the threshold or invent a binary floating-point tail.
        let wire = serde_json::to_string(&value).map_err(|_| "invalid metric")?;
        let metric = BigDecimal::from_str(&wire).map_err(|_| "invalid metric decimal")?;
        Ok(metric.cmp(&self.0))
    }`);
  scalars = scalars.replace('/// Only statistical metric thresholds use floating point, never money or weights.', '/// Compare a finite observable metric with the exact frozen threshold.');
  writeFileSync(scalarPath, scalars);
}

const evidencePath = 'crates/domain/src/evidence.rs';
let evidence = read(evidencePath);
if (evidence.includes('.metric_threshold()')) {
  evidence = replaceFunction(evidence, 'fn thresholds(', `fn thresholds(
    requirement: &MetricRequirementV1,
) -> Result<(Option<&contracts::DecimalValue>, Option<&contracts::DecimalValue>), DomainError> {
    let low = requirement.threshold_low.as_ref();
    let high = requirement.threshold_high.as_ref();
    match (requirement.comparator, low, high) {
        (Comparator::Gt | Comparator::Ge, Some(_), None)
        | (Comparator::Lt | Comparator::Le, None, Some(_)) => Ok((low, high)),
        (Comparator::Between, Some(low), Some(high)) if low <= high => Ok((Some(low), Some(high))),
        _ => Err(DomainError::Invalid("metric_threshold_bounds")),
    }
}

fn compare_threshold(bound: &contracts::DecimalValue, value: f64) -> Result<std::cmp::Ordering, DomainError> {
    bound.compare_metric(value).map_err(|_| DomainError::Invalid("metric_value"))
}`);
  const ordering = 'std::cmp::Ordering';
  evidence = replaceOnce(evidence, '(Comparator::Gt, Some(low), _) => value > low,', `(Comparator::Gt, Some(low), _) => compare_threshold(low, value)? == ${ordering}::Greater,`);
  evidence = replaceOnce(evidence, '(Comparator::Ge, Some(low), _) => value >= low,', `(Comparator::Ge, Some(low), _) => compare_threshold(low, value)? != ${ordering}::Less,`);
  evidence = replaceOnce(evidence, '(Comparator::Lt, _, Some(high)) => value < high,', `(Comparator::Lt, _, Some(high)) => compare_threshold(high, value)? == ${ordering}::Less,`);
  evidence = replaceOnce(evidence, '(Comparator::Le, _, Some(high)) => value <= high,', `(Comparator::Le, _, Some(high)) => compare_threshold(high, value)? != ${ordering}::Greater,`);
  evidence = replaceOnce(evidence, 'value >= low && value <= high', `compare_threshold(low, value)? != ${ordering}::Less && compare_threshold(high, value)? != ${ordering}::Greater`);
  writeFileSync(evidencePath, evidence);
} else {
  assert(evidence.includes('compare_metric') || evidence.includes('compare_threshold'), 'Unexpected evidence comparison implementation');
}

for (const path of ['crates/contracts/src/budget.rs', 'crates/contracts/src/runs.rs']) {
  const source = read(path);
  let count = 0;
  const updated = source.replace(/((?:    #\[[^\n]*\]\n)*)    pub (\w+): ((?:Option<)?(u16|u32)>?),/g, (_, attributes, field, type, integer) => {
    count++;
    const maximum = integer === 'u16' ? '65535' : '4294967295u64';
    attributes = attributes.replace(/    #\[schema\(maximum\s*=\s*(?:65535|4294967295u64)\)\]\n/g, '');
    assert(!attributes.includes('maximum'), `Review existing schema bounds manually: ${path}:${field}`);
    return `${attributes}    #[schema(maximum = ${maximum})]\n    pub ${field}: ${type},`;
  });
  assert(count > 0, `No integer contracts found: ${path}`);
  writeFileSync(path, updated);
}

mkdirSync('crates/contracts/tests', { recursive: true });
writeFileSync('crates/contracts/tests/exact_numeric_contract.rs', `use contracts::{budget::BudgetV1, DecimalValue};
use serde_json::{json, Value};
use std::cmp::Ordering;

#[test]
fn generated_integer_bounds_match_native_wire_representations() {
    let document: Value = serde_json::from_str(&contracts::openapi_json().unwrap()).unwrap();
    let schemas = &document["components"]["schemas"];
    for field in ["max_parallel_runs", "max_turns_per_mission", "max_repair_turns", "max_cycles_per_day"] {
        assert_eq!(schemas["BudgetV1"]["properties"][field]["maximum"], json!(65535), "{field}");
    }
    for field in ["stop_on_qualified_count", "stop_on_no_improvement_trials"] {
        assert_eq!(schemas["StopRuleV1"]["properties"][field]["maximum"], json!(65535), "{field}");
    }
    for field in ["max_experiments", "max_wall_seconds", "max_memory_mib", "min_cycle_interval_seconds"] {
        assert_eq!(schemas["BudgetV1"]["properties"][field]["maximum"], json!(4294967295u64), "{field}");
    }
    let mut value = json!({"schema_version":1,"max_experiments":20,"max_parallel_runs":65535,
        "max_turns_per_mission":16,"max_repair_turns":2,"max_wall_seconds":3600,
        "max_cpu_seconds":"7200","max_memory_mib":4096,"max_output_bytes":"67108864",
        "max_cycles_per_day":3,"min_cycle_interval_seconds":120,"max_tokens":null,
        "max_cost_decimal":null,"cost_currency":null,"cost_enforcement":"UNAVAILABLE"});
    assert!(serde_json::from_value::<BudgetV1>(value.clone()).is_ok());
    value["max_parallel_runs"] = json!(65536);
    assert!(serde_json::from_value::<BudgetV1>(value).is_err());
}

#[test]
fn observable_metric_comparison_preserves_exact_decimal_thresholds() {
    let exact: DecimalValue = "0.1".parse().unwrap();
    let greater: DecimalValue = "0.10000000000000001".parse().unwrap();
    assert_eq!(exact.compare_metric(0.1).unwrap(), Ordering::Equal);
    assert_eq!(greater.compare_metric(0.1).unwrap(), Ordering::Less);
    assert_eq!("-0.10000000000000001".parse::<DecimalValue>().unwrap().compare_metric(-0.1).unwrap(), Ordering::Greater);
    assert_eq!("0".parse::<DecimalValue>().unwrap().compare_metric(f64::MIN_POSITIVE).unwrap(), Ordering::Greater);
    assert_eq!(exact.compare_metric(f64::MAX).unwrap(), Ordering::Greater);
    for number in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(exact.compare_metric(number).is_err());
    }
}
`);
mkdirSync('crates/domain/tests', { recursive: true });
writeFileSync('crates/domain/tests/exact_thresholds.rs', `use chrono::{TimeZone, Utc};
use contracts::{evidence::*, DbCounter, Id, SchemaV1};
use domain::evidence::{evaluate_metrics, MetricCapability};

fn requirement(comparator: Comparator, low: Option<&str>, high: Option<&str>) -> MetricRequirementV1 {
    MetricRequirementV1 {
        schema_version: SchemaV1, metric_code: "risk".into(), scope: "total".into(), comparator,
        threshold_low: low.map(|x| x.parse().unwrap()), threshold_high: high.map(|x| x.parse().unwrap()),
        required: true, minimum_observations: DbCounter::new(1).unwrap(), method_allowlist: vec!["native-risk".into()],
    }
}

#[test]
fn exact_bounds_are_used_for_validation_and_every_gate_comparator() {
    let id=Id::new();
    let metric=MetricValueV1 {
        schema_version:SchemaV1, evaluation_id:id, metric_code:"risk".into(), scope:"total".into(),
        value:Some(0.1), status:MetricStatus::Ok, reason_code:None, unit:"fraction".into(),
        period_start:Utc.with_ymd_and_hms(2026,1,1,0,0,0).unwrap(),
        period_end:Utc.with_ymd_and_hms(2026,2,1,0,0,0).unwrap(),
        observation_count:DbCounter::new(30).unwrap(), frequency:"daily".into(), annualization_factor:None,
        method_id:"native-risk".into(), method_version:"1".into(), source_artifact_id:Id::new(), higher_is_better:None,
    };
    let capability=MetricCapability {metric_code:"risk".into(), method_id:"native-risk".into(),method_version:"1".into(),unit:"fraction".into(),frequency:"daily".into()};
    for (rule,expected) in [
        (requirement(Comparator::Ge,Some("0.10000000000000001"),None),Decision::Reject),
        (requirement(Comparator::Gt,Some("0.1"),None),Decision::Reject),
        (requirement(Comparator::Ge,Some("0.1"),None),Decision::Pass),
        (requirement(Comparator::Lt,None,Some("0.10000000000000001")),Decision::Pass),
        (requirement(Comparator::Le,None,Some("0.09999999999999999")),Decision::Reject),
        (requirement(Comparator::Between,Some("0.09999999999999999"),Some("0.10000000000000001")),Decision::Pass),
    ] {
        assert_eq!(evaluate_metrics(id,&[rule],std::slice::from_ref(&metric),std::slice::from_ref(&capability)).unwrap().decision,expected);
    }
    let reversed=requirement(Comparator::Between,Some("0.10000000000000001"),Some("0.1"));
    assert!(evaluate_metrics(id,&[reversed],&[metric],&[capability]).is_err());
}
`);

let design = read('DESIGN.md');
const heading = '## C. 精确数值、Mission 原生 Turn 账本与交付复合关系';
if (!design.includes(heading)) {
  design += `\n\n${heading}\n\n本节补齐 A4/A6/A7 的规范约束；不是已建成 SQL/API 或全量验收的声明。已有必填性、权限、不可变性和项目隔离约束同时生效。\n\n### C1. 指标的可观察十进制语义\n\nMetric 的有限 f64 由 Serde 原生 JSON 序列化为可往返的十进制数值，再用 BigDecimal 原生解析，与冻结的 Decimal 阈值精确比较。不得把阈值先转为 f64，也不把未通过 wire 暴露的二进制尾数当成额外有效数字。0.1 与 0.1 相等；0.1 不满足 GE 0.10000000000000001。BETWEEN 的上下界按完整 Decimal 比较，不能先舍入后接受逆序区间。NaN/Infinity 一律拒绝。u16 的生成 wire 上限为65535，u32为4294967295；DB/资源政策可进一步收窄，不默默截断。\n\n### C2. 一个 Mission 的唯一原生会话\n\n数据库必须建立 UNIQUE(codex_sessions.run_id) 和 UNIQUE(codex_sessions.profile_id,thread_id)。run_id 是 Mission 身份；Reviewer 必须是不同的 Mission/Run，不能靠同一 Mission 换 session_id 重置预算。同一 Mission 的会话创建重试须返回相同 session/profile/thread；不一致返回409。身份字段一经绑定禁止改写。原生 thread/start 的未知结果进入恢复，不盲目创建另一条 Thread。四个已用/预约 Turn 计数是下述账本的事务投影，不能作为唯一恢复事实。\n\n### C3. 每次模型请求的持久账本\n\n所有记录继承 A0 的 UUIDv7、时间和不可变约束；各本地 ID 都是实际 FK。\n\n\`\`\`text\nmodel_turn_reservations [immutable]\n  project_id: Id FK projects\n  cycle_id: Id FK research_cycles\n  mission_id: Id FK runs\n  session_id: Id FK codex_sessions\n  attempt_id: Id FK run_attempts\n  owner_epoch: Rev\n  request_id: Id UNIQUE\n  ordinal: int in [1,65535]\n  turn_kind: RESEARCH|REPAIR\n  input_artifact_id: Id FK artifacts\n  reserved_tokens: bigint > 0\n  reserved_estimated_cost: Decimal? >= 0\n  cost_currency: ISO4217?\n  profile_revision: Rev\n\nmodel_turn_dispatches [immutable, one per reservation]\n  reservation_id: Id UNIQUE FK model_turn_reservations\n  thread_id: nonempty text\n  dispatch_intent_at: Time\n\nmodel_turn_acknowledgements [immutable, one per reservation]\n  reservation_id: Id UNIQUE FK model_turn_reservations\n  profile_id: Id FK codex_profiles\n  thread_id: nonempty text\n  native_turn_id: nonempty text\n  acknowledged_at: Time\n\nmodel_turn_terminals [immutable, one per reservation]\n  reservation_id: Id UNIQUE FK model_turn_reservations\n  native_turn_id: text?\n  outcome: COMPLETED|FAILED|INTERRUPTED|CONFIRMED_NOT_SENT\n  reason_code: text?\n  observed_at: Time\n\nmodel_turn_settlements [immutable, one per reservation]\n  reservation_id: Id UNIQUE FK model_turn_reservations\n  used_tokens: bigint >= 0\n  used_estimated_cost: Decimal? >= 0\n  cost_currency: ISO4217?\n  usage_source: NATIVE_USAGE|CONFIRMED_NOT_SENT\n  settled_at: Time\n\`\`\`\n\nUNIQUE(mission_id,ordinal) 与 UNIQUE(session_id,ordinal) 阻止重复消费。预约的 session 必须属于精确 mission/project，attempt 必须属于精确 mission，cycle/输入必须属于该项目。费用及币种同时存在或同时为空；有费用上限时不允许未知账目。ACK 对 (profile_id,thread_id,native_turn_id) 全局唯一，profile/thread 必须等于该 session，且只能在持久 dispatch intent 后写入。除 CONFIRMED_NOT_SENT 外，terminal 的 (reservation_id,native_turn_id) 必须引用该预约的 ACK。\n\n锁顺序为项目/周期 → Run → Attempt → Session → Reservation。准入在同事务内核对当前 lease/epoch、冻结政策、项目状态/期限与所有预算，追加预约并更新周期 token/费用与 Mission total/repair 计数。每个 Session 同时最多一条未结算预约；request_id 同语义重试返回旧记录，异义409，不再次占用预算。Repair 同时消耗总 Turn 和 repair Turn。\n\n先提交 dispatch intent 再调用原生 App Server，外部调用不持 DB 锁。网络超时、进程退出或 ACK 丢失不能推定未发送，更不能退还占用；按原生 Thread/Turn 恢复并关联已有预约，无法证明关联则明确 RECONCILING，不盲目重发。CONFIRMED_NOT_SENT 只允许没有 dispatch intent 的预约，不能用本地超时替代证明。\n\n新 owner 只接管既有 Run/Attempt，并以当前 DB epoch 验证恢复权限，不改写旧预约归属。终态与可信 usage 均已观察到后，追加一次 settlement，并在同事务把相应 reserved 转为 used。原生实际消耗超出预约也如实记录，不裁剪或丢弃；超额阻止下一次准入。缺少可信用量则保留待结算占用；暂停、取消、过期不得阻止记录已发生的消耗。结算重传相同内容返回旧记录，不同内容409；不得同时记录两种 outcome 或重复计费。已用 Turn 不因失败、取消或恢复清零。\n\n### C4. 精确绑定评估和交付授权\n\n在 evaluations 建立 UNIQUE(id,subject_candidate_id)，releases 的 (evaluation_id,candidate_id) 复合 FK 必须引用该组合；Alpha 评估或其他 Candidate 的 PASS 不能用于当前 Release。Mandate、项目、输入、资格、独立评估状态、新鲜度等 Gate 仍要事务验证，FK 不代表已合格。\n\n在 approvals 建立 UNIQUE(id,release_id,downstream_id,environment)，handoff_offers 的 (approval_id,release_id,downstream_id,environment) 必须作为同一复合 FK 引用。Paper 审批不能授权 Live，其他下游或其他 Release 的审批不能借用。Offer/Claim 的权限、到期/撤销、人工拒绝、数据授权及 readiness 检查不得因复合 FK 存在而省略。\n\nT10/T23–T26 必须增加真实 DB 的预约并发、相同 request_id 重试、dispatch-before-ACK 崩溃、旧 owner 回报、未知用量不退款、重复结算、实际超额及暂停后结算测试；T28/T29 必须负向验证错 Candidate、错 Release、错下游和 Paper/Live 混用。纯函数或文档不替代这些集成证据。\n`;
  writeFileSync('DESIGN.md', design);
}

const recordPath = 'docs/architecture/issue-62-execution.md';
if (existsSync(recordPath)) {
  let record = read(recordPath);
  if (!record.includes('## Verification record scope')) {
    record = record.replace('## Actual local evidence', '## Historical local evidence');
    record += '\n\n## Verification record scope\n\nNumeric test totals in the historical section belong to that earlier source and dependency context, not an assertion of the current Head. The current suites and exact dependency combination are established by locked CI logs and their tested commit. A local development/vendor lock is not the product lock and cannot replace that evidence. This repair adds exact decimal-bound comparisons and integer schema regressions; generated contracts must be regenerated with utoipa, committed, and independently compared in read-only CI. New model-turn ledger and composite-FK requirements remain implementation targets until the Store and real database fault tests are committed and pass.\n';
    writeFileSync(recordPath, record);
  }
}

const ciPath='.github/workflows/ci.yml';
let ci=read(ciPath);
if (!/\bworkflow_dispatch\s*:/.test(ci)) {
  assert(/^on:\s*$/m.test(ci),'Cannot safely add explicit read-only CI dispatch to this workflow format');
  ci=ci.replace(/^on:\s*$/m,'on:\n  workflow_dispatch:');
  writeFileSync(ciPath,ci);
}
console.log('Prepared exact numeric and recovery contracts; no tests or delivery claimed.');
