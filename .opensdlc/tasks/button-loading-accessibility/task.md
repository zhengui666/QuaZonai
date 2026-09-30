# Button loading accessibility

## Observed failure

[PR 145's exact-head browser run](https://github.com/zhengui666/QuaZonai/actions/runs/36787369821/job/110131895243)
at `895ad435` observed `loading 原样重试上传请求` as the upload retry button's
accessible name for 15 seconds after its loading class had disappeared. Both
lost-ACK and unavailable receipt-validator cases failed. The scoped confirmation
dialog corrections passed; this is a separate decorative-icon/name defect.

## Correction

Use the installed official Ant Design `ConfigProvider` button `loadingIcon`
property with official `LoadingOutlined aria-hidden`. This single application
configuration keeps decorative glyphs out of every button's accessible name,
including the fast loading-to-error transition. Visible labels, mutation guards,
disabled states, idempotency identity and retry behavior stay unchanged. Explicit
`aria-busy` conveys upload/read and download activity without renaming actions.
No upstream source, timeout, locator actionability or assertion is patched.

## Verification

- Two native React/AntD server-rendering checks cover hidden busy glyphs and
  unchanged idle text; all 568 web unit tests pass
- TypeScript, production/PWA build and all 350 tracked response-contract drift
  checks pass
- Original immediate upload failures remain. Browser cases additionally exercise
  a held upload and held DATA_VALIDATE request through pending/error/retry, with
  exact accessible-name, busy-state and original-body/key assertions
- Real native download acceptance continues to compare original Runtime bytes
  and provenance; this change does not create a scientific result
- Hosted browser, native service/PWA and complete final-head CI remain required
  before merge. Local browser execution is unavailable in this executor
