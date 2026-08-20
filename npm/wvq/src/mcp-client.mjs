import { resolveBinary } from '../bin/resolve-binary.mjs'
import { invokeMcpTool } from './process.mjs'

const PROFILES = new Set(['default', 'recovery', 'protection', 'authoring'])

export class WvqMcpClient {
    #binary
    #args
    #invoke
    #timeoutMs

    constructor({
        repo = '.', profile = 'default', change = 'current', base = 'HEAD', head = 'WORKTREE',
        binary, invoke = invokeMcpTool, timeoutMs,
    } = {}) {
        if (!PROFILES.has(profile)) throw new TypeError(`profile must be ${[...PROFILES].join(' or ')}`)
        this.#binary = binary || resolveBinary('mcp')
        this.#invoke = invoke
        this.#timeoutMs = timeoutMs
        this.#args = ['--repo', requireText('repo', repo)]
        if (profile !== 'default') {
            this.#args.push(
                '--profile', profile,
                '--change', requireText('change', change),
                '--base', requireText('base', base),
                '--head', requireText('head', head),
            )
        }
    }

    call(tool, input = {}, { signal, timeoutMs = this.#timeoutMs } = {}) {
        return this.#invoke(
            this.#binary,
            [...this.#args],
            requireText('tool', tool),
            requireObject('input', input),
            { signal, timeoutMs },
        )
    }

    draft({ tokenBudget = 8_000, useModel = false, signal } = {}) {
        if (!Number.isSafeInteger(tokenBudget) || tokenBudget <= 0) {
            throw new TypeError('tokenBudget must be a positive integer')
        }
        if (typeof useModel !== 'boolean') throw new TypeError('useModel must be boolean')
        return this.call('quality_test_draft', {
            token_budget: tokenBudget,
            use_model: useModel,
        }, { signal })
    }

    validate(program, { signal } = {}) {
        return this.call('quality_test_validate', {
            program: requireObject('program', program),
        }, { signal })
    }

    preview(program, { screenshot = true, trace = false, signal, timeoutMs } = {}) {
        if (typeof screenshot !== 'boolean' || typeof trace !== 'boolean') {
            throw new TypeError('screenshot and trace must be boolean')
        }
        return this.call('quality_test_preview', {
            program: requireObject('program', program),
            screenshot,
            trace,
        }, { signal, timeoutMs })
    }
}

function requireText(name, value) {
    if (typeof value !== 'string' || !value.trim()) throw new TypeError(`${name} must be non-empty text`)
    return value
}

function requireObject(name, value) {
    if (!value || typeof value !== 'object' || Array.isArray(value)) {
        throw new TypeError(`${name} must be an object`)
    }
    return value
}

export { resolveBinary }
