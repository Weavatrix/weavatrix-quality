import { spawn } from 'node:child_process'

const MAX_STDOUT_BYTES = 16 * 1024 * 1024
const MAX_STDERR_BYTES = 2 * 1024 * 1024

export async function invokeNativeJson(binary, args, options = {}) {
    const { timeoutMs = 15 * 60_000, signal } = options
    const result = await collectProcess(binary, args, { timeoutMs, signal })
    if (result.code !== 0) {
        throw new Error(`wvq exited with code ${result.code}: ${result.stderr.trim() || 'no diagnostic'}`)
    }
    try {
        return JSON.parse(result.stdout)
    } catch (error) {
        throw new Error(`wvq returned malformed JSON: ${error.message}`)
    }
}

export async function invokeMcpTool(binary, args, tool, input, options = {}) {
    const { timeoutMs = 15 * 60_000, signal } = options
    const request = JSON.stringify({
        jsonrpc: '2.0',
        id: 1,
        method: 'tools/call',
        params: { name: tool, arguments: input },
    })
    const result = await collectProcess(binary, args, {
        timeoutMs,
        signal,
        stdin: `${request}\n`,
    })
    if (result.code !== 0) {
        throw new Error(`wvq-mcp exited with code ${result.code}: ${result.stderr.trim() || 'no diagnostic'}`)
    }
    const line = result.stdout.split(/\r?\n/u).find((item) => item.trim())
    if (!line) throw new Error('wvq-mcp returned no JSON-RPC response')
    let reply
    try {
        reply = JSON.parse(line)
    } catch (error) {
        throw new Error(`wvq-mcp returned malformed JSON-RPC: ${error.message}`)
    }
    if (reply.error) {
        throw new Error(`wvq-mcp ${reply.error.code}: ${reply.error.message}`)
    }
    const content = reply.result?.content
    const text = Array.isArray(content) && content[0]?.type === 'text' ? content[0].text : null
    if (typeof text !== 'string') throw new Error('wvq-mcp response omitted text content')
    try {
        return JSON.parse(text)
    } catch (error) {
        throw new Error(`wvq-mcp tool content is malformed JSON: ${error.message}`)
    }
}

function collectProcess(binary, args, { timeoutMs, signal, stdin } = {}) {
    return new Promise((resolve, reject) => {
        const child = spawn(binary, args, {
            stdio: ['pipe', 'pipe', 'pipe'],
            windowsHide: true,
            shell: false,
        })
        let stdout = Buffer.alloc(0)
        let stderr = Buffer.alloc(0)
        let settled = false
        const finish = (action) => {
            if (settled) return
            settled = true
            clearTimeout(timer)
            signal?.removeEventListener('abort', abort)
            action()
        }
        const abort = () => {
            child.kill()
            finish(() => reject(new Error('wvq process aborted')))
        }
        const timer = setTimeout(() => {
            child.kill()
            finish(() => reject(new Error(`wvq process exceeded ${timeoutMs}ms`)))
        }, timeoutMs)
        timer.unref?.()
        signal?.addEventListener('abort', abort, { once: true })
        if (signal?.aborted) return abort()
        child.on('error', (error) => finish(() => reject(error)))
        child.stdout.on('data', (chunk) => {
            stdout = appendBounded(stdout, chunk, MAX_STDOUT_BYTES, child, finish, reject, 'stdout')
        })
        child.stderr.on('data', (chunk) => {
            stderr = appendBounded(stderr, chunk, MAX_STDERR_BYTES, child, finish, reject, 'stderr')
        })
        child.on('close', (code) => finish(() => resolve({
            code: code ?? 1,
            stdout: stdout.toString('utf8'),
            stderr: stderr.toString('utf8'),
        })))
        child.stdin.on('error', (error) => finish(() => reject(error)))
        child.stdin.end(stdin ?? '')
    })
}

function appendBounded(current, chunk, limit, child, finish, reject, stream) {
    if (current.length + chunk.length <= limit) return Buffer.concat([current, chunk])
    child.kill()
    finish(() => reject(new Error(`wvq ${stream} exceeded ${limit} bytes`)))
    return current
}
