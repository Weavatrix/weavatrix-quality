/**
 * Authoring MCP helpers: draft → validate → preview → promote → heal.
 * Requires Playwright + browser.base_url in the target repo.
 *
 *   WVQ_CHANGE=checkout-fix node examples/js/mcp-authoring.mjs
 *
 * Set CANDIDATE_PROGRAM_JSON to a path with a candidate TestProgram, or the
 * script stops after draft (so an agent can fill the candidate).
 */
import { readFileSync } from 'node:fs'
import { WvqMcpClient } from '@weavatrix/wvq/mcp'

const repo = process.env.WVQ_REPO ?? process.cwd()
const change = process.env.WVQ_CHANGE ?? 'current'
const base = process.env.WVQ_BASE ?? 'origin/main'
const head = process.env.WVQ_HEAD ?? 'WORKTREE'

const authoring = new WvqMcpClient({
  repo,
  profile: 'authoring',
  change,
  base,
  head,
})

const draft = await authoring.draft({ useModel: false, tokenBudget: 8000 })
console.log('draft', {
  obligations: draft.obligations?.length,
  changed_files: draft.changed_files,
  truncated: draft.truncated,
})

const candidatePath = process.env.CANDIDATE_PROGRAM_JSON
if (!candidatePath) {
  console.log('Set CANDIDATE_PROGRAM_JSON to continue validate/preview/promote.')
  process.exit(0)
}

const candidateProgram = JSON.parse(readFileSync(candidatePath, 'utf8'))
const validated = await authoring.validate(candidateProgram)
console.log('validated', validated.program_id, validated.valid)

const preview = await authoring.preview(validated.program, {
  screenshot: true,
  trace: true,
})
console.log('preview', {
  preview_id: preview.preview_id,
  passed: preview.passed,
  asserted: preview.asserted,
  contradicted: preview.contradicted,
})

if (!preview.passed) process.exit(1)

const promoted = await authoring.promote(preview.preview_id, validated.program)
console.log('promoted', promoted)

const healed = await authoring.heal(promoted.program_id, promoted.program_revision, [
  { edit: 'insert_wait', after: 0, condition: { kind: 'url', route: '/ready' } },
])
console.log('healed', healed)
