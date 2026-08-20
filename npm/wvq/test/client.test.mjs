import assert from 'node:assert/strict'
import { existsSync, readFileSync } from 'node:fs'
import test from 'node:test'

import { selectWvqInvocation } from '../bin/run-native.mjs'
import { WvqClient } from '../src/client.mjs'
import { WvqMcpClient } from '../src/mcp-client.mjs'

test('package entrypoint selects only the three fixed native programs', () => {
    assert.deepEqual(selectWvqInvocation(['status']), {
        kind: 'wvq', label: 'wvq', args: ['status'],
    })
    assert.deepEqual(selectWvqInvocation(['mcp', '--repo', '.']), {
        kind: 'mcp', label: 'wvq-mcp', args: ['--repo', '.'],
    })
    assert.deepEqual(selectWvqInvocation(['bench', '--repo', '.']), {
        kind: 'bench', label: 'wvq-bench', args: ['--repo', '.'],
    })
})

test('npm and MCP Registry metadata identify the same runnable package', () => {
    const packageJson = JSON.parse(readFileSync(new URL('../package.json', import.meta.url)))
    const packagedServer = new URL('../server.json', import.meta.url)
    const server = JSON.parse(readFileSync(
        existsSync(packagedServer) ? packagedServer : new URL('../../../server.json', import.meta.url),
    ))
    assert.equal(packageJson.mcpName, server.name)
    assert.equal(packageJson.version, server.version)
    assert.equal(server.packages[0].identifier, packageJson.name)
    assert.equal(server.packages[0].version, packageJson.version)
    assert.equal(server.packages[0].packageArguments[0].value, 'mcp')
})

test('typed client maps methods to bounded native argv', async () => {
    const calls = []
    const client = new WvqClient({
        repo: 'C:/repo',
        invoke: async (binary, args) => {
            calls.push({ binary, args })
            const command = args[2] === 'spec' ? `spec_${args[3]}` : args[2]
            return { command, body: { ok: true } }
        },
        binary: 'wvq-test',
    })

    assert.deepEqual(await client.specValidate({ change: 'live' }), { ok: true })
    assert.deepEqual(await client.run({
        change: 'live',
        base: 'base-sha',
        head: 'WORKTREE',
        scope: 'impacted',
        evidencePolicy: 'minimal',
    }), { ok: true })
    assert.deepEqual(calls, [{
        binary: 'wvq-test',
        args: ['--repo', 'C:/repo', 'spec', 'validate', '--change', 'live'],
    }, {
        binary: 'wvq-test',
        args: [
            '--repo', 'C:/repo', 'run', '--change', 'live', '--base', 'base-sha',
            '--head', 'WORKTREE', '--scope', 'impacted', '--evidence-policy', 'minimal',
        ],
    }])
})

test('client rejects unknown enum values before starting native code', async () => {
    let invoked = false
    const client = new WvqClient({
        repo: '.',
        binary: 'wvq-test',
        invoke: async () => {
            invoked = true
            return { command: 'run', body: {} }
        },
    })
    assert.throws(
        () => client.run({ change: 'live', base: 'HEAD', scope: 'quick' }),
        /scope must be impacted or all/,
    )
    assert.equal(invoked, false)
})

test('MCP client fixes authoring scope at process startup', async () => {
    const calls = []
    const client = new WvqMcpClient({
        repo: '/repo',
        profile: 'authoring',
        change: 'live',
        base: 'base-sha',
        head: 'WORKTREE',
        binary: 'wvq-mcp-test',
        invoke: async (binary, args, tool, input) => {
            calls.push({ binary, args, tool, input })
            return { valid: true }
        },
    })
    assert.deepEqual(await client.validate({ schema_v: 1, id: 'generated' }), { valid: true })
    assert.deepEqual(calls[0], {
        binary: 'wvq-mcp-test',
        args: [
            '--repo', '/repo', '--profile', 'authoring', '--change', 'live',
            '--base', 'base-sha', '--head', 'WORKTREE',
        ],
        tool: 'quality_test_validate',
        input: { program: { schema_v: 1, id: 'generated' } },
    })
    assert.deepEqual(
        await client.promote('preview-generated', { schema_v: 1, id: 'generated' }),
        { valid: true },
    )
    assert.deepEqual(calls[1], {
        binary: 'wvq-mcp-test',
        args: [
            '--repo', '/repo', '--profile', 'authoring', '--change', 'live',
            '--base', 'base-sha', '--head', 'WORKTREE',
        ],
        tool: 'quality_test_promote',
        input: {
            preview_id: 'preview-generated',
            program: { schema_v: 1, id: 'generated' },
        },
    })
    assert.deepEqual(
        await client.heal('generated', 1, [{
            edit: 'insert_wait', after: 0, condition: { kind: 'url', route: '/ready' },
        }], { screenshot: false }),
        { valid: true },
    )
    assert.deepEqual(calls[2], {
        binary: 'wvq-mcp-test',
        args: [
            '--repo', '/repo', '--profile', 'authoring', '--change', 'live',
            '--base', 'base-sha', '--head', 'WORKTREE',
        ],
        tool: 'quality_test_heal',
        input: {
            program_id: 'generated',
            expected_program_revision: 1,
            edits: [{
                edit: 'insert_wait', after: 0, condition: { kind: 'url', route: '/ready' },
            }],
            screenshot: false,
            trace: false,
        },
    })
})
