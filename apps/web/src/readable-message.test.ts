import { describe, expect, it } from 'vitest';
import { ApiFailure } from './api';
import { readableMessage } from './readable-message';

describe('readable server diagnostics', () => {
  it('preserves ordinary messages and non-data braces', () => {
    expect(readableMessage('请重新载入。')).toBe('请重新载入。');
    expect(readableMessage('模板 {name} 尚未配置')).toBe('模板 {name} 尚未配置');
  });
  it('presents a complete structured error with exact diagnostic values', () => {
    const result = readableMessage('{"message":"版本已更新","current_revision":9007199254740993,"cost":1.2300000000000000001}');
    expect(result).toBe('说明：版本已更新；当前版本：9007199254740993；cost：1.2300000000000000001');
    expect(result).not.toMatch(/[{}\[\]"]/);
  });
  it('converts nested, escaped and fenced payloads in error prose', () => {
    expect(readableMessage('执行失败：{"error":{"message":"参数错误","fields":["limit","budget"]}}，请重试。'))
      .toBe('执行失败：错误：说明：参数错误；fields：limit、budget，请重试。');
    expect(readableMessage('```json\n{"detail":"请重新载入"}\n```')).toBe('详情：请重新载入');
    expect(readableMessage('{"detail":"{\\"message\\":\\"嵌套错误\\"}"}')).toBe('详情：说明：嵌套错误');
    expect(readableMessage('失败："{\\"detail\\":\\"已失效\\"}"')).toBe('失败：详情：已失效');
    expect(readableMessage('运行 ```bash\nretry --once\n``` 后重试。')).toBe('运行 ```bash\nretry --once\n``` 后重试。');
  });
  it.each(['jsonl', 'ndjson', 'json'])('presents line-separated %s diagnostics without losing records', language => {
    expect(readableMessage(`\`\`\`${language}\n{"message":"第一项"}\n{"message":"第二项"}\n\`\`\``))
      .toBe('说明：第一项；说明：第二项');
  });
  it('keeps unknown object keys readable when they overlap inherited properties', () => {
    expect(readableMessage('{"__proto__":"原始字段","constructor":"构造说明","toString":"文本说明"}'))
      .toBe('__proto__：原始字段；constructor：构造说明；toString：文本说明');
  });
  it.each([
    '{"message":"已失效","current_revision":9007199254740993}',
    '失败："{\\"detail\\":\\"已失效\\"}"',
    '```jsonl\n{"message":"第一项"}\n{"message":"第二项"}\n```',
    '模板 {name} 尚未配置',
  ])('does not change diagnostic meaning when formatted twice: %s', message => {
    const once = readableMessage(message);
    expect(readableMessage(once)).toBe(once);
  });
  it('does not print malformed or incomplete structured fragments', () => {
    expect(readableMessage('失败：{"detail":')).toBe('失败：结构化详情无法显示，请核对操作结果。');
    expect(readableMessage('[01]')).toBe('结构化详情无法显示，请核对操作结果。');
    expect(readableMessage('```json\n{"secret":"unfinished')).not.toContain('secret');
  });
  it('normalizes ApiFailure without mutating its authoritative Problem', () => {
    const detail = '{"message":"请刷新"}';
    const problem = {
      type: 'urn:quazonai:problem:revision-conflict', title: 'REVISION_CONFLICT', status: 409,
      code: 'REVISION_CONFLICT', detail, request_id: '01990000-0000-7000-8000-000000000001',
      retryable: false, current_revision: '9007199254740993', field_errors: [], safe_next_actions: ['RELOAD'],
    };
    const failure = new ApiFailure('REVISION_CONFLICT', detail, 409, problem);
    expect(failure.message).toBe('说明：请刷新');
    expect(failure.code).toBe('REVISION_CONFLICT');
    expect(failure.problem).toBe(problem);
    expect(failure.problem?.detail).toBe(detail);
  });
});
