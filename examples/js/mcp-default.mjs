/**
 * Default MCP profile via WvqMcpClient (one-shot tool calls).
 *
 *   node examples/js/mcp-default.mjs
 */
import { WvqMcpClient } from '@weavatrix/wvq/mcp'

const repo = process.env.WVQ_REPO ?? process.cwd()
const change = process.env.WVQ_CHANGE ?? 'current'

const mcp = new WvqMcpClient({ repo, profile: 'default', change })

const context = await mcp.call('quality_context', {
  change,
  purpose: 'implementation',
  token_budget: 4000,
})
console.log('context obligations', context)

const plan = await mcp.call('quality_plan', { change })
console.log('plan', plan)

const status = await mcp.call('quality_status', {})
console.log('status', status)

const verify = await mcp.call('quality_verify', { change })
console.log('verify', verify)
