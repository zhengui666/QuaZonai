from pathlib import Path
exec(Path('.github/personal-lean.py').read_text().split('# Compile one immutable')[0])
replace('DESIGN.md', '.github/               原生 CI、贡献模板和 CODEOWNERS', '.github/               原生 CI 与贡献模板')
replace('DESIGN.md', 'AUTH_RATE_LIMITED 的原有限流重试语义不变。', 'CRYPTO_BUSY 表示当前验证计算槽已满，可稍后重试；不再存在持久认证限流窗口。')
replace('DESIGN.md', '原生校验、deterministic、extra-checks、严格编译结构限制和fuel', '原生校验、deterministic、严格编译结构限制和fuel（extra-checks 按第0.6节移除）')
p='CONTRIBUTING.md'
s=read(p); a=s.index('## Choose a change'); b=s.index('## Set up a checkout',a)
s=s[:a]+'''## Choose a change

Work from the owner's concrete request or an existing Issue. No separate proposal, approval ledger or mandatory form is needed. Never post credentials, private data or hidden model reasoning.

'''+s[b:]
s=s.replace('Fork the repository, clone your fork, add the upstream below and create a branch from current main.', 'Create a branch from current main; a fork is optional.')
a=s.index('The native browser flow additionally needs'); b=s.index('`check-unit` excludes',a)
s=s[:a]+'''The native browser checks require the actual systemd user manager, disposable PostgreSQL, Caddy and built server/web artifacts. Use the exact [Web workflow prerequisites](.github/workflows/web.yml), not a second setup copied here. The fixture runs real API/Worker/gateway services and checks restart, session and receipt persistence; it is not real market-research or host-boot acceptance.

'''+s[b:]
s=s.replace('Continue this delivery in the existing [personal-production task](.opensdlc/tasks/personal-production/task.md); other work reuses its own relevant task instead of writing into the old onboarding record.', 'Reuse the relevant task record; do not append new work to an unrelated historical task.')
write(p,s)
