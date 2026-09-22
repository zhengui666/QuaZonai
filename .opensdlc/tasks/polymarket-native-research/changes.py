from pathlib import Path

path = Path('apps/job/tests/simulation.rs')
source = path.read_text()
old_name = 'fn native_binary_options_do_not_produce_portfolio_results_before_or_after_expiry() {'
assert source.count(old_name) == 1
source = source.replace(old_name, 'fn binary_options_without_native_fee_contract_are_rejected_before_and_after_expiry() {')
old = '''            "SIMULATION_MARKET_UNSUPPORTED"
        );
        let output = native::command('''
new = '''            "POLYMARKET_NATIVE_FEE_MODEL_REQUIRED"
        );
        // A BinaryOption alone is not sufficient: the native venue, original
        // collateral, fee schedule and lifecycle remain mandatory. Valid native
        // Polymarket contracts are exercised separately in tests/polymarket.rs.
        let output = native::command('''
assert source.count(old) == 1
path.write_text(source.replace(old, new))
