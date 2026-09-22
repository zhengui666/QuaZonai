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

p = Path('apps/runtime/tests/catalog_scope.rs')
s = p.read_text()
old = '''            let one: contracts::DecimalValue = "1".parse().unwrap();'''
assert s.count(old) == 1
s = s.replace(old, '''            // The allocation fixture's default asset IDs are not the registered
            // catalog's IDs. Bind all three views to the actual native selection;
            // unregistered selections still fail the independent catalog checks.
            for ((asset, fee), name) in request.assets.iter_mut()
                .zip(&mut request.execution_settings.fee_rates)
                .zip(&request.selection.bar_types)
            {
                let id = name.parse::<nautilus_model::data::BarType>().unwrap()
                    .instrument_id().to_string();
                asset.instrument_id = id.clone();
                fee.instrument_id = id;
            }
            for (weight, asset) in request.current_weights.weights.iter_mut().zip(&request.assets) {
                weight.instrument_id = asset.instrument_id.clone();
            }
            let one: contracts::DecimalValue = "1".parse().unwrap();''')
p.write_text(s)
