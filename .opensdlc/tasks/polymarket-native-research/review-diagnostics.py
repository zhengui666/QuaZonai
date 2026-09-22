from pathlib import Path

p = Path('apps/job/tests/polymarket.rs')
s = p.read_text()
old = '        let result = job::portfolio::build(catalog.path(), &request, |id| {'
assert s.count(old) == 1
s = s.replace(old, '''        domain::execution::portfolio_build_request(&request)
            .unwrap_or_else(|error| panic!("expiry={expiration}; fixture contract: {error:?}; request={request:?}"));
        let result = job::portfolio::build(catalog.path(), &request, |id| {''')
old = '            let error = result.unwrap_err().to_string();'
assert s.count(old) == 1
s = s.replace(old, '''            let failure = result.unwrap_err();
            let error = format!("{failure:?}; domain={:?}", failure.downcast_ref::<domain::DomainError>());''')
p.write_text(s)
