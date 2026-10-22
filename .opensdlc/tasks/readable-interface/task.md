# Readable interface data

## Task

Remove raw serialized data from the QuaZonai interface and push the verified change to the remote, as requested by the owner on 2026-10-22. This task does not include merging or deployment.

## Behavior

- Display saved portfolio, simulation, evaluation and automation configuration, original build requests, migration inspection metadata and primary keys as readable fields and ordered lists.
- Present complete serialized historical fields without raw syntax, preserving exact integer and decimal text. Keep original field downloads available. Bounded field segments show their position and a download action instead of an incomplete data dump.
- Keep useful error explanations, request identities and recovery actions visible while preventing serialized error payloads from appearing in the interface.
- Retain wire formats, generated contracts, research rules and mutation behavior.

## Verification

`make check-web` passed: generated contracts unchanged, TypeScript passed, all 567 unit tests passed and the production/PWA build succeeded. An independent review found no remaining issues.

The six Chromium checks in `apps/web/playwright.no-json.config.ts` passed against synthetic, contract-validated HTTP fixtures at 1280px and 390px. They verify nested policy criteria, exact numeric values, migration primary keys and complete/line-delimited/malformed/partial fields, original text downloads, error explanations and absence of document overflow. The mobile migration check also passed three independent repeats after the test waited for navigation focus restoration and used the Select's supported arrow keys.

The browser checks exercise the production interface with synthetic HTTP data; they do not establish native backend or deployment acceptance.

## Delivery

Remote delivery target: [codex/no-json-ui](https://github.com/zhengui666/QuaZonai/tree/codex/no-json-ui).
