/**
 * Broader WvqClient surface: spec, select, debt, status, explain, observe-only.
 *
 *   node examples/js/select-debt-explain.mjs
 */
import { WvqClient } from '@weavatrix/wvq'

const repo = process.env.WVQ_REPO ?? process.cwd()
const change = process.env.WVQ_CHANGE ?? 'current'
const base = process.env.WVQ_BASE ?? 'origin/main'
const head = process.env.WVQ_HEAD ?? 'HEAD'

const wvq = new WvqClient({ repo })

const spec = await wvq.specValidate({ change })
console.log('spec', spec)

const selected = await wvq.select({ change, base, head })
console.log('select', {
  algorithm: selected.algorithm,
  selected: selected.selected,
  uncovered: selected.uncovered_mandatory,
  complete: selected.selection_complete,
})

const debt = await wvq.debt({ change, base, head })
console.log('debt', {
  existing: debt.existing,
  new: debt.new,
  fixed: debt.fixed,
  returned: debt.returned,
  excepted: debt.excepted,
})

const status = await wvq.status()
console.log('status', status)

const verify = await wvq.verify({ change, observeOnly: true })
console.log('observe-only verify', verify.state, verify.observe_only)

const firstProof = verify.proofs?.[0]?.id
if (firstProof) {
  const explained = await wvq.explain(firstProof)
  console.log('explain', explained.kind, explained.summary)
}
