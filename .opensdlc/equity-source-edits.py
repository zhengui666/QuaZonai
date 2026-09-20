"""Apply only the web author's exact integration edits to the pinned source."""
from pathlib import Path
import sys

root = Path(sys.argv[1])
def edit(path, old, new):
    target = root / path
    content = target.read_text()
    if content.count(old) != 1:
        raise SystemExit(f"Expected one exact anchor in {path}: {old!r}; found {content.count(old)}")
    target.write_text(content.replace(old, new, 1))

def append(path, text):
    target = root / path
    target.write_text(target.read_text().rstrip() + "\n\n" + text.strip() + "\n")

edit('crates/contracts/src/lib.rs', 'pub mod evidence;\n', 'pub mod equity_curve;\npub mod evidence;\n')
edit('crates/contracts/src/lib.rs', '    http::Problem,\n', '    http::Problem,\n    equity_curve::EquityCurveQuery,\n    equity_curve::EquityCurveV1,\n')
edit('crates/domain/src/execution/output.rs', 'mod simulation;\n', 'mod simulation;\nmod equity_curve;\npub use equity_curve::{equity_curve_query, portfolio_equity_curve};\n')
edit('crates/domain/src/execution.rs', 'pub use output::{\n', 'pub use output::{\n    equity_curve_query, portfolio_equity_curve,\n')
edit('crates/domain/src/execution/output/simulation.rs', 'fn native_count(', 'pub(super) fn native_count(')
edit('crates/domain/src/execution/output/simulation.rs', 'fn money(', 'pub(super) fn money(')
edit('crates/store/src/evidence.rs', "type Tx<'a> = Transaction<'a, Postgres>;", "mod equity_curve;\ntype Tx<'a> = Transaction<'a, Postgres>;")
edit('apps/server/src/lib.rs', 'pub mod evidence;\n', 'pub mod equity_curve;\npub mod evidence;\n')
edit('apps/server/src/lib.rs', '.route("/api/v2/evaluations/{id}/metrics", get(evidence::metrics))', '.route("/api/v2/evaluations/{id}/metrics", get(evidence::metrics))\n        .route("/api/v2/evaluations/{id}/equity-curve", get(equity_curve::get))')
edit('apps/server/src/lib.rs', 'evidence::evaluation,evidence::metrics,', 'evidence::evaluation,evidence::metrics,equity_curve::get,')
edit('apps/web/src/alphas.tsx', "import { useRef, useState } from 'react';", "import { lazy, Suspense, useRef, useState } from 'react';")
edit('apps/web/src/alphas.tsx', 'Modal, Space, Table, Typography', 'Modal, Skeleton, Space, Table, Typography')
edit('apps/web/src/alphas.tsx', "type Alpha = Schema['AlphaView'];", "const EquityCurve = lazy(() => import('./equity-curve'));\n\ntype Alpha = Schema['AlphaView'];")
edit('apps/web/src/alphas.tsx', '        <Alert showIcon type="info" title="这是历史科学证据，不是资格或交付批准。"', '        {candidate && !query.isError && value.evaluation_kind === \'PORTFOLIO\' && <Suspense fallback={<Skeleton active />}><EquityCurve key={value.id} evaluation={value} /></Suspense>}\n        <Alert showIcon type="info" title="这是历史科学证据，不是资格或交付批准。"')
edit('crates/store/tests/support/qualified_portfolio.rs', '#[path = "approval_checks.rs"]', '#[path = "equity_curve_checks.rs"]\npub(super) mod equity_curve_checks;\n\n#[path = "approval_checks.rs"]')
edit('crates/store/tests/support/equity_curve_checks.rs', 'use super::*;', '#![allow(dead_code)]\nuse super::*;')
edit('crates/store/tests/support/qualified_portfolio.rs', '        let view = store.evaluation(actor, left.resource).await.unwrap();\n        let releases:', '        let view = store.evaluation(actor, left.resource).await.unwrap();\n        Box::pin(equity_curve_checks::check_projection(store, actor, f, left.resource, candidate, infeasible)).await;\n        let releases:')
edit('apps/server/tests/portfolio_study_http.rs', '#[path = "support/automatic_live.rs"]', '#[path = "support/equity_curve.rs"]\nmod equity_curve;\n#[path = "support/automatic_live.rs"]')
edit('apps/server/tests/portfolio_study_http.rs', '        evaluation.decision,\n        contracts::evidence::Decision::Inconclusive\n    );\n    listener.abort_all();', '        evaluation.decision,\n        contracts::evidence::Decision::Inconclusive\n    );\n    Box::pin(equity_curve::verify((pool, store, actor, f), (&client, &origin, &token), build, candidate, evaluation.id)).await;\n    listener.abort_all();')
append('docs/research/reuse.md', '''## Historical portfolio equity

The equity view reuses Apache ECharts (Apache-2.0), `echarts-for-react/lib/core` (MIT), native Nautilus total-equity snapshots and Ant Design controls. Exact versions are in `apps/web/package-lock.json`. Only Line, Grid, Tooltip, DataZoom, Aria and Canvas are registered; no competing chart engine or first-party renderer is added. View bucketing selects existing native observations and is never used for financial metrics. Rendering compatibility and performance are verified by repository tests, not inferred from upstream peer ranges or benchmark claims.

Upstream references: [ECharts](https://github.com/apache/echarts), [React adapter](https://github.com/hustcc/echarts-for-react), [modular imports](https://echarts.apache.org/handbook/en/basics/import/).''')
append('OPERATIONS.md', '''## 查看历史组合价值

在“组合 → 候选快照 → 评估”打开已发表的 PORTFOLIO 历史回测，价值图读取原生模拟总权益。区间控件按本地时间选择，图与明细使用 UTC；金额币种显示在标题中。自动粒度在原始、日末、周末和月末中选择最多 10000 个点的视图。“完整区间”恢复全段；更精细的长区间超限时请缩小范围。图表缩放与明细查看不启动回测。

旧回测没有快照、未产生模拟或运行失败时会显示具体状态，不补造价值。科学 REJECT 或资格过期不删除有效的历史观测。日/周/月末视图不是完整日内极值，不能据其重新计算风险指标。业务数据不进入 PWA 持久缓存，离线时需恢复连接再读取。''')
print('Applied exact authored contract, route, UI, test and documentation integration')
