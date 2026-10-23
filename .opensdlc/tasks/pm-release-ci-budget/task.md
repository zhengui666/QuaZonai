# PM 发布完整验收的运行预算

## 目标与基线

本任务继续完成 PM 平台的正式 Dev 发布交付。基线为最新远程 dev `4e20fa156b488819413840ebebbbe43d9c791893`，来自 [PR #233](https://github.com/zhengui666/QuaZonai/pull/233)。业务实现、科学方法、账户约束和测试范围保持原版本；本任务只调整 CI/发布等待的运行预算及现有超时用例边界。

## 实际证据

- 最终 PR Head `ed16632094f126c9874b7376fcf3369a0bac59f9` 的 [成功 CI](https://github.com/zhengui666/QuaZonai/actions/runs/39865701788) 中，science-core 作业实际用时 59分02秒；debug 四程序构建 7分16秒，科学/core 42分16秒，optional history 7分39秒。科学 676/676、core 91/91 全过，零忽略/跳过。
- 合并提交的 [push CI](https://github.com/zhengui666/QuaZonai/actions/runs/39872975121) 使用 release profile。四程序构建 23:37:50→00:09:54 UTC，用时 32分04秒；科学检查实际 676/676 全过，零失败/忽略。
- 同一作业 2026-10-22 23:37:21→2026-10-23 00:37:37 UTC，共 60分16秒后被取消。core 只完成 #1–#55；PAPER 场景正在运行，无完整 TAP 汇总。作业 API 为 `cancelled`，日志为 `The operation was canceled`；注释正文未能读取，不能声称已获得明确的超时注释或取消者身份。
- 该时长与原 60 分钟上限吻合。现有成功检查本已接近上限，本次 release 构建又明显更长，因此需要提高完整验收的运行预算。不同 profile 的实际总耗时仍须新运行确认。
- 被取消的 CI 未上传任何 artifact；[对应 Dev release](https://github.com/zhengui666/QuaZonai/actions/runs/39872975238) 的 checks 失败、publish 跳过。旧标签存在不构成发布成功。

## 修改与不变量

| 边界 | 原预算 | 新预算 |
| --- | --- | --- |
| science-core 作业 | 60 分钟 | 90 分钟 |
| 精确提交 CI 等待函数 | 85 分钟 | 115 分钟 |
| Dev release checks 外层作业 | 90 分钟 | 120 分钟 |

等待函数比科学作业上限多 25 分钟，外层 checks 比等待函数多 5 分钟；这是运行资源预算，不是生产 SLO，也不保证跨 job 排队永远不会超时。现有 deadline 测试使用新的真实边界，仍验证有限等待。

完整科学/core/optional 检查、无跳过前置条件、同一 SHA/push/正式路径的最新 CI 选择、失败立即拒绝、成功快照 run ID、独立产物来源复验以及 `tested-revision` 检查全部保留。业务时钟、目标 TTL、资金精度、试验预算和方法常数均不改变。缓存策略不改变；被取消作业不能被当作拥有可恢复的已测试发布产物。

## 验收与交付

本地运行现有 release 单元检查、workflow 检查及 diff 静态检查，记录实际结果。最终 Head 仍须全部适用 CI、独立审查、GitHub Codex 明确无问题后合并远程 dev。

合并后跟踪新 merge SHA 的 push CI 和 Dev release 至终态，核对该 CI 的原生产物、新标签、schema4 manifest、三个固定 OCI digest 及 12 项 uploaded/非空的正式资产。旧取消运行和部分 core 结果保留原证据；没有正式发布前不记为运维完成。正式发布也不等于用户安装已升级。

相关业务与原运维边界见 [PM 平台任务](../pm-adaptive-platform/task.md) 和 [运维手册](../pm-adaptive-platform/operations.md)。真实模型授权、经济容量识别和 SHADOW 晋级上限沿用原边界。
