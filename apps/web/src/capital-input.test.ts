import { expect,it } from 'vitest';
import * as input from './capital-input';
it('retired planner and slice helpers are absent',()=>{for(const name of ['capitalPreviewInput','suggestedCapitalSlice','capitalResumePreviewMatches'])expect(name in input).toBe(false);});
