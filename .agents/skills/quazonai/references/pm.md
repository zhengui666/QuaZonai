# 保留的历史查询与控制

新研究使用 `research` 和原生参数扫描，不新建旧 PM mandate、campaign、因子或组合。先看实际 `quazonai --help`、`pm capabilities`，不可把仍存在的历史类型当可创建能力。

保留的读取包括 `pm list/get/decision/history/events`、`campaign get/trials/compare`、`strategy versions/assessments`、`pm cycle/capacity`。仅使用已有对象真实 ID，空结果不补造。

既有对象的 retire、pause、refresh-data、recompute、strategy resume 等只按服务实际能力和用户授权执行，不恢复退役的 BUILD/STUDY 科研。先读取所属 mandate 的当前 revision，再依相应 schema 形成原请求。旧目标接口不能用于仅原生策略账户；不能把历史 PM strategy resume 当 `account-strategy start`。

账户启动/停止请用 [账户与资金](accounts.md)。停止历史任务、隐藏对象与资金清仓是不同动作；保留账户资金、持仓、订单和原回执，不通过删除数据修复失败。
