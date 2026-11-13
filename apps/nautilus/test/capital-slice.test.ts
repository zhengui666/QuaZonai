import assert from 'node:assert/strict';
import test from 'node:test';
import * as capital from '../src/capital-account.ts';
test('retired slice and fee computation exports are absent',()=>{assert.equal('planCapitalSlice' in capital,false);assert.equal('capitalFillLoss' in capital,false);});
