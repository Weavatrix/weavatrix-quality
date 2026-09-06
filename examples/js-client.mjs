/**
 * Typed JS boundary over the native wvq / wvq-mcp binaries.
 * Run after: npm install wvq@0.1.0-alpha.1
 */
import { WvqClient } from '@weavatrix/wvq'
import { WvqMcpClient } from '@weavatrix/wvq/mcp'

const repo = process.cwd()
const change = process.env.WVQ_CHANGE ?? 'current'

const quality = new WvqClient({ repo })
const plan = await quality.plan({ change, base: 'origin/main', head: 'HEAD' })
console.log('plan obligations', plan.obligations?.length ?? plan)

const run = await quality.run({
  change,
  base: 'origin/main',
  head: 'HEAD',
  scope: 'impacted',
  evidencePolicy: 'minimal',
})
console.log('run', run.run_id, run.outcome)

const verify = await quality.verify({ change })
console.log('verify', verify.state, verify.verdict, verify.base, verify.head)

const mcp = new WvqMcpClient({ repo, profile: 'default', change })
const status = await mcp.call('quality_status', { change })
console.log('mcp status', status)
