# Retained native account transport and Paper continuation

Continue the existing source account observation and native Paper work on current producer delivery. This slice adds the retained CLI relay, official node observer and optional native Sandbox Paper host behind the existing job executable. Existing native/release build commands enable that feature; no standalone installed binary is added.

The account route replays transactionally from source/session, sequence/event and the complete native envelope. It requires no additional idempotency header and the relay computes no digest. All other CLI command idempotency remains unchanged. The retained file is the only recovery source; retries preserve original bytes and no second durable cursor is created.

The original direct_research::direct_native_strategy_paper_account_readback test now continues the same producer database, vault and TCP listener through actual native Paper, six observations and replayed original receipts/current/history. Native fills, fees, balances, clocks and shutdown assertions precede retained output publication. This controlled fixture does not establish live market connectivity, instrument qualification or strategy profitability.

Source formatting and ordinary whitespace checks have passed. This current integration has not compiled or run. Next: actual relay unit/subprocess tests, native observer/Sandbox tests, V1/V2 Paper targets and the explicitly selected ignored same-session runtime test using rebuilt current source. Preserve actual cleanup; no SHA or additional validation framework.
