import type { ExecutionErrorCategory, ExecutionErrorDetail } from '../../../packages/contracts/src/execution.ts';

type ExecutionContext = { operation?: unknown; stage?: unknown };
type Fallback = 'scientific_execution_failed' | 'backtest_failed' | 'operation_failed';
const operations = new Set(['evaluate-factor', 'forecast', 'forecast-factor', 'evaluate-experiment', 'simulate', 'study-portfolio', 'allocate',
  'blend-targets', 'assess-capacity', 'size-account', 'validate-capital-exit', 'portfolio-evaluation', 'portfolio-backtest']);
const stages = new Set(['compile', 'execute', 'decode', 'validate']);

export function executionDiagnostic(error: unknown, fallback = 'scientific_execution_failed'): string {
  return error instanceof Error ? error.message : typeof error === 'string' ? error : fallback;
}

/** Context survives child-task receipts without replacing the original private error text. */
export class ExecutionFailure extends Error {
  readonly operation?: string;
  readonly stage?: ExecutionErrorDetail['stage'];
  constructor(error: unknown, context: ExecutionContext = {}) {
    super(executionDiagnostic(error), { cause: error });
    if (typeof context.operation === 'string' && operations.has(context.operation)) this.operation = context.operation;
    if (typeof context.stage === 'string' && stages.has(context.stage)) this.stage = context.stage as ExecutionErrorDetail['stage'];
  }
}

type Reason = { codes: string[]; category: ExecutionErrorCategory; message: string; action: string };
const reasons: Reason[] = [
  { codes: ['QZ_NATIVE_JOB_MEMORY_LIMIT'], category: 'resource', message: '原生编译器触及内存限制。', action: '检查本机可用内存和模型复杂度；本次失败已记录，不会自动重算。' },
  { codes: ['QZ_SIMULATION_INSTRUMENT_UPDATES_UNSUPPORTED', 'SIMULATION_INSTRUMENT_UPDATES_UNSUPPORTED'], category: 'runtime',
    message: '当前原生模拟引擎不支持区间内的合约定义变更。', action: '使用定义稳定且满足原研究条件的数据区间，或升级支持该能力的官方引擎；不要删除原始定义历史。' },
  { codes: ['QZ_SIMULATION_MARKET_UNSUPPORTED', 'SIMULATION_MARKET_UNSUPPORTED'], category: 'runtime',
    message: '当前原生模拟引擎不支持所选市场或合约类型。', action: '核对市场与官方引擎支持范围，明确选择可支持的研究设置。' },
  { codes: ['CATALOG_EMPTY_SELECTION'], category: 'data', message: '原数据目录在所选标的和时间范围内没有可用行情。', action: '核对目录准备结果、标的、时间范围和可得时间；不要以零值补齐缺失行情。' },
  { codes: ['CATALOG_INSTRUMENT_MISSING', 'CATALOG_INSTRUMENT_VERSION_MISMATCH', 'CATALOG_INSTRUMENT_CHANGED', 'CATALOG_EXTRA_INSTRUMENT'], category: 'data',
    message: '行情目录缺少匹配的原始合约定义，或定义与请求不一致。', action: '检查目录中的合约定义及历史版本；重新准备匹配的数据，不要猜测合约参数。' },
  { codes: ['CATALOG_ROOT_INVALID', 'CATALOG_PARQUET_FOOTER_INVALID', 'CATALOG_NATIVE_TYPE_MISMATCH', 'CATALOG_SELECTION_INVALID', 'CATALOG_BAR_TYPE_INVALID',
    'CATALOG_UNREQUESTED_BAR_TYPE', 'CATALOG_TIME_INVALID', 'CATALOG_NONUNIQUE_OR_REVISED_BAR', 'CATALOG_VOLUME_UNDEFINED', 'CATALOG_PRICE_UNDEFINED',
    'CATALOG_OHLC_INVALID', 'CATALOG_PRICE_PRECISION_MISMATCH', 'CATALOG_PRICE_OUTSIDE_INSTRUMENT_BOUNDS', 'CATALOG_AMBIGUOUS_INSTRUMENT_UPDATE'], category: 'data',
    message: '原始行情目录、时间或合约数据不满足科学计算要求。', action: '检查数据准备报告、价格精度、时间顺序和重复／修订记录；保留原数据，不补造数值。' },
  { codes: ['CATALOG_ROW_LIMIT'], category: 'resource', message: '所选行情超出原请求的行数预算。', action: '核对原数据范围与显式行数预算；修改研究输入应另行明确提交，不能重置原试验。' },
  { codes: ['FORECAST_INSUFFICIENT_WARMUP', 'PORTFOLIO_FORECAST_WARMUP'], category: 'data', message: '至少一个标的的历史观测不足以完成指标预热。', action: '核对每个标的的历史长度和慢周期；使用足够的因果历史，不要把预热缺口填零。' },
  { codes: ['FORECAST_NO_PREDICTIONS', 'strategy_no_forecasts', 'strategy_no_targets'], category: 'data', message: '所选区间没有产生可用预测或策略目标。', action: '检查观测覆盖、预热周期和策略决策区间；不把无预测当作零收益。' },
  { codes: ['strategy_observations_missing', 'strategy_market_observations_missing', 'strategy_instrument_bars_missing'], category: 'data',
    message: '策略所需的原始市场观测不完整。', action: '核对每个原标的的行情覆盖与可得时间；不要补造观测或沿用过期行情成交。' },
  { codes: ['SIGNAL_ABI_MISMATCH', 'FEATURE_MODEL_ABI_MISMATCH'], category: 'model', message: '模型导出接口与原生科学计算合同不匹配。', action: '因子模型须导出 predict(f64 × 8) → f64；核对所用模型类型的原生接口，修正后作为新的计数试验提交。' },
  { codes: ['SIGNAL_MODULE_INVALID', 'SIGNAL_REQUIRES_WASM_BINARY', 'SIGNAL_IMPORTS_FORBIDDEN', 'SIGNAL_INSTANTIATION_REJECTED'], category: 'model',
    message: 'Wasm 模型无法由原生引擎加载或实例化。', action: '检查模型结构、内存声明和禁用的导入／启动函数；修正模型后明确提交新试验。' },
  { codes: ['SIGNAL_BUDGET_EXHAUSTED', 'FEATURE_MODEL_BUDGET_EXHAUSTED', 'SIGNAL_PREDICTION_LIMIT', 'SIGNAL_FUEL_LIMIT'], category: 'resource',
    message: '模型的显式科学计算预算不足或无效。', action: '检查模型循环及原请求的燃料／预测预算；不要重置预算或把失败当作免费重算。' },
  { codes: ['SIGNAL_EXECUTION_FAILED', 'FEATURE_MODEL_EXECUTION_FAILED', 'SIGNAL_INSTANCE_FAILED', 'FEATURE_MODEL_INSTANCE_FAILED'], category: 'model',
    message: 'Wasm 模型执行失败，可能触发运行时陷阱、非有限输出或燃料耗尽。', action: '检查模型计算、循环、内存访问和显式燃料预算；现有诊断不能进一步区分时不要断言唯一原因。' },
  { codes: ['SIGNAL_INPUT_NONFINITE', 'FORECAST_LABEL_NONFINITE'], category: 'data', message: '计算输入或收益标签包含非有限数值。', action: '检查原始行情和特征计算，不把 NaN、无穷值或缺失值替换成零。' },
  { codes: ['FEATURE_MODEL_OUTPUT_NONFINITE'], category: 'model', message: '模型输出不是有限数值。', action: '检查除零、溢出和输出定义；修正后另行提交计数试验。' },
  { codes: ['heldout_already_exposed', 'heldout_criteria_mismatch', 'heldout_group_window_mismatch', 'research_already_final'], category: 'request',
    message: '最终评估样本已暴露、研究已结束，或原评估条件不一致。', action: '保留原选择、暴露记录和评估条件；不能通过重启、改名或复制数据重新声明独立样本。' },
  { codes: ['trial_budget_exhausted', 'research_search_scope_exceeded'], category: 'request', message: '本次提案超出原研究的试验预算或搜索范围。', action: '查看原试验账本和搜索约束；失败仍计数，不重置预算。' },
  { codes: ['historical_exposure_required'], category: 'request', message: '历史重评模式缺少其他原研究的已暴露样本记录。', action: '核对原研究的数据绑定与模式；不能自行制造暴露记录或将重评当作独立评价。' },
  { codes: ['trial_identity_conflict', 'strategy_request_binding_mismatch'], category: 'request', message: '同一试验身份对应的提案或原始请求不一致。', action: '核对原请求及提案，保持原身份重放；不能换键重算来掩盖冲突。' },
  { codes: ['strategy_instrument_fee_missing', 'strategy_instrument_fee_duplicate'], category: 'request',
    message: '策略标的缺少唯一匹配的原始手续费／费用准备金设置。', action: '核对所选标的与原研究登记的费用表；不猜测费率或跳过费用约束。' },
  { codes: ['strategy_market_not_allowed', 'strategy_instrument_not_allowed'], category: 'request',
    message: '所选市场或标的不在原研究登记的候选范围内。', action: '核对原始市场／标的标识，并从原研究已登记的市场与标的中选择。' },
  { codes: ['native_partition_boundaries', 'catalog_required', 'native_request_required', 'forecast_required', 'parameters_required', 'selected_trial_not_computed',
    'selected_factor_evaluator_unavailable', 'selected_portfolio_evaluator_unavailable', 'heldout_data_required', 'strategy_policy_mismatch', 'strategy_selection_mismatch',
    'portfolio_weights_required', 'portfolio_weights_invalid', 'strategy_instrument_fee_reserve_required', 'strategy_target_alignment_mismatch',
    'strategy_currency_required', 'strategy_starting_capital_required', 'strategy_adapter_unsupported', 'strategy_bar_types_invalid', 'strategy_base_instrument_not_allowed',
    'strategy_clock_invalid', 'strategy_currency_mismatch', 'strategy_evaluation_partition_invalid', 'strategy_evidence_mode_invalid', 'strategy_forecast_mode_invalid',
    'strategy_forecast_nonfinite', 'strategy_instrument_invalid', 'strategy_instrument_universe_invalid', 'strategy_market_allocations_required', 'strategy_market_duplicate',
    'strategy_market_duplicate_clock', 'strategy_market_policy_required', 'strategy_market_single_choice_invalid', 'strategy_members_required',
    'strategy_threshold_invalid', 'strategy_weight_invalid', 'target_ttl_invalid', 'selected_factor_model_required', 'policy_weight_decimal_required', 'decimal_precision_unsupported',
    'bar_types_required', 'forecast_request_required', 'catalog_selection_required', 'caller_selection_required', 'distinct_research_selections_required'], category: 'request', message: '研究输入、策略映射或所选试验不满足原执行合同。', action: '核对原请求、分区边界、模型选择、权重和策略设置；不自动改写输入或替换试验身份。' },
  { codes: ['native_result_request_mismatch', 'native_summary_request_mismatch', 'native_summary_required', 'native_result_required', 'native_blended_targets_missing',
    'native_shared_account_equity_missing', 'native_money_unsupported', 'native_equity_currency_mismatch', 'native_single_currency_equity_required', 'native_json_multiple_values', 'native_json_value_missing',
    'native_forecast_points_missing', 'native_evidence_part_missing', 'native_evidence_parts_invalid', 'native_command_unsupported'], category: 'runtime',
    message: '原生结果缺失、格式不符合合同，或未对应原请求。', action: '检查服务与科学内核版本及原任务诊断；不能把进程退出成功当作有效结果。' },
  { codes: ['aborted', 'AbortError'], category: 'cancelled', message: '本地科学执行被中断，未产生计算成功结论。', action: '核对原任务终态；宿主暂停应恢复原任务，显式停止不等于远端操作已取消。' },
  { codes: ['ENOSPC', 'EDQUOT', 'ENOMEM'], category: 'resource', message: '本机磁盘、配额或内存不足，无法完成科学执行。', action: '检查本机资源与科学临时目录；保留原任务和数据，不自动发起新的试验。' },
  { codes: ['EACCES', 'EPERM', 'ENOENT', 'ENOTDIR'], category: 'runtime', message: '执行所需的目录、文件或权限不可用。', action: '检查配置的科学程序、数据目录和临时目录权限；修复环境后核对原失败记录。' },
];
const knownReasons = reasons.flatMap(reason => reason.codes.map(code => ({ ...reason, code,
  pattern: new RegExp(`(?:^|\\n|:\\s*|;\\s*)${code}(?=$|[\\s:;])`),
})));

/** Only fixed known reasons are public. Also classify old stderr/WABT strings without rewriting stored facts. */
export function classifyExecutionError(value: unknown, fallback: Fallback = 'scientific_execution_failed', context: ExecutionContext = {}): ExecutionErrorDetail | null {
  if (!(value instanceof Error) && typeof value !== 'string' || !executionDiagnostic(value, '')) return null;
  const raw = executionDiagnostic(value);
  const metadata = value instanceof ExecutionFailure ? { operation: value.operation ?? context.operation, stage: value.stage ?? context.stage } : context;
  const operation = typeof metadata.operation === 'string' && operations.has(metadata.operation) ? metadata.operation : undefined;
  let stage = typeof metadata.stage === 'string' && stages.has(metadata.stage) ? metadata.stage as ExecutionErrorDetail['stage'] : undefined;
  const found = knownReasons.find(reason => reason.pattern.test(raw));
  let detail: ExecutionErrorDetail;
  if (/^(?:parseWat|validate|toBinary) failed:/m.test(raw)) {
    stage = 'compile'; detail = { code: 'wasm_compile_failed', category: 'model', message: '模型 WAT 编译或类型校验未通过。', action: '检查括号、指令、参数／返回类型和内存声明；原失败保留并计数，修正后另行提交新试验。' };
  } else if (found) {
    detail = { code: found.code, category: found.category, message: found.message, action: found.action };
  } else if (stage === 'compile') {
    detail = { code: 'wasm_compile_failed', category: 'unknown', message: '模型 WAT 编译未完成，现有记录无法确定具体原因。', action: '检查模型输入、编译环境和原任务诊断；不自动重算本次失败试验。' };
  } else if (stage === 'decode') {
    detail = { code: 'native_result_invalid', category: 'runtime', message: '原生程序输出不是完整有效的科学结果。', action: '检查科学程序版本与原任务诊断；不要使用截断或格式错误的结果。' };
  } else if (/research contract fields are invalid/.test(raw)) {
    stage ??= 'validate'; detail = { code: 'native_request_invalid', category: 'request', message: '原生科学请求中的字段不满足合同要求。', action: '核对原请求字段、数值范围和分区设置；详细字段诊断保留在原任务中。' };
  } else if (/^qz-(?:native|factor):.*(?:No such file or directory|not found|Permission denied)/m.test(raw)) {
    detail = { code: 'native_executable_unavailable', category: 'runtime', message: '配置的原生科学程序无法启动。', action: '检查科学程序是否已安装、配置正确且可执行；不要以替代输出伪造成功。' };
  } else {
    detail = { code: /^QZ_NATIVE_JOB_FAILED(?:\s|$)/.test(raw) ? 'QZ_NATIVE_JOB_FAILED' : fallback, category: 'unknown',
      message: '科学执行失败，现有记录无法确定具体原因。', action: '请凭原研究、试验或任务编号检查私有诊断；不要另建身份自动重算。' };
  }
  if (!stage && (found || detail.code === 'QZ_NATIVE_JOB_FAILED') && /^(?:QZ_|[A-Z][A-Z_]+(?:\s|:|$))/.test(raw)) stage = 'execute';
  return { ...detail, ...(operation ? { operation } : {}), ...(stage ? { stage } : {}) };
}
