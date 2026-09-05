# CLI 实现状态

完整目标CLI及权限合同只在DESIGN附录B；本文件不把未实现的命令写成可运行。

## 当前可运行

- `cargo run --locked -p job -- verify-native --output NEW_DIRECTORY`：固定Rust Clarabel/Nautilus/Arrow fixture；目录必须不存在；不会读取用户数据或交付目标包。
- `cargo run --locked -q -p contracts --example generate`：原生生成当前DTO OpenAPI，尚无HTTP endpoints。
- `cargo run --locked -p job --example codex_contract`：需CODEX_NATIVE_BIN和不存在的CODEX_PROBE_DIR；仅原生stdio/模型/Thread兼容性，使用新空profile。

README给出完整可执行参数。没有兼容的旧Python CLI；旧命令已撤下。Operator-only命令、正式数据/项目/审批管理及MCP是仍需完成的实现项，不允许绕过API用手工SQL冒充完成。
