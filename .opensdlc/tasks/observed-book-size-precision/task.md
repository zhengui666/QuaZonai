# Observed L2 quantity precision

Task ID: observed-book-size-precision. Allow the explicit `OBSERVED_L2_IOC_V2` execution path to consume observed book sizes that exceed the currently declared instrument precision. Floor each size to the current `size_increment`, retain source rows and clocks, and omit `Add` levels that floor to zero. Keep V1's strict precision behavior unchanged. Current instrument increments are a proxy until historical point-in-time rules are available.

Delivery: preserve the V2 request, result, and queued metadata label; review the exact source change and merge only after the repository's applicable checks pass. This adapter change does not establish data coverage, historical instrument rules, strategy performance, or qualification.
