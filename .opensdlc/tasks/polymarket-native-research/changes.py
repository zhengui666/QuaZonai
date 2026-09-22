from pathlib import Path


def replace(path, old, new):
    p = Path(path)
    text = p.read_text()
    assert text.count(old) == 1, (path, old)
    p.write_text(text.replace(old, new))


p = Path('apps/web/tests/cost-budget.spec.ts')
s = p.read_text()
assert s.count("getByLabel('基础币种（ISO 4217）')") == 2
s = s.replace("getByLabel('基础币种（ISO 4217）')", "getByLabel('基础币种', { exact: true })")
s += '''
for (const currency of ['USDC', 'USDC.e', 'pUSD']) {
  test(`research collateral ${currency} survives browser dispatch without changing model billing`, async ({ page }) => {
    const state = await editor(page, { ...initialBudget, cost_enforcement: 'ESTIMATED', max_cost_decimal: '12.50', cost_currency: 'USD' });
    await page.getByLabel('基础币种', { exact: true }).fill(currency);
    await expect(page.getByLabel('基础币种', { exact: true })).toHaveValue(currency);
    await save(page).click();
    await expect.poll(() => state.commands.filter(command => command.method === 'PATCH').length).toBe(1);
    expect(state.commands.find(command => command.method === 'PATCH')?.body).toMatchObject({
      content: { base_currency: currency, budget: { cost_enforcement: 'ESTIMATED', max_cost_decimal: '12.50', cost_currency: 'USD' } },
    });
  });
}
'''
p.write_text(s)

replace('apps/web/demo/records.ts',
    '''record(`/api/v2/evaluations/${demoEvaluation.id}/metrics`, '/api/v2/evaluations/{id}/metrics', page([]));''',
    '''record(`/api/v2/evaluations/${demoEvaluation.id}/metrics`, '/api/v2/evaluations/{id}/metrics', page([]));
// This scene has presentation history but no executed simulation or equity snapshots.
// Use the real unavailable-response contract rather than inventing an equity curve.
record(`/api/v2/evaluations/${demoEvaluation.id}/equity-curve`, '/api/v2/evaluations/{id}/equity-curve', {
  schema_version: 1, project_id: project.id, candidate_id: demoCandidate.id,
  evaluation_id: demoEvaluation.id, run_id: demoEvaluation.run_id, origin: 'FIXTURE',
  curve: { status: 'UNAVAILABLE', reason_code: 'NO_SIMULATION' },
} satisfies Schema['EquityCurveV1']);''')
replace('apps/web/tests/demo-history.spec.ts',
    '''  await candidate.getByRole('button', { name: '评估 00000506', exact: true }).click();''',
    '''  const equityResponse = page.waitForResponse(response =>
    new URL(response.url()).pathname === '/api/v2/evaluations/01990000-0000-7000-8000-000000000506/equity-curve');
  await candidate.getByRole('button', { name: '评估 00000506', exact: true }).click();''')
replace('apps/web/tests/demo-history.spec.ts',
    '''  await expect(evaluation.getByText('PORTFOLIO', { exact: true })).toBeVisible();''',
    '''  await expect(evaluation.getByText('PORTFOLIO', { exact: true })).toBeVisible();
  const equity = await equityResponse;
  expect(equity.status()).toBe(200);
  expect(await equity.json()).toMatchObject({
    origin: 'FIXTURE', curve: { status: 'UNAVAILABLE', reason_code: 'NO_SIMULATION' },
  });
  await expect(evaluation.getByText('本次研究未产生权益数据', { exact: true })).toBeVisible();''')

replace('apps/runtime/tests/native_oci.rs',
    '''        request.mandate.base_currency = "pUSD".into();''',
    '''        request.mandate.base_currency = "pUSD".into();
        // Ten million shares at about .4 provide about four million collateral
        // units of bar notional. A 40% participation bound cannot invest the FX
        // fixture's ten million capital. Use a feasible one-million account,
        // retaining the original participation limit and all solver checks.
        request.mandate.capital_assumption = "1000000".parse().unwrap();
        request.execution_settings.starting_capital = request.mandate.capital_assumption.clone();''')
