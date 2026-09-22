from pathlib import Path


def replace(path: str, old: str, new: str) -> None:
    target = Path(path)
    text = target.read_text()
    assert text.count(old) == 1, (path, old)
    target.write_text(text.replace(old, new))


replace('apps/job/tests/support/polymarket.rs',
    '    catalog.write_to_parquet(&closes, None, None, None).unwrap();',
    '''    for close in closes {
        catalog.write_to_parquet(&[close], None, None, None).unwrap();
    }''')
replace('apps/job/tests/polymarket.rs',
    '''        return Err(String::from_utf8_lossy(&output.stderr).into_owned());''',
    '''        // Keep the real CLI failure; inspect only this synthetic fixture in-process
        // for a useful test diagnostic. The second call cannot turn failure into success.
        let detail = job::simulation::simulate(root, request)
            .err()
            .map(|error| format!("{error:#}"))
            .unwrap_or_else(|| "CLI/library result mismatch".into());
        return Err(format!("{}: {detail}", String::from_utf8_lossy(&output.stderr)));''')
replace('apps/job/tests/polymarket.rs',
    '''    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: NativePortfolioStudyResultV1''',
    '''    if !output.status.success() {
        let detail = job::study::evaluate(catalog.path(), &request, |id| {
            Ok(fs::read(objects.path().join(id.to_string()))?)
        }).err().map(|error| format!("{error:#}"));
        panic!("{}; fixture diagnostic: {detail:?}", String::from_utf8_lossy(&output.stderr));
    }
    let result: NativePortfolioStudyResultV1''')
