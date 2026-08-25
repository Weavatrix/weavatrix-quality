import { resolveBinary } from '../bin/resolve-binary.mjs'
import { invokeNativeJson } from './process.mjs'

const SCOPES = new Set(['impacted', 'all'])
const EVIDENCE_POLICIES = new Set(['standard', 'minimal', 'none'])
const PURPOSES = new Set(['spec', 'implementation', 'review'])
const MODEL_KINDS = new Set(['planning', 'runtime', 'browser_escape', 'vision'])

export class WvqClient {
    #repo
    #binary
    #invoke
    #timeoutMs

    constructor({ repo = '.', binary, invoke = invokeNativeJson, timeoutMs } = {}) {
        this.#repo = requireText('repo', repo)
        this.#binary = binary || resolveBinary('wvq')
        this.#invoke = invoke
        this.#timeoutMs = timeoutMs
    }

    specValidate({ change = 'current', signal } = {}) {
        return this.#call('spec_validate', ['spec', 'validate', '--change', requireText('change', change)], signal)
    }

    specSeal({ change = 'current', signal } = {}) {
        return this.#call('spec_seal', ['spec', 'seal', '--change', requireText('change', change)], signal)
    }

    analyze({ change = 'current', purpose = 'implementation', tokenBudget = 4_000, signal } = {}) {
        requireEnum('purpose', purpose, PURPOSES)
        requirePositiveInteger('tokenBudget', tokenBudget)
        return this.#call('analyze', [
            'analyze', '--change', requireText('change', change), '--purpose', purpose,
            '--token-budget', String(tokenBudget),
        ], signal)
    }

    debt({ change = 'current', base = 'HEAD', head = 'WORKTREE', signal } = {}) {
        return this.#rangeCall('debt', change, base, head, signal)
    }

    select({ change = 'current', base = 'HEAD', head = 'WORKTREE', signal } = {}) {
        return this.#rangeCall('select', change, base, head, signal)
    }

    run({
        change = 'current', base = 'HEAD', head = 'WORKTREE', scope = 'impacted',
        evidencePolicy = 'standard', signal,
    } = {}) {
        requireEnum('scope', scope, SCOPES)
        requireEnum('evidencePolicy', evidencePolicy, EVIDENCE_POLICIES)
        return this.#call('run', [
            'run', '--change', requireText('change', change),
            '--base', requireText('base', base), '--head', requireText('head', head),
            '--scope', scope, '--evidence-policy', evidencePolicy,
        ], signal)
    }

    record({
        change = 'current', base = 'HEAD', head = 'WORKTREE', route = '/', fixtureValues = {},
        idleTimeoutMs = 3_000, maxEvents = 200, headless, signal,
    } = {}) {
        requirePositiveInteger('idleTimeoutMs', idleTimeoutMs)
        requirePositiveInteger('maxEvents', maxEvents)
        if (typeof fixtureValues !== 'object' || fixtureValues === null || Array.isArray(fixtureValues)
            || Object.entries(fixtureValues).some(([key, value]) => !key.trim() || typeof value !== 'string')) {
            throw new TypeError('fixtureValues must contain non-empty names and string values')
        }
        if (headless !== undefined && typeof headless !== 'boolean') {
            throw new TypeError('headless must be boolean')
        }
        const args = [
            'record', '--change', requireText('change', change),
            '--base', requireText('base', base), '--head', requireText('head', head),
            '--route', requireText('route', route), '--idle-ms', String(idleTimeoutMs),
            '--max-events', String(maxEvents), '--fixtures-json', JSON.stringify(fixtureValues),
        ]
        if (headless !== undefined) args.push('--headless', String(headless))
        return this.#call('record', args, signal)
    }

    ingestJournal({
        change = 'current', base = 'HEAD', head = 'WORKTREE', file, journal, signal,
    } = {}) {
        if ((file && journal) || (!file && !journal)) {
            throw new TypeError('ingestJournal requires exactly one of file or journal')
        }
        const args = [
            'ingest-journal', '--change', requireText('change', change),
            '--base', requireText('base', base), '--head', requireText('head', head),
        ]
        if (file) args.push('--file', requireText('file', file))
        if (journal) {
            if (typeof journal !== 'string' || journal.trim() === '') {
                throw new TypeError('journal must be non-empty JSON')
            }
            args.push('--journal', journal)
        }
        return this.#call('ingest_journal', args, signal)
    }

    status({ signal } = {}) {
        return this.#call('status', ['status'], signal)
    }

    verify({ change = 'current', observeOnly = false, signal } = {}) {
        const args = ['verify', '--change', requireText('change', change)]
        if (observeOnly) {
            args.push('--observe-only', 'true')
        }
        return this.#call('verify', args, signal)
    }

    explain(id, { signal } = {}) {
        return this.#call('explain', ['explain', requireText('id', id)], signal)
    }

    plan({ change = 'current', signal } = {}) {
        return this.#call('plan', ['plan', '--change', requireText('change', change)], signal)
    }

    model({ change = 'current', kind, prompt, signal } = {}) {
        requireEnum('kind', kind, MODEL_KINDS)
        return this.#call('model', [
            'model', '--change', requireText('change', change), '--kind', kind,
            '--prompt', requireText('prompt', prompt),
        ], signal)
    }

    #rangeCall(command, change, base, head, signal) {
        return this.#call(command, [
            command, '--change', requireText('change', change),
            '--base', requireText('base', base), '--head', requireText('head', head),
        ], signal)
    }

    async #call(expected, args, signal) {
        const envelope = await this.#invoke(
            this.#binary,
            ['--repo', this.#repo, ...args],
            { signal, timeoutMs: this.#timeoutMs },
        )
        if (!envelope || envelope.command !== expected || !Object.hasOwn(envelope, 'body')) {
            throw new Error(`wvq returned ${envelope?.command ?? 'no command'}, expected ${expected}`)
        }
        return envelope.body
    }
}

function requireText(name, value) {
    if (typeof value !== 'string' || !value.trim()) throw new TypeError(`${name} must be non-empty text`)
    return value
}

function requireEnum(name, value, allowed) {
    if (!allowed.has(value)) throw new TypeError(`${name} must be ${[...allowed].join(' or ')}`)
}

function requirePositiveInteger(name, value) {
    if (!Number.isSafeInteger(value) || value <= 0) throw new TypeError(`${name} must be a positive integer`)
}

export { resolveBinary }
