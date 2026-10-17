# Research experience: component-level requirements and design

<a id="basis"></a>
## 1. Basis, scope and evidence limits

The [task](task.md#intent) authorizes understanding the single researcher's needs, replacing the UI/frontend from first principles, changing backend presentation contracts where necessary, and delivering through a remote `dev` merge. The owner's subsequent clarification requires a component-level, highly detailed design grounded in UI/product research. The delivered interface must contain no long explanatory or promotional passages.

This specification replaces the initial high-level proposal. It does not use existing frontend layouts, components, styles, screenshots or historical UI designs as references. Domain behavior comes from the current [architecture](../../architecture.md), [source contracts](../../../packages/contracts/src/index.ts), TypeScript service/research implementation and retained native scientific contracts. Integration/test infrastructure is reusable; old presentation is not the design basis.

**Evidence limit:** This is desk research, source-code/domain analysis and a proposed component design. No interview, contextual observation or usability test with the owner has occurred. External research supports design hypotheses; it does not prove this workbench is usable, profitable or preferred by its owner. Browser/heuristic checks and owner acceptance must be reported separately from human task testing.

**Product boundary:** A single local quantitative researcher delegates finite studies, inspects original trials and independent evidence, and may consume results in optional shared-account simulations and target-only downstream observation. QZ does not hold brokerage credentials or place financial orders. Do not add collaboration, multi-user administration, trading controls, a second scheduler, a generic policy editor or a plugin marketplace.

**Visual revision 2:** The owner rejected the first detailed prototype as insufficiently refined and overly rough. This revision replaces its green-tinted administrative shell, repeated outlined containers and fallback-only typography. The current direction is a precision research document: neutral paper/graphite surfaces, restrained teal links/selection, a single working-document surface and deliberate typography.

**Design Read:** A professional research workbench for a single quantitative researcher, with a precise, refined utilitarian language. Official Ant Design primitives provide interaction behavior; Geist/Geist Mono, one Phosphor icon family and application-specific semantic composition provide the visual identity.

`DESIGN_VARIANCE: 5`, `MOTION_INTENSITY: 3`, `VISUAL_DENSITY: 6`. Moderate list/detail asymmetry supports comparison; density serves working information; motion is restricted to direct feedback. The marketing skill's hero, stock-photography, logo-wall and scroll-storytelling rules are inapplicable to these product surfaces.

<a id="research"></a>
## 2. Public research and what changes because of it

Sources were retrieved on **2026-10-17**. The catalog contains **13 sources**: six NN/g application/UX articles, one peer-reviewed HCI paper, two product/developer survey publications, two W3C accessibility explanations, and two official Ant Design specifications. Summaries below are analysis, not copied reports. Full third-party reports are not vendored into this repository.

### 2.1 Source catalog

| ID | Source and inspected scope | Finding used here | Limits |
| --- | --- | --- | --- |
| S1 | Jakob Nielsen, [Progressive Disclosure](https://www.nngroup.com/articles/progressive-disclosure/), 2006. Full article. | Show frequent, important actions first. Secondary detail must be discoverable and labelled. More than two disclosure levels makes orientation harder. Separate progressive disclosure from mandatory linear wizards. | General application guidance, not a measured experiment on QZ. It does not mean hiding prerequisite errors. |
| S2 | Raluca Budiu, [Memory Recognition and Recall in User Interfaces](https://www.nngroup.com/articles/recognition-and-recall/), 2024. Full article and its cited revisitation context. | Human-readable choices, visible context and prior work support recognition. Requiring users to memorize IDs, gestures or tutorials increases burden. | Recognition does not justify showing every option at once. No local recognition/recall benchmark exists yet. |
| S3 | Jakob Nielsen, [Response Times: The 3 Important Limits](https://www.nngroup.com/articles/response-times-3-important-limits/), 1993, with its 2014 application note. Full article. | Immediate selection feedback differs from an operation's actual completion. Long work needs meaningful progress; when total work is unknown, show actual work completed rather than fabricate a percentage. | The 0.1/1/10-second values are interaction guidance, not guaranteed backend deadlines. Research runtime is not bounded by a UI timer. |
| S4 | Page Laubheimer, [Data Tables: Four Major User Tasks](https://www.nngroup.com/articles/data-tables/), 2022. Full article. | Tables support finding, comparison, individual detail and actions. Related values should be adjacent; default identifiers should be human-readable. Nonmodal detail preserves adjacent context better than deep modal editing. | Card/table choice depends on task. This does not require a configurable enterprise grid for a small personal workbench. |
| S5 | Kate Kaplan, [8 Design Guidelines for Complex Applications](https://www.nngroup.com/articles/complex-application-design/), 2020. Full article. | Specialized work is nonlinear. Preserve context across workspaces, reduce clutter without removing capability, make important information salient by removing irrelevant material. | Examples span specialized professions. Notes, export and collaboration are not automatically new QZ requirements. |
| S6 | Kate Kaplan, [Designing for Long Waits and Interruptions](https://www.nngroup.com/articles/designing-for-waits-and-interruptions/), 2021. Full article. | Allow background work, help users reenter the task, preserve meaningful historical context, communicate actual progress and completed results. | Its suggestion of modal completion messages is deliberately not copied: repeated unattended studies would interrupt the current task. Persistent per-study status is more appropriate here. Unknown times remain unknown. |
| S7 | Amershi et al., [Guidelines for Human-AI Interaction](https://www.microsoft.com/en-us/research/publication/guidelines-for-human-ai-interaction/), CHI 2019, [paper](https://www.microsoft.com/en-us/research/wp-content/uploads/2019/01/Guidelines-for-Human-AI-Interaction-camera-ready.pdf), DOI `10.1145/3290605.3300233`. Inspected method, Table 1, evaluation and limitations. | Eighteen guidelines distinguish capabilities, quality, contextual information, dismissal, correction, recent interaction, global controls and consequences. Make what the system does distinct from how reliable an outcome is. | The 49 practitioners evaluated guidelines across 20 AI products; they were not QZ users. The paper explicitly notes specialized financial systems may require additional domain guidance. Do not turn general guidance into new scientific rules. |
| S8 | ProductPlan, [State of Product Management 2026](https://www.productplan.com/ebooks/the-state-of-product-management-report-2026), [public report PDF](https://3434168.fs1.hubspotusercontent-na1.net/hubfs/3434168/SPM26/State%20of%20Product%20Management%202026_Final.pdf). Inspected executive summary/method context, participant profile and selected sections on pp. 54-78. | The report surveyed nearly 250 product professionals in Q4 2025. It separates generating more output from improving decisions. Respondents report evidence fragmentation, difficulty connecting insight to decisions and workflow integration friction. Traceability is more valuable than another activity counter. | Vendor-sponsored, self-reported and organizational. The report markets its own platform; that recommendation is not adopted. Sample percentages do not estimate QZ's needs or benefits. No enterprise alignment/governance layer is added. |
| S9 | Andrew Boyagi, Atlassian/DX/Wakefield Research, [developer-experience research](https://www.atlassian.com/blog/developer/developer-experience-report-2024), page dated January 14, 2025. Full public research summary. | Survey of more than 2,100 developers/managers highlights disagreement about friction and the danger of measuring output while missing experience. Investigate task obstacles rather than assume AI output volume means user value. | Developer work is not factor research; the public summary is not the full survey instrument. Only the caution about indirect measures transfers. No claimed QZ productivity increase follows. |
| S10 | W3C, [Understanding SC 4.1.3: Status Messages](https://www.w3.org/WAI/WCAG22/Understanding/status-messages.html). Inspected criterion, intent, examples and techniques. | Announce important asynchronous results without unnecessarily moving focus. Live regions must not become a stream of repetitive polling announcements. | The Understanding document is informative guidance for the actual WCAG criterion. Passing axe alone does not demonstrate full conformance. |
| S11 | W3C, [Understanding SC 2.5.8: Target Size (Minimum)](https://www.w3.org/WAI/WCAG22/Understanding/target-size-minimum.html). Inspected criterion, spacing/overlap exceptions and examples. | AA minimum is 24x24 CSS px with exceptions; larger targets help touch/motor use. This design chooses 38-40px ordinary desktop controls, 34-36px compact auxiliary controls and 44px touch controls, not an inaccurate claim that AA universally requires 44px. | Targets must be tested after layout; nominal icon size is not target size. Dense charts need an equivalent accessible data route. |
| S12 | Ant Design, [Alignment](https://ant.design/docs/spec/alignment/). Full specification page. | Shared left edges support scanning; right-aligned numeric columns support comparison. | Its recommendation to use equal decimal places must not override original scientific decimal precision. Exact strings take precedence here. |
| S13 | Ant Design, [Data Display](https://ant.design/docs/spec/data-display/). Full specification page. | Choose display order by importance/frequency/association, preserve extreme/empty states, use tables for structured comparison and controlled disclosure for detail. | Do not copy every pattern. No carousel, decorative timeline or default grid of cards is needed. Unknown semantic values use explicit wording rather than an ambiguous dash. |

### 2.2 Research-to-component decisions

| Observed design problem | Decision | Components affected | Observable acceptance |
| --- | --- | --- | --- |
| Context is lost between sessions and records | Deep-linked study detail; preserve list/filter state and original submission identity | F01, F02, R01-R04, R09 | Reload/back opens the same study/tab; uncertainty cannot create a new request ID |
| Output volume is mistaken for research value | No token-count hero, success-rate score or invented qualification; show actual independent result | R04-R06, M05 | A computed trial with no independent result never gets a passed badge |
| A table is useful for comparison but poor for long individual content | Compact trial table plus one inline expanded record; ordinary detail workspace | F10, R07 | User can compare adjacent metric values and inspect a full hypothesis without a modal |
| Users do not know whether work is progressing | Actual recorded trial count, execution state and original submission status; no completion percentage or ETA | F06, R04, R08 | Accepted, running, stopped and unanswered remain distinguishable |
| Explanatory text crowds working information | Persistent labels, short necessary feedback, deliberate secondary detail; no generic page introductions | F04, F12-F15 and every workspace | Filled pages contain data/actions, not paragraphs about the product |
| AI capability is confused with reliability | Separate independent evidence, computation, delivery receipt and account facts | R05, P04, O02-O05 | ACK is never labelled filled, and null equity is never plotted as zero |
| Background updates disrupt reading | No automatic list reorder, no modal completion notifications, no focus stealing | F01, F12, R01 | Polling updates facts without moving the selected row/focus |
| Complex tools force unnecessary sequencing | Research is usable without a portfolio or account; data/settings have direct return paths | F02, D03, P01, O01 | A factor-only study can be created and inspected with no configured connections |

### 2.3 Explicitly rejected transfers

Do not add enterprise approval workflows because a product-management survey discusses prioritization. Do not add chat just because the system uses AI. Do not add notes/export unless a later user need and supported implementation justify them. Do not create model confidence scores, automatic recommendations, predictive ETAs or real-time account badges from absent facts. Research recommendations are filtered through this product's actual contracts, not treated as feature requests.

<a id="users"></a>
## 3. User model and jobs

There is one owner, not several invented personas. The owner moves among three modes: **authoring** a question, **monitoring** unattended work, and **evaluating** evidence. Optional simulation/observation follows only when useful.

| Job | Trigger | What the owner must identify | Desired next action | Main failure to prevent |
| --- | --- | --- | --- | --- |
| Start a study | A concrete hypothesis | Goal, actual available data/model, reasoning, trial budget | Submit once, then leave it running | Starting against unavailable data or repeating an uncertain POST |
| Return after absence | Reopen workbench/link | Original goal, execution state, trial attempts, final result | Open evidence or inspect a specific failure | Reconstructing the study from IDs or token totals |
| Compare attempts | Several recorded trials | Hypotheses, partition-specific comparable metrics, failures, selected identity | Inspect a full trial; understand original selection | Hiding failed trials or mixing heldout and iterative metrics |
| Inspect independent result | Final assessment recorded | Computation status, chosen trial, usable/missing metrics and reason | Decide whether the research is useful | Calling computed evidence passed or profitable |
| Build optional simulation | Appropriate original evidence exists | Same data/policy compatibility, explicit capital/weights | Submit supported portfolio operation | Default normalization, invented fees or an arbitrary optimizer |
| Check downstream state | Target delivery/account binding exists | Receipt state/expiry/identity, paper/live mode, actual snapshot time | Reconcile or read a new snapshot | Resending an unknown target or calling ACK execution |
| Repair prerequisites | Missing model/data/extension | Concrete unavailable capability and responsible setting | Configure supported option and return | A tutorial wall, dead button or fake ready state |

### 3.1 Principal wireflows

1. **New owner:** owner password setup -> research workspace empty state -> configure model if absent -> select registered data -> new study -> original receipt -> study detail. There is no compulsory tutorial or portfolio step.
2. **Returning researcher:** original study link -> evidence tab -> trial table -> expand selected hypothesis -> return to evidence. List/filter context stays intact.
3. **Lost submission response:** pending form retains original request/body -> `核对提交` replays exactly that request -> opens the original returned identity. Changed settings or a reload cannot regenerate the body.
4. **Failed independent evaluation:** study opens with a distinct evaluation-failure message -> recorded selection remains visible -> `技术详情` reveals original public error -> trial history still includes all attempts. No retry/finalize button is invented.
5. **Optional portfolio:** evidence -> `创建组合` -> compatible study choices, data, explicit weights/capital -> submit once -> queued/computed/failed result -> actual shared-account curve and target data.
6. **Unknown target result:** observation shows `待核对` -> `核对回执` -> original receipt GET reconciliation -> receipt/account facts update. No second offer is available under a new ID.
7. **Stop observation:** `停止观测` -> short scoped confirmation -> original STOPPED state; receipt remains independently visible. No remote revocation claim.
8. **Repair model:** missing-model state -> model settings -> authenticate actual provider -> pick supported default -> return to original draft. Provider secrets are not draft-persisted.

<a id="navigation"></a>
## 4. Information architecture, routes and composition

### 4.1 Navigation

Primary navigation, in order: **研究**, **数据**, **组合回测**, **目标与观测**. Secondary navigation: **模型设置**, **扩展**. Appearance and owner logout live in the right side of the desktop top bar; mobile exposes appearance in the navigation Drawer. Keep the product name **QuaZonai**; do not invent a new brand or modify existing legal content.

Do not add an overview landing page. It would duplicate the research list and add a click before the work. Do not add a global search box when the API offers only a paginated research list.

| Route | Scope and default |
| --- | --- |
| `#/research` | Research collection, no forced auto-selection on small screens |
| `#/research/:id?tab=evidence` | Original study; default Evidence |
| `#/research/:id?tab=trials` | Original trial ledger |
| `#/research/:id?tab=inputs` | Captured inputs and execution facts |
| `#/research/new?dataset=:id` | New-study form, optional explicit dataset preselection |
| `#/data` | Registered dataset collection |
| `#/portfolios` | Portfolio collection |
| `#/portfolios/:id` | Original portfolio detail |
| `#/portfolios/new?research=:id` | Optional portfolio builder with explicit originating study |
| `#/observations` | Delivery/observation collection |
| `#/observations/:id` | Detail resolved from actual observations collection; no nonexistent detail API |
| `#/observations/new?portfolio=:id` | Target/account binding form |
| `#/settings` | Models, defaults, usage and appearance |
| `#/extensions` | Extension state/preferences |

Use ordinary anchors and native history/hash navigation; no router dependency is needed for this scope. Unknown route/ID gets `未找到记录` and a relevant return link, not a blank screen. Browser Back restores the preceding route, not an unrelated first record. Deep links remain reloadable behind the existing static fallback. Existing URL compatibility is checked before removing old route handlers; a simple redirect is preferable to a parallel legacy UI.

### 4.2 Desktop layout

Use a 64px desktop top bar rather than a full-height sidebar. Brand and four primary destinations sit on the left; the two settings destinations, appearance and owner access sit on the right. The current destination has a restrained 2px underline, not another tinted rounded container. At desktop widths all navigation stays on one line.

Main content is capped at 1360px including 40px horizontal padding. Research uses `280px minmax(0,1fr)` with a 32px gap. The collection sits directly on the neutral canvas, without an outer card. Only the selected study is a coherent working-document surface with a thin perimeter, 14px radius and almost imperceptible tinted elevation. Its input context, tabs, metrics and trial rows are not nested cards. Ordinary document padding is 30px; use aligned values and sparse separators to organize content.

At 1024-1279px use 28px page padding, a 256px collection and 24px gap. If useful detail width cannot be retained, switch composition rather than squeeze text. Below 1024px use a 58px compact header and a navigation Drawer; show list or selected detail as the main page. Below 768px the header is 56px, document content has 20px insets, and the selected document loses its desktop perimeter/radius/shadow so it reads as an app screen rather than a card inside a phone.

### 4.3 Scroll and focus

The normal page scrolls vertically. Avoid permanently nested list/detail scroll areas on small laptops. Sticky collection controls or table headers are allowed only when they do not obscure focus/content. Chart/table horizontal overflow belongs to that labelled region, never the page. New-study/builder forms are ordinary pages, not deep editing modals.

Use a visible-on-focus `跳到内容` link. Route changes focus the new main heading once; polling never does. Returning from detail restores the collection context and the original selected link when possible. Dialogs return focus to their trigger.

<a id="visual"></a>
## 5. Visual system and measurable refinement

### 5.1 Semantic colors

| Token | Light | Dark | Intended use |
| --- | --- | --- | --- |
| `canvas` | `#F5F6F8` | `#171B20` | Neutral page background |
| `surface` | `#FDFEFE` | `#20252B` | Working document and controls |
| `surfaceInset` | `#F1F4F6` | `#292F36` | Quiet grouping, expanded original record |
| `text` | `#26313A` | `#E7ECF1` | Body and primary values |
| `textSecondary` | `#626F7B` | `#A7B1BC` | Labels, metadata, placeholders |
| `accent` | `#2A6B62` | `#94C7BC` | Links, current-navigation underline, chart line |
| `primary` / `primaryText` | `#293740` / `#FCFDFE` | `#E7ECF1` / `#202830` | Neutral high-contrast primary actions |
| `accentSelected` | `#EDF3F1` | `#2B3B3A` | One selected research row |
| `divider` | `#E0E5EA` | `#38414A` | Structural separators, not essential control boundaries |
| `controlBorder` | `#8795A1` | `#70808E` | Input boundary/focus-independent outline |
| `warningText` / `warningFill` | `#815D26` / `#FBF3E5` | `#E8C482` / `#3B3224` | Pending reconciliation, restart requirement |
| `errorText` / `errorFill` | `#AF433E` / `#FCF0EE` | `#F0A29A` / `#3D2C2C` | Actual failed/rejected operations |

Color is a scarce resource: large surfaces are neutral, primary actions are graphite in light mode and off-white in dark mode, and teal identifies links/selection rather than bathing the interface. Semantic warning/error are not decorative accents. `COMPUTED` has a neutral outcome label, never a qualification color.

Calculated token-pair contrast for revision 2: body 13.13:1 light / 12.98:1 dark; secondary 5.10:1 / 7.10:1; primary CTA 12.02:1 / 12.55:1; field border 3.04:1 / 3.80:1; warning text 5.40:1 / 7.59:1; error text 5.10:1 / 6.48:1; accent text 6.15:1 / 8.20:1. These are specified color-pair calculations, not full rendered conformance or visual-quality acceptance. Component-derived hover/disabled/focus colors still require browser inspection.

Use a single application-level official Ant `ConfigProvider`, its documented light/dark algorithms and semantic seed tokens. Layout CSS consumes matching variables. Do not fight the library with pervasive per-component overrides or a second design system.

### 5.2 Typography

Use self-hosted `Geist Variable` and `Geist Mono Variable`, with native Noto Sans CJK SC/platform sans for Chinese. The revised prototype actually loads these fonts; it no longer merely names an unavailable Geist fallback. Production installation, after design acceptance, is `npm install --prefix apps/web @fontsource-variable/geist @fontsource-variable/geist-mono @phosphor-icons/react`. The prototype dependencies remain isolated in `/tmp/qz-design-prototype`; no application dependency has been changed. Preserve their upstream license notices when adding production packages. No Google Fonts request occurs at runtime.

| Role | Size / line height | Weight | Rule |
| --- | --- | --- | --- |
| Page heading | 25-26 / 36-38px desktop, 23-24 / 34-36px mobile | 600 | One clear page title, no marketing display scale |
| Selected goal/record title | 26 / 38px desktop, 23 / 36px mobile | 600 | The selected goal is the document's h1; full authored content remains accessible |
| Section heading | 15-16 / 24-25px | 550-600 | No uppercase eyebrow above every heading |
| Body and form value | 14 / 22px desktop, 16 / 24px touch forms | 400 | Main facts remain readable on laptops |
| Control label | 14 / 20px | 500 | Not all bold text |
| Metadata | 12 / 18px normally; 11px only for supplemental technical identities/captions | 400-450 | Never make tiny metadata the only route to required information |
| Major financial value | 26 / 34px | 500 | Right/decimal alignment where compared |
| Numeric table cell | 13-14 / 20px | 400 | Tabular numerals; exact strings retained |

Use monospace only for numeric/technical identity fields. Chinese body text is not forced into monospace. Avoid all-caps strings, gratuitous tracking, italics and decorative punctuation. The only display separators are spacing, columns or a regular hyphen where a range requires it.

### 5.3 Spacing, shape and layers

Spacing scale: 4, 8, 12, 16, 24, 32, 48px. A field label sits 8px above its control; helper/error sits 8px below. Related fields use 16px gaps; separate groups use 24px. Page sections use 32px. Do not compensate for poor grouping with extra borders.

Radius roles: 4px metadata/status labels; 7px controls and short action links; 9px selected rows/small glyph wells; 14px working documents and dialogs. No arbitrary pills. Use one almost imperceptible, neutral-tinted working-document shadow (`0 5px 20px rgb(38 49 58 / .025)`) only where it separates the main document from its directory. Internal metric/trial/settings groups use space or functional separators, not more cards. On mobile the document perimeter and shadow disappear. No glass, grain, glows or gradients.

Document layer tokens in the theme module: base content 0, sticky controls 10, shell/mobile navigation 20, official popups/dialogs in the library's own documented popup layer, with no competing arbitrary high-z-index content. Respect the component library's overlay/focus management.

### 5.4 Interaction finish

Ordinary controls are 38-40px high desktop; compact auxiliary actions are 34-36px, and touch actions at least 44px, with at least 8px separation. Use one visible application icon family, official `@phosphor-icons/react`, normally 14-18px regular weight. Reserve duotone for the single creation-context glyph; do not scatter icon tiles throughout the page. Primary desktop navigation is text-led. Glyphs beside text are decorative to assistive technology; icon-only buttons have explicit accessible names. Never hand-draw paths.

Hover, active and focus are distinct: hover tint for pointer discovery, active `scale(.98)` only on safe small controls, and a visible 2px focus indicator with offset. No active transformation on an entire table row. Use 120-160ms opacity/transform feedback only when motion is permitted; reduced-motion removes it. Status/data updates never animate layout or count upward from zero.

### 5.5 Content rules

A filled workspace has no introductory paragraph. Title, controls and facts are the content. System-authored headings should normally fit 2-8 Chinese characters; buttons 2-6; ordinary feedback one sentence, usually under 24 Chinese characters. A short necessary consequence statement may be longer in a confirmation. These are editing targets, not validators on user-authored research goals or original evidence.

Full user goals, original hypotheses, recorded reasons and public errors remain accessible. Use a two-line preview plus a clear `展开` for long content rather than throwing away facts or manufacturing an AI summary. Do not put the only full value in a hover tooltip. A temporary toast cannot be the only record of a failed/uncertain mutation.

Do not print product lectures, pipeline manifestos, motivational copy, greeting paragraphs, repeated `计算不代表盈利` warnings or API vocabulary across every page. Convey boundaries in names and affected controls: `独立评估`, `目标回执`, `账户快照`, `停止观测`. Show one scoped consequence at the relevant action, not a permanent wall of caution.

<a id="components"></a>
## 6. Shared component contracts

The identifiers below are design/test references, not a mandate to create one React wrapper per row. Reuse official components directly where that is simpler. A shared wrapper is justified only by repeated product behavior.

### F01. Application frame and theme

- **Anatomy:** owner/session boundary, slim top navigation or compact mobile header, main content, one contextual status area, official popup context.
- **Behavior:** one Query client and one theme root; theme choices `跟随系统`, `浅色`, `深色` persist locally. System mode responds to OS changes. Theme does not reset route/draft/selection.
- **States:** initial session pending uses a stable shell skeleton; unauthenticated renders A01; expired auth returns to A01 while preserving a nonsecret pending request for the same origin; service unavailable renders a retryable root state.
- **Acceptance:** both themes retain hierarchy; no light popup on a dark page; polling failures do not replace the whole app with a blank screen.

### F02. Top navigation / mobile navigation

- **Anatomy:** 64px desktop bar, brand at the left, four text-led primary links, two right-hand settings destinations and appearance/logout controls. No permanent administrative sidebar or duplicate brand subtitle.
- **Selected state:** text weight plus a restrained 2px underline, with `aria-current=page`; no full tinted tab/card or decorative status dot. Keyboard focus is independently visible.
- **Mobile/tablet:** below 1024px the compact header opens an official Drawer with the same named links. Touch links/buttons are 44px. Selecting a destination closes the Drawer and updates main focus. No hover-only names or icon-only bottom bar.
- **Acceptance:** the desktop bar stays on one line at 1024px; all six destinations remain keyboard/touch reachable; browser Back never unexpectedly opens the Drawer.

### F03. Page header

- **Anatomy:** a breadcrumb only for detail/new pages, one `h1`, optional short factual count, one primary action. In Research the selected goal is h1 and the directory has a smaller Research heading with its creation action, avoiding a redundant page-title block. Secondary actions remain adjacent but quieter.
- **Behavior:** page title is not a hero; no welcome paragraph or duplicate primary CTA within the same first screen. Do not place a floating right-hand explainer opposite the heading.
- **Mobile:** title and action form two rows only when needed; action label never wraps internally.
- **Acceptance:** 1280x720 shows meaningful data below the header; title/actions do not consume most of the viewport.

### F04. Field block

- **Anatomy:** persistent label, official control, optional concise necessary helper, conditional error. Required state is expressed in label/control semantics, not placeholder alone.
- **Behavior:** validate after blur/submission, then update the touched error as it is corrected. Never show all fields invalid before the user interacts. Invalid submit focuses the first invalid field.
- **Variants:** text/textarea, searchable select, explicit integer, exact decimal string, checkbox/select prompt. No slider for a precise budget/weight/capital.
- **Acceptance:** label click targets the input; Enter submits only in appropriate single-line contexts; textarea Enter remains a newline; errors use `aria-describedby`.

### F05. Action button and link

- **Variants:** primary filled, secondary bordered, tertiary text link, danger action with explicit verb. Exactly one visual primary per local decision group.
- **Pending:** preserve button width, show short pending label and official busy state; repeated activation does not issue a new command. Accepted 202 transitions to original result detail, not `完成`.
- **Disabled:** use only for a real unavailable prerequisite/current mutation; a nearby short reason or actionable empty state explains it. Never rely on an inaccessible tooltip on a disabled button.
- **Acceptance:** normal/hover/active/focus/pending/disabled labels have sufficient contrast; no action relies on an unlabeled icon or hover.

### F06. Status label

- **Anatomy:** 4px-radius compact neutral label in a document header; a plain glyph-plus-text variant in record rows. Use recognizable clock/check/warning glyphs for actual states, never pulsing colored dots. Source context determines wording; color alone never conveys state.
- **Mapping:** research, trial, native task, submission, receipt, extension and provider states are separate dictionaries. An unknown enum is shown in neutral original form, not mapped to success.
- **Acceptance:** a `terminal` native task is not shown as `完成` unless its outcome is completed. `COMPUTED` is not `验证通过`; `ACKNOWLEDGED` is not `已成交`.

### F07. Detail tabs

- **Anatomy:** official Tabs, 44px minimum hit height, short text labels, underline/selected text rather than decorative full-card controls.
- **Behavior:** selection is in the URL; keyboard follows official tab semantics; server refresh does not reset selection. Main content remains stable while a child query refreshes.
- **Mobile:** three research labels fit without horizontal page scrolling. If future additions exceed width, use the library's accessible overflow, not squeezed text.
- **Acceptance:** reload/back restores tab; inactive heavy charts are not mounted unnecessarily.

### F08. Search/filter toolbar

- **Anatomy:** a compact inline search/filter row, with persistent accessible label `搜索已加载研究`, search glyph, clearable input, `全部状态` control and a visible loaded-record scope. A visually hidden label is acceptable for this familiar search tool; it is not replaced by a placeholder. Authoring fields still use visible labels above controls. Render exactly one `清除筛选`: beside matching results or inside the no-match state, not both.
- **Behavior:** filter loaded records synchronously; show scope as `已加载 N 条` and result count `匹配 M 条`. No claim of global search. Polling does not clear filters.
- **Empty:** distinguish no records from no matches. `清除筛选` retains the collection; `加载更多` stays available when the server cursor exists even if loaded results have no match.
- **Acceptance:** a record on a later page is reachable; filtering never hides the server pagination control.

### F09. Key-value facts

- **Anatomy:** semantic `dl`; short muted label above or beside a readable value. Use two columns only when both remain legible. Long technical values wrap or have an explicit full-value/copy affordance.
- **Behavior:** absent means `未记录` or `未提供` as appropriate; zero stays `0`. No whole JSON object dump. Copy acknowledges only an actual successful clipboard operation.
- **Mobile:** one column for long values; two columns only for short independent facts that remain readable and wrap fully. Never force an offscreen horizontal key/value grid.
- **Acceptance:** a long provider/model ID and large exact amount do not overflow the page or become indistinguishable ellipses.

### F10. Working table

- **Anatomy:** clear caption/heading, meaningful first column, compact header, approximately 52px default rows; long hypotheses may grow. Related metrics adjacent; numeric columns right-aligned/tabular.
- **Behavior:** local sort only when explicitly labelled as loaded records; unknown numbers sort last. Use exact decimal comparison where needed, not lossy Number coercion. One inline expanded record is preferred for long trial content. Preserve all rows through pagination when local paging is needed.
- **Mobile:** labelled `overflow-x:auto` region with keyboard focus and minimum readable column widths; preserve identity column where helpful. Do not delete critical columns or turn every number into an unrelated card.
- **Acceptance:** full column meaning remains known while scrolling; actions remain reachable without hovering; no generic top-and-bottom border on every cell.

### F11. Metric value and availability

- **Anatomy:** human-readable metric name, original value, unit only if recorded, partition and component attribution, optional concise availability reason. A compact metric summary is plain aligned data, not a decorative KPI card wall.
- **Formatting:** retain decimal/counter strings. Numeric correlation uses a documented display precision with full original value available. Do not invent percent/return units. Unknown value reads `不可用`; absent metric reads `未提供`.
- **Known IC reasons:** `INSUFFICIENT_SAMPLES` -> `样本不足`; `CONSTANT_SCORE` -> `分数恒定`; `CONSTANT_LABEL` -> `标签恒定`; `NONFINITE_DATA` -> `数据非有限`; `NONFINITE_RESULT` -> `结果非有限`. Preserve an unknown status in detail.
- **Acceptance:** `null + CONSTANT_SCORE` does not display 0, blank success, a low-score bar or a pass/fail judgement.

### F12. Contextual feedback

- **Anatomy:** compact one-line success/pending/error feedback near its owning operation. Actual error details may expand below. Root/query failures occupy the relevant region rather than an unrelated toast.
- **Accessibility:** semantic status/alert only on meaningful state transitions; do not announce every polling interval or the full trial table.
- **Acceptance:** a user who misses a toast can still find an uncertain/error result beside the affected record/form.

### F13. Empty state

- **Anatomy:** one short heading, at most one short necessary line, one meaningful next action; use a small library glyph only when it aids recognition. No illustration scene, slogan or large onboarding narrative.
- **Variants:** no study, no filter match, no evidence, no datasets, no curve, no positions, no configured connection. Each names the missing fact precisely.
- **Acceptance:** no data error becomes an empty state. Missing positions and an explicitly recorded empty positions array are distinct: the former says `尚无快照`, the latter `无持仓`.

### F14. Loading/refresh state

- **Anatomy:** skeleton shaped like the real list/detail/table; reserve chart height. Initial load and background refresh are not the same state.
- **Behavior:** keep last successful facts visible during refresh, with `更新失败` if needed. Do not replace the whole page with a spinner every few seconds. No progress percentage unless an actual measured total exists.
- **Acceptance:** refreshing a selected study leaves its heading, tab and scroll position stable.

### F15. Error panel

- **Anatomy:** concise translated problem, one valid repair/retry action, optional `技术详情` with original public error code. Preserve the input/draft.
- **Behavior:** known 4xx prerequisite failures route to repair; uncertain network/5xx after a mutation retain original identity; auth expiry uses the owner access flow. A request conflict is not automatically retried with a new ID.
- **Acceptance:** `dataset_not_ready`, model unavailability and missing independent evidence have different messages and actions.

### F16. Scoped confirmation

- **Use:** explicit owner stop, observation stop or provider disconnect; not routine reads, navigation, form saves or every mutation.
- **Anatomy:** action-specific title, affected record name, one short consequence, `取消`, explicit action verb. No generic `确定`/`操作成功`.
- **Behavior:** official focus trap and return; dismissing cannot issue a mutation. Pending action prevents duplicate activation; errors remain in context.
- **Acceptance:** observation stop says `仅停止本地观测` and does not mention cancelling orders or withdrawing targets.

### F17. Exact-value chart

- **Anatomy:** heading `账户权益`, currency, actual time/value axes, one native equity series, readout for original point, `查看数据` disclosure. Height 300px desktop, 240px mobile.
- **Behavior:** native snapshots only; chart Number conversion is display-only; labels/data table retain original decimal/time strings. Nulls are gaps; `connectNulls` is false. No interpolated fake samples, smoothing that changes interpretation, cumulative factor-curve sum or locally recomputed authoritative NAV.
- **Accessibility:** semantic summary of existing record count/date extent, accessible exact-point table; pointer tooltip is not the only way to read a value. Plot uses one theme accent and sufficient line contrast.
- **Acceptance:** a missing snapshot does not produce a flat zero line; one point remains inspectable; changing theme resizes/recolors without losing data.

### F18. Identity/provenance disclosure

- **Anatomy:** one visible short identity when needed, explicit `来源与身份` secondary disclosure for original request/conversation/submission/trial/receipt/account sequence fields.
- **Behavior:** no passwords, tokens, raw headers, hidden reasoning or private transcript. Technical strings are selectable; copy is explicit and reports real success/failure.
- **Acceptance:** a researcher can correlate a selected trial/receipt without being forced to read technical IDs in every primary row.

<a id="research-components"></a>
## 7. Research workspace components

### R01. Study collection

The directory sits directly on the page canvas, not inside another framed card. Its heading contains the single `新建研究` action. Use the compact F08 toolbar above the collection, followed by a native-cursor `加载更多` button only when available. Default order follows descending original native identity; do not fake recency from null timestamps. Show `已加载 N 条`, not `全部 N 条` when more pages exist.

The initial unselected desktop right region says `选择一项研究` without a second primary CTA. On mobile, show the collection as its own page. Returning from a detail restores filter and selected study context. When a selected ID is outside loaded pages, its direct detail query still works; do not require loading every earlier page.

Initial error occupies the collection region; subsequent page error preserves previous records and offers `重试加载`. Polling refreshes loaded pages without replacing them with only page one. Record status changes do not reorder rows while the user reads.

### R02. Study row

Anatomy: a small original-ID/contextual-state line; a stronger two-line human-readable goal; then trial count/model. ID is visually secondary, never the main title. Use 16px insets, 12px metadata and 14px goal text with controlled medium weight. Selection is one quiet tinted row and a 2px leading indicator, not a bordered card. A whole row is one anchor; never nest independent buttons inside it.

Do not display a timestamp when the projection returns null. Do not display a success sparkline or invented percentage. The complete goal appears in R04, even if the row preview truncates. Background failure remains visible through the status label.

### R03. State filter

Options are `全部`, `进行中`, `已计算`, `失败`, `已停止`, `待处理`. These are presentation groupings, not rewritten durable state. `待处理` includes explicit inactive/unanswered cases using available native facts; do not call every IDLE study failed. If only the compact list summary exists, do not infer an unanswered reason until detail is loaded. Display original unresolved state neutrally rather than hiding it.

Filter controls remain visible above results. Counts are loaded-record counts. Changing filter does not silently navigate to an unrelated first matching study. `清除筛选` is always available in a no-match state.

### R04. Selected study header

Anatomy: a small record breadcrumb and neutral scientific state; the full goal as the working document's h1; then aligned dataset/model/trial-budget context. Use the captured budget for `N / M 次试验` when known, never as a completion percentage. A quiet `研究操作` menu contains original identities and a stop action only while original research can still be stopped. A long goal starts as two lines with `展开`, retaining full content when expanded.

Below it, show **证据**, **试验**, **输入与执行** tabs. A scientific final `STARTED` is `独立评估中`, not `研究完成`. If stop intent is recorded while native tasks remain live, show `停止中` for execution, while preserving the original recorded final status in Inputs and execution. Only actual settled execution supports `已停止` as the execution result.

The default header is not a collection of four disconnected KPI cards. No token usage, greeting, countdown or model-generated explanation is inserted here.

### R05. Independent result panel

Use one unboxed evidence section inside the working document. Its heading is `独立评估`, with a compact original selected-trial action at the right. Do not repeat the same computed badge both here and in the document header. Metrics are adjacent aligned values with only functional vertical separators, not three individual cards. Beneath them show the recorded partition/component once. Branches:

| Facts | Primary presentation | Available next action |
| --- | --- | --- |
| `final=null`, research active | `尚无独立结果`; current attempt facts elsewhere | `查看试验` |
| `final=null`, inactive/unanswered | `未产生独立结果`; original execution reason in R08 | `查看执行` |
| `final.status=STARTED` | `独立评估中`; selected trial if recorded | `查看试验` |
| `COMPUTED` with metrics | `已计算`; independent metric group | `查看所选试验`, optional `创建组合` |
| `COMPUTED` but no usable metric | `已计算`; explicit `指标不可用` or `未提供指标` | Availability detail, original selection |
| `FAILED` | `独立评估失败`; separately exposed public error | `技术详情`, selected original trial |
| `STOPPED` | `研究已停止`; original stop reason if useful | Original trial history |
| Unknown status | Original neutral state string | Original details |

The model's recorded reason is not a new generated explanation. If long, preview two lines and reveal the original full reason with `展开结论`. Error and selection rationale are separate fields. No manual finalize/retry/resume control exists because no public API supports it.

A computed result may expose `创建组合` as an optional next action, not a qualification certification. Necessary dataset/adapter incompatibility is shown in the builder or directly reported by the existing service. Do not duplicate a complex eligibility engine in React.

### R06. Partition-aware metrics

The default Evidence tab shows independent-result metrics, not a blended summary with iterative trials. Group by recorded partition and distinguish original `TRAIN`, `VALIDATION`, `HELDOUT` as `训练`, `验证`, `独立样本`. Highlight the independent group through position and heading, not a green pass background.

Where the native result carries component attribution, use an explicit group label such as `分量 0`; never present component diagnostics as ensemble account metrics. Original native account statistics use their supplied native keys and units; do not invent Sharpe/return labels through string guesses.

Primary compact summary includes only existing, recognized IC/sample-count fields. Remaining recorded metrics are in an adjacent comparison table or one `全部指标` disclosure. Do not invent empty fixed KPI slots. Included/excluded counts remain exact counter strings. Unknown metric keys remain readable/selectable in secondary detail.

### R07. Trial ledger and expansion

Desktop columns: **假设与身份**, **验证指标**, **状态**, **独立选择**, and the right-hand **详情** action. If multiple comparable metrics exist, offer a concise metric selector or grouped columns without pushing hypothesis/identity away. Do not compare different partitions under the same unlabeled heading.

Each trial retains original task/trial identity; failed/rejected attempts are present. `STARTED` -> `计算中`, `COMPUTED` -> `已计算`, `FAILED` -> `失败`, `REJECTED` -> `已拒绝`, with original error/reason available. Selection label means exactly `所选试验`, not best/profitable.

The Evidence tab may show the latest three original trial rows as a compact reentry aid, with identity, hypothesis, state and a link to the complete ledger. This is not a new summary/selection engine. Selecting a preview opens the matching original trial; failures remain visible. Full table order is hypothesis/identity, adjacent metric, status, original selection and a right-hand expansion control; headers have no heavy filled band.

An expanded row shows full hypothesis, all recorded partition metrics and actual public error. At most one expansion at a time keeps comparison legible. No additional nested accordion inside the expanded row. The selected trial link in R05 opens this tab, reveals the matching row and places it in view. If its row cannot be found, state the missing reference rather than choose another trial.

### R08. Inputs and execution

Two readable groups, not raw JSON:

- **研究输入:** original goal, captured dataset identity/metadata when recorded, model/provider, effective or requested reasoning as actually available, trial budget and recognized original policy fields. A historical missing dataset display name is `未记录`, never substituted with a claim about the current registry. Original full input data remains untouched server-side.
- **执行记录:** original submission ID/status and latest native tasks with kind, state and separately exposed outcome. `queued`, `placed`, `done`, `unanswered` stay meaningful as submission facts. An answered model submission is not independent scientific success.

Native task inspection currently covers only the latest 100 records; label `最近执行记录` and do not claim complete history. Unknown created/updated timestamps and cost remain omitted from primary layout or explicitly `未记录` in detail. Do not synthesize a timeline from ID order with fake times.

### R09. New-study form and submission identity

Use an unboxed full-page form up to 680px wide under the slim top navigation. A single small creation glyph accompanies the heading; do not frame the entire form and every input group in nested cards. The goal/data group is followed by a sparse separator and `运行设置`, containing model and the reasoning/budget pair. Field order:

1. **研究问题:** required TextArea, initially 4 rows, grows to a bounded comfortable height with internal scrolling; placeholder is a short example, not the label. Do not generate a hypothesis on the owner's behalf.
2. **数据集:** searchable actual registered options; label with name, compact source/coverage below selection. Nonready entries remain inspectable but cannot be submitted as ready. A dataset selected from Data is explicit preselection, not an invisible automatic substitution.
3. **模型:** searchable actual available options, displayed as provider/name; exact ID available in the option/detail. Use a valid configured default when present. Never silently substitute a different model.
4. **推理强度:** only supported choices for the selected model. Nonreasoning model has the actual `off` value; stale unsupported selection is cleared visibly, not submitted silently.
5. **试验预算:** explicit nonnegative safe integer with `次` suffix, matching existing admission rules including zero; do not add an arbitrary UI maximum, time estimate or monetary cap. Helper: `失败试验也计入预算` only when useful beside this control.

Model is a full-width field. Reasoning and budget share a two-column desktop row; mobile explicitly stacks all controls. Footer has `取消` and `开始研究`; cancel returns to the originating workspace. Button remains available for validation feedback when fields are incomplete; real unavailable prerequisites instead show a direct repair state.

Submission phases:

- **Editing:** nonsecret form draft persists in the current tab across route changes/reload; field changes do not call the research API.
- **Submitting:** construct one `requestId` and capture the exact simplified body; freeze these command fields while pending. The button reads `提交中`.
- **Accepted:** retain returned original conversation/submission identities, clear only the resolved pending command, open that study; scientific status is still read from the service.
- **Uncertain:** keep original command unchanged, show `提交结果待核对` and `核对提交`. Replaying uses the exact captured request/body, not current defaults or a new ID.
- **Definitive rejected:** show specific field/context error; editing may create a genuinely new command only after the original was definitively not admitted.

If sessionStorage is unavailable, keep a memory draft and honestly indicate if persistence failed; never claim `已保存` without a write. Do not store provider credentials, owner passwords or raw private input objects. Leaving/reloading an uncertain form must not silently discard its original command.

<a id="data-components"></a>
## 8. Data workspace components

### D01. Dataset collection

Use a readable working table/list: **数据集**, **来源**, **资产**, **覆盖区间**, **币种**, **状态**, **操作**. The human-readable name is primary; original dataset ID is secondary. Order follows configured data, with an optional local search when useful. Do not add a global ingestion progress dashboard.

Coverage comes only from declared `start/end`. Missing boundary reads `未提供`, not an infinite history assumption. Readiness labels `已就绪` and `未就绪` describe actual configuration state, not PIT quality or licensing approval. Show an actual source string, not an invented logo/trust score.

### D02. Dataset facts

A selected/expanded dataset presents the same source, range, currency, asset class and any explicitly exposed adapter capability as aligned facts. It does not render `data/policy/strategy` JSON or expose private host paths as an editable browser configuration. Distinguish a configured portfolio adapter from a guarantee that every research result is eligible.

An empty dataset workspace says `尚无已登记数据集` with one supported configuration/help link. There is no dead `上传数据`, `同步`, `扫描本机` or `添加来源` action because no such API is implemented in this task.

### D03. Start from data

The row/detail `用于研究` action opens R09 with the original explicit dataset ID. It does not start a study immediately. If not ready, replace the active action with a short status/repair message; do not silently choose another ready dataset. Missing models are handled by the new-study prerequisite state, not by hiding data from the user.

<a id="portfolio-components"></a>
## 9. Portfolio/backtest components

### P01. Portfolio collection

An ordinary list/table: **组合名称**, **状态**, **数据集**, **研究数**, **本金**, **创建时间**. Creation time uses the actual recorded timestamp. Status `QUEUED`, `COMPUTED`, `FAILED`, `STOPPED` remains factual. Selecting opens original detail. One `创建组合` action is available; the empty state points to research when no usable original evidence exists.

Do not show summed factor sparklines, invented annualized returns, fictional recommended portfolios or a tuning leaderboard.

### P02. Evidence-member selector

Full-page builder width up to 900px. Actual original research choices display human-readable goal, independent result state and captured/current dataset identity with honest historical gaps. Only clear known prerequisites are applied locally; original service admission is authoritative for dataset/policy/selection compatibility.

Selecting research never runs a probe or re-evaluates it. Multiple members remain individually visible with a remove action, original ID and explicit weight. The initiating study is preselected visibly. No `Select all` across unseen cursor pages; more research remains loadable.

### P03. Exact allocation and capital form

Fields: **组合名称**, **数据集**, member **权重**, **本金** with the dataset currency visibly attached. All monetary/weight values are strings, not browser floating-point InputNumber values. For a single member the supported exact weight is 1; show it as such. For multiple members require explicit positive weights summing exactly to 1, matching the backend's 18-fractional-digit admission support. Do not silently normalize.

Inline weight footer shows `合计` with exact decimal total and `合计须为 1` only when invalid/needed. Capital must be positive under the actual service rules. Do not add arbitrary risk caps, leverage defaults or policy edits. Submit button `运行回测` uses the shared original-command replay behavior. API compatibility failures appear adjacent to the affected selection, not as an unexplained toast.

### P04. Portfolio result summary

Header: original name, actual status, data/currency/capital, member links and captured allocations when recorded. In COMPUTED state show existing native metrics and the actual shared-account curve. Queued state shows skeleton/status without a fake curve. Failed state keeps original inputs and specific error visible; it is not a zero return.

A concise `策略来源` disclosure can show recorded supported policy name, threshold and invested weight when available. Do not guess fees or model policy from the selected instrument. Missing historical allocations/policy fields say `未记录`.

### P05. Equity and target detail

Use F17 for the one authoritative shared-account equity series. Below, a compact actual target table has **标的**, **目标权重** with exact original decimals. It is not labelled orders, fills or live positions. An absent target is `尚无目标`, not an empty portfolio success.

Native metric names/units remain faithful. Do not compute a new return, Sharpe, drawdown or composite score merely to fill a design tile. If a displayed metric is derived for presentation, label it and retain its source; this task should prefer existing native metrics.

### P06. Optional target handoff action

`交付目标` opens O01 with the explicit portfolio identity. It does not immediately POST downstream. It appears only for a computed portfolio with an actual target; expiry/connection rejection still comes from the service. A historical expired target reads `目标已过期` and has no fake renewal control. The backtest itself remains readable even when target delivery is unavailable.

<a id="observation-components"></a>
## 10. Target and observation components

### O01. Binding form

Fields: **组合**, **模式**, **连接**. Portfolio choices show actual computed status and original name. Mode choices are `模拟` / `实盘`, derived from configured connections, not a color-only distinction. Connection option uses actual name and identity; current `connected` means a recorded active observation, not a fresh network probe.

Do not silently choose a live connection. Require explicit selection when no unambiguous preselection exists. A short consequence line beside submit is `仅交付目标，由下游执行`. This is necessary scope at the action, not a repeated page lecture. Submit label `交付并观测` and original-command persistence follow F05/R09. A successful 202 means accepted work, not delivery ACK.

### O02. Observation collection and heading

Collection rows: original name, **模拟/实盘**, **目标回执**, **观测状态**, **快照时间**. Selecting opens the record. Do not collapse the two states into one green `在线` badge. Header retains portfolio link, connection identity when recorded and a clearly visible mode label.

Use `上次快照` for actual snapshot time, not `实时`. Polling the collection reads stored facts; it does not imply the downstream account was polled. No automatic account-fetch loop is added without an actual supported policy.

### O03. Receipt block

A compact standalone group **目标回执** precedes account facts. Primary fields: actual receipt state and expiry; secondary `来源与身份`: original receipt ID, external ID and acknowledgement time.

| Receipt status | Copy | Action |
| --- | --- | --- |
| PENDING | `待接收` | Read current state; allow supported refresh |
| UNCERTAIN | `待核对` | `核对回执` using original receipt |
| ACKNOWLEDGED | `已接收` | `更新快照` |
| REJECTED | `已拒绝` | Read actual reason/details |
| EXPIRED | `已过期` | No fake renewal/reoffer |
| REVOKED | `已撤销` | Recorded remote fact only |
| NOT_OFFERED | `未交付` | Show actual delivery failure when present |
| Unknown | Original neutral status | Read details |

Keep receipt error separate from account-fetch error when exposed by available facts; if the API only has one shared error field, label it `最近操作错误`, not invent a causal classification. ACK must never read `已成交`, `已执行` or `盈利中`.

### O04. Account snapshot

Group **账户快照**: **权益**, **现金**, actual **币种**, snapshot time; below, F17 equity history and actual positions table **标的**, **数量**, **市值**, **盈亏**.

No snapshot means `尚无账户快照`, with no numerical zeros. A recorded empty positions array after a snapshot means `无持仓`. Null cash/value/pnl reads `未提供`. Exact strings and signs remain; a positive P&L can be sign-marked but never implies evidence qualification. No invented broker, account type or sequence.

A secondary provenance disclosure shows captured connection ID plus actual client/account/sequence/native clock only when projected from captured facts. URLs, environment-variable values, bearer tokens and private provider state remain offscreen and out of the DTO.

### O05. Refresh/reconciliation action

One primary action changes label by the actual receipt context: `核对回执` when uncertain/pending reconciliation, otherwise `更新快照`. It invokes the same existing `/refresh` operation; it never creates a second observation/request/receipt or extends original expiry.

During request, show `核对中` or `更新中`, preserve last successful snapshot, and retain the original selected record. Error shows inline and leaves previous facts visible with their original time. Do not stamp them with local fetch time. A repeated identical snapshot remains the same historical point; no duplicate chart point is fabricated by the browser.

### O06. Stop observation

Use a secondary danger `停止观测` action and F16. Confirmation: title `停止观测？`, one line `仅停止本地观测，不撤回下游目标。`, buttons `取消` / `停止观测`.

STOPPED preserves receipt/account facts. If the original receipt remains uncertain, allow the actual supported `核对回执` path without claiming observation restarted or fetching a new account snapshot. The server owns this distinction. No `撤单`, `平仓`, `清仓`, `重发目标` or unsupported remote-cancel action is rendered.

<a id="settings-components"></a>
## 11. Model/settings components

### M01. Provider connection list

A quiet list, not marketing cards: provider name, `已配置`/`未配置`/`状态不可用`, supported authentication action(s), pending-login state and actual provider error. A configured provider with no available models does not become ready by assumption.

One visible authentication action is used when only one method exists. If OAuth and API key are both supported, show explicit method choices labelled `登录` / `API Key`; do not guess which provider credential to use. Disconnect uses `断开` and a scoped confirmation. If environment authentication remains after logout, show returned `仍已配置`; never force the display to disconnected.

### M02. Provider authentication dialog

A small official modal for the short authentication ceremony; mobile is full-width with content reachable above the keyboard. It shows provider name, current actual prompt and events relevant to that prompt.

- `secret`: labelled password input, no storage/autocomplete of returned secret in draft state; clear after response or cancellation.
- `text` / `manual_code`: labelled ordinary input with actual provider prompt; no universal fake six-digit mask.
- `select`: actual supplied options with supplied descriptions; no invented defaults.
- `auth_url`: clear external authorization link, opened only by the user, preserving safe URL policy.
- `device_code`: actual code with explicit copy action and verification link.
- `progress/info`: show current short message, not an ever-growing wall of old events.

Answer the exact numbered prompt. A stale prompt conflict refreshes login state; it does not resend a secret to a new prompt. One active login/provider is respected. Closing explicitly cancels via the actual DELETE operation; a failed/lost cancellation remains visible. On service restart a missing memory-only login is `登录已结束`, not a permanent spinner. Completion refreshes actual catalog; a `done` event is not a claim that every model is usable.

### M03. Default model and reasoning

One `默认模型` searchable select, followed by `推理强度` with only supported levels. Existing source settings, not guessed model names, populate the form. Saving affects new research only, never captured active inputs.

Use explicit `保存设置` for the combined defaults rather than silently autosaving every intermediate model/reasoning combination. This design avoids submitting a transient invalid pair and makes the update observable. Dirty state shows `未保存`; pending `保存中`; confirmed actual response `已保存`; failed patch preserves edit with a short inline error. Navigation with a genuinely unsaved edit offers one scoped choice, not a warning for every click. No page-wide generic dirty guard for read-only views.

The old autosave behavior is not a design constraint for this complete replacement; its meaningful race/lost-response assertions must be retained or adapted to the explicit-save contract, not simply deleted.

### M04. Appearance controls

Three choices `跟随系统`, `浅色`, `深色`, matching F01. A small preview swatch is optional; no duplicate theme toggles with conflicting local state. Theme is local preference, not a PATCH to unsupported server settings. Selection applies immediately and persists when possible. All menus/dialogs/charts use the same mode.

### M05. Usage facts

A compact secondary fact group: actual input/output tokens and active native tasks. Unknown cost reads `未提供`, not free/zero. Scope is workspace usage because the API is not per-study billing. No token-usage hero, decorative progress gauge, savings estimate or pseudo-productivity score. Native task count is not the count of successful studies.

<a id="extensions-components"></a>
## 12. Extension components

### X01. Extension collection

Table/list: **扩展名称**, **类型**, **版本**, **启用设置**, **当前状态**. Versions remain `未提供` if absent; no version badge is added to the marketing/header identity. Long names wrap/read fully. Kind is a fact, not an invented category description.

The switch expresses desired enabled state. A separate status expresses actual `已加载`, `已停用`, `待重启`, `加载失败`. Never use the switch alone to imply successful runtime activation. Required host/science extensions are read-only with a short `必需` label; do not knowingly offer an unsupported disable mutation.

### X02. Toggle feedback and errors

PATCH shows row-local pending state, preserves previous actual runtime state and accepts only the returned preference/state. Error returns the switch to the known server value and leaves a short inline cause. Unknown network result triggers a read before claiming either state.

`待重启` gets one concise `重启后生效` message. There is no restart button because the API has none, and this task does not authorize restarting live services. Installation/upgrade/marketplace controls are absent. An actual load error has `技术详情`, not a fake repair action.

<a id="access-components"></a>
## 13. Owner access components

### A01. Login/setup panel

A focused max-400px form inside the same theme system, product name and one action. Setup mode: `设置工作台密码`, labelled password and confirmation checked locally, `创建密码`. Login mode: `登录工作台`, password, supported remember checkbox `记住登录`, `登录`.

Use appropriate current/new-password autocomplete. Password reveal is labelled and keyboard reachable. Do not add username, role, invitation, organization, 2FA or password-strength gates not required by the actual product. Backend/password failures stay inline; loading does not claim successful authentication. Owner password never appears in route, logs, request persistence or design assets.

### A02. Session expiry and logout

Expiry interrupts protected mutations with actual auth state. Preserve nonsecret original pending commands without replaying them automatically after login. Restore the intended destination after authentication; the owner deliberately chooses `核对提交` if needed.

Logout calls actual DELETE and reflects the returned session. If it fails/has unknown result, do not silently show success. Provider authentication and research stop are not coupled to logout; leaving the workbench does not cancel remote research.

<a id="states"></a>
## 14. State, copy and recovery matrices

### 14.1 Distinct state dictionaries

| Context | Raw fact | Primary visible wording |
| --- | --- | --- |
| Active research | RUNNING | `研究中` |
| Inactive research without final | IDLE | `当前无执行` |
| Independent final | STARTED | `独立评估中` |
| Independent final | COMPUTED | `已计算` |
| Independent final | FAILED | `独立评估失败` |
| Independent final | STOPPED | `已停止` with separately read execution state |
| Trial | STARTED / COMPUTED / FAILED / REJECTED | `计算中` / `已计算` / `失败` / `已拒绝` |
| Submission | queued / placed / done / unanswered | `待处理` / `已送入执行` / `已回答` / `未完成回答` |
| Native task | pending / running / waiting / completing / terminal | `待执行` / `执行中` / `等待中` / `收尾中` / actual terminal outcome |
| Extension | loaded / disabled / pending_restart / error | `已加载` / `已停用` / `待重启` / `加载失败` |

An unknown state uses original code and neutral presentation. No status dictionary is shared across unrelated scientific/provider/receipt/task enums merely because strings look similar.

### 14.2 Short error copy and valid repair

| Actual code/context | Visible message | Valid action |
| --- | --- | --- |
| `authentication_required` | `登录已过期` | `重新登录` |
| `invalid_password` | `密码不正确` | Correct password in place |
| `provider_status_unavailable` | `模型状态暂不可用` | Retry read / model settings |
| `model_unavailable` | `所选模型不可用` | Choose actual available model |
| `thinking_level_unsupported` | `模型不支持此推理强度` | Correct reasoning choice |
| `dataset_not_ready` | `数据集尚未就绪` | Open Data |
| `request_id_conflict` | `原提交内容不一致` | Inspect preserved original command; never swap ID automatically |
| Mutation network loss | `提交结果待核对` | Replay original request/body |
| `independent_evidence_missing` | `缺少独立评估结果` | Open originating study |
| `heldout_observations_missing` | `独立样本无有效观测` | Open recorded evidence |
| `strategy_adapter_not_configured` | `数据集未配置组合策略` | Open Data; no fake adapter fallback |
| `research_dataset_mismatch` | `所选研究的数据不一致` | Correct member/data selection |
| `strategy_selection_mismatch` | `研究评估区间不一致` | Correct selected members |
| `portfolio_weights_invalid` | `权重须为正且合计为 1` | Correct exact input |
| `target_expired` | `目标已过期` | Read original result; no fake renewal |
| `target_delivery_requires_reconciliation` | `已有目标待核对` | Open existing affected observation when identifiable |
| `account_snapshot_out_of_order` | `账户快照顺序异常` | Retain last actual snapshot; inspect original code |
| Unknown public problem | `操作未完成` | Actual safe retry/read and technical code |

Only translate known public errors. Do not echo arbitrary stack traces, HTML responses or secret-bearing internal messages into the app. This is ordinary public-error handling, not a new permission framework.

### 14.3 Interaction continuity

Query refresh is allowed to be automatic; external-effect mutation is not retried automatically under a different identity. Nonsecret uncertain research/portfolio/observation command envelopes survive route/reload in the same tab. Provider secret prompts do not persist. Current draft, edited defaults and original admitted request are different objects; never overwrite one with another.

Poll visible active research detail approximately every 2 seconds and its collection less often, pause background-tab polling and retain actual last facts. Portfolio status may be polled while queued. Observation collection polling reads stored state only; account refresh remains explicit. Polling interval is observation cadence, not a research timeout or a remote-health statement. Do not add a raw scroll listener or render-loop animation for status.

<a id="contracts"></a>
## 15. Minimal backend/contract changes

Implement presentation facts from already-recorded source data. All additions are optional/additive for historical compatibility; missing historical fields stay missing/unknown. No migrations, scheduler, computation, budget/exposure or native-engine behavior changes are required.

| Source DTO | Addition/preservation | Authoritative source | Why needed |
| --- | --- | --- | --- |
| `Metric` | Optional availability `status`, partition and component attribution when actually recorded | Existing native wrapper/partition/component fields in `metrics()` | R06/F11 must not hide unavailable-IC causes or mislabel component statistics |
| `ResearchDetail.final` | Separate optional `error` from selection `reason` | `study.final.error` and `study.final.reason` independently | Final failure is currently obscured when a reason exists |
| `TaskView` | Optional terminal `outcome` distinct from lifecycle `status` | Native `record.state.outcome.status` | `terminal` can mean failed/aborted, not completed |
| `ResearchDetail` | Optional original registered dataset ID and captured display facts for new admissions where practical | Original application request; new admission may capture the public dataset metadata beside the original request | Name/source cannot be reconstructed as historical fact from today's registry; full-input/older records remain explicit gaps |
| `PortfolioView` | Optional recorded member weights for detail | Captured `PortfolioFact.weights`/original input, not reconstructed normalization | P04 must show actual allocation after reload |
| `ObservationView` | Optional captured connection identity and snapshot provenance | Original `ObservationFact.connection/input` and accepted snapshot `clientId/accountId/sequence/nativeClockNs` | O04 must identify what account/source a snapshot belongs to, without exposing URL/token/credentials |

Keep domain validation in the service. Do not implement a second portfolio-eligibility state machine; clear local prerequisites and actual admission errors suffice. Preserve exact integer/decimal/nanosecond strings. Metrics with no recorded unit must not acquire an invented one. Scientific statuses remain original facts.

New captured metadata is an application display fact, not a modified scientific input, and must not change replay comparison or original request/body association. Original command replay returns the original receipt after settings/dataset changes. Existing docs/state reopen unchanged; legacy null fields are handled explicitly.

<a id="implementation"></a>
## 16. Implementation boundaries and order

### 16.1 Intended code organization

Use the existing React/Vite runtime and official packages. The names below identify responsibilities, not a mandatory new folder framework:

- `apps/web/src/App.tsx`, `main.tsx`: session, application frame, route and lazy workspace boundaries.
- `apps/web/src/styles.css` and a small theme module: new semantic tokens, typographic/layout rules and light/dark system preference. No inherited old style sheet as a base.
- `apps/web/src/ui.tsx`: only repeated status/empty/error/value/heading behavior that earns reuse. Official buttons/fields/tabs stay official components.
- `apps/web/src/research.tsx`: R01-R09, with local subcomponents split when actual size/readability warrants it.
- `apps/web/src/data.tsx`: D01-D03, new first-class data workspace.
- `apps/web/src/portfolio.tsx`: P01-P06.
- `apps/web/src/observations.tsx`: O01-O06.
- `apps/web/src/settings.tsx`, `model-login.tsx`: M01-M05 and actual provider ceremony.
- `apps/web/src/extensions.tsx`, `auth.tsx`: X01-X02/A01-A02.
- `apps/web/src/api.ts`, `request-intent.ts`: inspect original integration behavior, retain correct HTTP/error/replay mechanics and adapt minimally. Do not copy old presentation decisions from callers.
- `apps/web/src/equity-plot.tsx`: actual ECharts projection, accessible exact data and theme updates.
- `packages/contracts/src/index.ts`, `apps/service/src/projections.ts` plus original read handlers/admission facts where needed: additive display facts only.

No hand edits to historical generated sources. Remove dead old UI styles/callers only after new flows replace them. Preserve licenses, original worktree changes and migrations.

### 16.2 Ordered delivery

1. Owner reviews this component specification and labelled visual prototypes. Confirm material changes together, not per file/control.
2. Add minimal DTO/projection tests for status/error/partition/outcome/provenance and exact legacy-null behavior; implement the source changes.
3. Build new shell/theme/access and shared repeated patterns; then research/data. Verify main research job before optional consumers.
4. Replace portfolio/observation and model/settings/extensions with the specified interactions. Preserve request replay and provider prompt identities.
5. Adapt meaningful unit/browser assertions to new behavior and selectors. Add a disposable real HTTP-service browser scenario using official Faux Provider without API interception. Keep mocked transport tests for deliberate failure/loading situations and label their scope.
6. Inspect desktop/tablet/mobile screenshots in light/dark, keyboard and reduced motion. Compare to the accepted component intent, correct discrepancies, rerun affected checks.
7. Update actual architecture/usage documentation; run applicable local checks and exact-Head hosted CI; obtain independent read-only and required GitHub Codex read-only review, fix findings, then merge and verify remote `dev`. Do not deploy or restart live services/session daemon.

<a id="acceptance"></a>
## 17. Observable acceptance plan

These are acceptance scenarios, not claims of completed validation or new approval machinery.

| ID | Scenario | Expected observable result | Verification |
| --- | --- | --- | --- |
| V01 | New owner, empty installation | Setup/login works, no invented data/model/curve | Real disposable service + browser |
| V02 | No configured model | New-study state has direct model-setting repair; no fake option | Browser controlled catalog + actual service |
| V03 | No dataset/nonready data | Precise missing/not-ready state; no silent substitution | HTTP/browser |
| V04 | Select dataset from Data | New form visibly carries the explicit dataset ID | Browser |
| V05 | Model supports different reasoning levels | Only supported options submit; default pair remains valid | Unit + browser + service |
| V06 | Explicit zero/large valid trial budget | UI follows actual existing integer admission; no arbitrary extra cap | Unit/service |
| V07 | Lost research POST response, then changed defaults | Same original body/ID resolves same original receipt | Transport-failure browser + service replay test |
| V08 | Uncertain form route/reload | Original command persists; no duplicate research appears | Browser actual HTTP and injected response-loss case |
| V09 | Native cursor has another page | Load more retains previous records and filters; later record reachable | Browser cursor test |
| V10 | No loaded filter match while cursor exists | Clear-filter/load-more remain reachable, no global no-results claim | Browser |
| V11 | Direct link outside loaded list | Original detail opens immediately and reload/back preserves tab | Browser |
| V12 | Computed trial without final result | No independent-pass/success claim | Unit/browser |
| V13 | Final computed with null constant-score IC | `不可用` + `分数恒定`, not zero/pass | Projection unit + browser |
| V14 | Component diagnostic plus portfolio statistic | Correct partition/component/account grouping | Projection unit + browser |
| V15 | Failed final with reason and error | Selection reason and actual failure both accessible | Service + browser |
| V16 | Trial failures/rejections and selected trial | All attempts remain; selected original trial opens matching row | Service + browser |
| V17 | Terminal failed/aborted task | Correct outcome, not generic completion | Projection unit + browser |
| V18 | Null dates/cost/historical metadata | Unknown/omitted, never fabricated | Projection + browser |
| V19 | Multiple portfolio members, exact weights | Exact positive sum validation, no normalization; actual API errors mapped | Unit/service/browser |
| V20 | Queued/failed/computed backtest | Real status and inputs; no zero curve or fake profitability | Real service protocol + native-core CI |
| V21 | Equity strings beyond JS precision, null gap | Exact data readable, chart only display conversion, no gap fill | Unit + browser |
| V22 | Expired/no target | No renewal/delivery success/control invented | Service/browser |
| V23 | Uncertain target delivery | Original receipt reconciliation, no second offer/extended expiry | Existing actual downstream protocol + browser |
| V24 | ACK with missing/failed account snapshot | `已接收` receipt distinct from unavailable account facts | Service/browser |
| V25 | Stop observation | Local STOPPED, original receipt retained, no remote cancellation claim | Service/browser |
| V26 | Stored account snapshot refreshed unsuccessfully | Last actual time/value remains, failure visible | Service/browser |
| V27 | Provider OAuth/device/manual/select/secret prompts | Correct actual prompt identity and cancellation; no secret persistence | Service/browser |
| V28 | Provider login disappears after restart | Finite ended state, not permanent loading | Service/browser |
| V29 | Explicit defaults save failure/race | Edited pair retained, saved state follows actual latest accepted response | Unit/browser/service |
| V30 | Extension toggle pending restart/required extension | Preference distinct from runtime; no fake reload/restart button | Service/browser |
| V31 | 1440x900 and 1280x720 desktop | Clear hierarchy, no CTA/header overflow, meaningful first-screen facts | Screenshot inspection |
| V32 | 1024x768 tablet; 390x844 and 320px-wide mobile | Explicit composition change, all actions reachable, no page horizontal overflow | Browser screenshots/layout assertions |
| V33 | Light/dark/system change and reduced motion | Consistent popups/chart/text; no unnecessary animation | Browser screenshots/media emulation |
| V34 | Keyboard-only, zoom, screen-reader status semantics | Focus visible/restored; short status updates announced without polling chatter | Keyboard/manual semantics + axe |
| V35 | Long goal/provider ID/error/decimal string | Full original content accessible; no broken layout/hover-only essential value | Browser extreme-content cases |
| V36 | Service/query error vs empty collection | Distinct recoverable states, previous valid facts retained on refresh | Browser failure cases |
| V37 | Session expiry/logout during original pending command | No secret leak, no automatic duplicate effect, intentional return/reconcile | Service/browser |
| V38 | Final content audit | No filler introductions, em dashes, fake stats, raw JSON, dead actions or lengthy generated explanations | Visible-string/source audit + screenshots |

Actual human task usability remains a separate owner check: start a study, find the selected trial, explain whether independent evidence exists, locate a failed attempt, and distinguish receipt from account observation. Record actual completion/errors if performed; never report success from an agent's own walkthrough as if the owner participated.

Run `make check-web`, the adapted browser suite and `make check-ts` for changed contracts/service. Real native/core/science verification is selected by the existing CI change scope; old binaries, synthetic protocol success and old-Head checks are not replacements. Lighthouse/performance and accessibility results must name the actual route/environment and remain measurements, not promises. Do not claim LCP/INP targets were met without measurements.

<a id="decisions"></a>
## 18. Decisions, alternatives and remaining acceptance

- **Chosen:** one research-oriented workspace with nonmodal detail. **Rejected:** an overview dashboard followed by project/agent/task administration pages. Reason: it adds navigation and foregrounds implementation instead of evidence.
- **Chosen:** exact comparison tables plus focused record detail. **Rejected:** a wall of equally weighted cards. Reason: adjacent numerical comparison and long hypotheses require different presentation.
- **Chosen:** a single new-study form. **Rejected:** a rigid wizard or raw JSON editor. Reason: goal/data/model/budget are interdependent and fit a focused page.
- **Chosen:** explicit combined default-model save. **Rejected:** invisible per-field autosave. Reason: the model/reasoning pair is one validated setting; the owner must see what is saved.
- **Chosen:** short default copy and accessible original detail. **Rejected:** deleting evidence to make screens short or inserting generated explanatory essays. Reason: concise UI must not destroy provenance.
- **Chosen:** minimal additive read facts. **Rejected:** new retry/resume/finalize/eligibility state machines. Reason: the actual service owns scientific behavior and command identity.
- **Chosen:** task-owned browser service and official Faux Provider for deterministic interactions. **Rejected:** claiming intercepted HTTP tests prove real-service or scientific acceptance.

**Owner decision:** The overall delivery goal is accepted, but visual design is not. The owner first requested component-level detail, then rejected the first detailed prototype with `UI 不够精美，过度粗糙`. Revision 2 replaces that visual treatment rather than treating technical checks as aesthetic acceptance. This specification and the updated references remain unapproved for production implementation. No completed application, owner usability test, final-Head review or merge is claimed.

<a id="prototypes"></a>
## 19. Labelled visual references and actual inspection

Every reference below was rendered in Chromium from the separate [design-only React prototype](assets/prototype.tsx) and [prototype stylesheet](assets/prototype.css), using official Ant components. Its top bar explicitly says that records are examples and it does not connect to a service. Numbers, sample identities and hypotheses are prototype content, not scientific/account evidence. There is no fabricated equity curve. Revision 2 actually serves the official Geist/Geist Mono font assets locally and uses Phosphor icons; it no longer relies on an unavailable-font fallback. These dependencies and generated bundles remain in the isolated preview directory, not the application or vendored repository source.

| Reference | Purpose |
| --- | --- |
| [Research, light desktop](assets/research-light.png) | List/detail hierarchy, independent unavailable metric and optional downstream action |
| [Research, dark desktop](assets/research-dark.png) | Same hierarchy and semantic palette in dark mode |
| [Research, light mobile](assets/research-mobile.png) | Separate detail view, explicit back action and reachable short controls |
| [Research, dark mobile](assets/research-mobile-dark.png) | Dark mobile parity |
| [New study, desktop](assets/new-study.png) | Focused goal/data/model/reasoning/budget composition |
| [New study, mobile](assets/new-study-mobile.png) | Explicit single-column inputs and nonwrapping actions |
| [Trial comparison and expansion](assets/trials.png) | Adjacent facts and one selected expanded original trial |
| [Data collection](assets/data.png) | Source/readiness/coverage facts without unsupported ingestion controls |
| [Portfolio builder](assets/portfolio-builder.png) | Explicit original member, exact weight and capital; no speculative curve |
| [Receipt/account distinction, dark](assets/observation-dark.png) | Unknown receipt separately from absent account snapshot |
| [Model settings](assets/settings.png) | Quiet provider/default/appearance/usage groups, not a token-count dashboard |
| [Extensions](assets/extensions.png) | Desired preference and pending runtime state as separate facts |

The prototype demonstrates representative composition, not all 51 components or all 38 acceptance cases. Its navigation/short controls are functional design interactions; provider, research, account and settings actions never call a real service. Production must implement the complete behavioral contracts above rather than copy sample records or placeholder dialogs.

Observed design-only checks: strict TypeScript passed; 12 screenshots saved; 28 view/width layout checks found no page overflow; six selected prototype axe runs reported zero tagged violations; four interaction checks passed; no checked-view page errors. The task records scope and exclusions. No production Lighthouse measurement, owner usability test, real-service integration or final-Head CI/review has occurred.
