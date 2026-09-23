# Install the service skill with the Skills CLI

## Goal

Install the existing self-contained `quazonai` service-operation skill using the official skills.sh CLI, without a custom installer, duplicated skill, npm publication or repository checkout on the consuming project.

## Scope

- Baseline: main `a8825942e0d61760729c99cb57aa7886a17c2def`.
- Keep `skills/quazonai/SKILL.md` and its relative `references/` as the single package. The existing DESIGN service-skill contract and CLI/Mission behavior are unchanged.
- Add README commands for interactive project installation, explicit Codex/Claude Code global installation, listing and updates. Installing instructions does not provision a server, credential or MCP connection.
- Reuse the existing Web workflow and Node.js 24. Run official `skills@1.7.0` in a disposable home/project; compare every installed file with the exact checkout. No new workflow, application dependency or production service operation.

## Acceptance and evidence

- Discover the skill from the complete local checkout and install for Codex and Claude Code at project scope using the default installation method.
- Install the same PR Head from its public GitHub skill-directory URL, globally for both agents using copy mode. Compare the complete installed directories, including all reference files.
- Use isolated HOME, agent config directories and npm cache. Disable installation telemetry in CI; verification does not manufacture public install counts.
- GitHub Actions and the associated PR record actual command results. The current Head must pass applicable checks and receive an explicit clean read-only Codex review before merge. Do not infer public directory indexing from successful installation.

## Upstream contract

[Skills CLI](https://github.com/vercel-labs/skills) supplies repository discovery, agent placement, copy/symlink handling and updates. [skills.sh FAQ](https://skills.sh/docs/faq) describes public indexing via actual installation telemetry. The runtime skill contains neither these development notes nor CI scripts.
