/**
 * Plan → run → verify via the typed WvqClient boundary.
 *
 *   npm install --save-dev @weavatrix/wvq@0.1.0-alpha.1
 *   node examples/js/plan-run-verify.mjs
 */
import { WvqClient } from '@weavatrix/wvq'

const repo = process.env.WVQ_REPO ?? process.cwd()
const change = process.env.WVQ_CHANGE ?? 'current'
const base = process.env.WVQ_BASE ?? 'origin/main'
const head = process.env.WVQ_HEAD ?? 'HEAD'

const wvq = new WvqClient({ repo })

const plan = await wvq.plan({ change })
console.log('plan', {
  obligations: plan.obligations?.length,
  gaps: plan.gaps?.length,
  existing_proofs: plan.existing_proofs?.length,
})

const run = await wvq.run({
  change,
  base,
  head,
  scope: 'impacted',
  evidencePolicy: 'minimal',
})
console.log('run', {
  run_id: run.run_id,
  outcome: run.outcome,
  scope: run.scope,
  scope_reason: run.scope_reason,
  selected: run.selected_test_count,
  available: run.available_test_count,
})

const verify = await wvq.verify({ change })
console.log('verify', {
  state: verify.state,
  verdict: verify.verdict,
  blocking: verify.blocking,
  base: verify.base,
  head: verify.head,
  proven: verify.quality?.proof?.proven,
  unproven: verify.quality?.proof?.unproven,
})

if (verify.blocking) process.exitCode = 2
