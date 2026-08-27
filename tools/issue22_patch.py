from __future__ import annotations

from pathlib import Path

path = Path("backend/tests/frontend_e2e_seed.py")
content = path.read_text(encoding="utf-8")
old = '''        session.add_all(
            [universe, data_source, discovery, mandate, paper, live, portfolio_program]
        )
        session.flush()
'''
new = '''        # DatasetRevision carries scalar FK identifiers rather than ORM
        # relationships, so make the governed parents durable before the
        # revision is flushed on PostgreSQL.
        session.add_all([universe, data_source])
        session.flush()
        session.add_all([discovery, mandate, paper, live, portfolio_program])
        session.flush()
'''
if content.count(old) != 1:
    raise RuntimeError(f"expected one E2E seed insertion block, found {content.count(old)}")
path.write_text(content.replace(old, new, 1), encoding="utf-8")
