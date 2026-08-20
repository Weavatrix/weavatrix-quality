import { spawn } from 'node:child_process'

import { resolveBinary } from './resolve-binary.mjs'

export function selectWvqInvocation(args) {
    if (args[0] === 'mcp') return { kind: 'mcp', label: 'wvq-mcp', args: args.slice(1) }
    if (args[0] === 'bench') return { kind: 'bench', label: 'wvq-bench', args: args.slice(1) }
    return { kind: 'wvq', label: 'wvq', args }
}

export function runNative(kind, label, args = process.argv.slice(2)) {
    let binary
    try {
        binary = resolveBinary(kind)
    } catch (error) {
        console.error(`${label}: ${error.message}`)
        process.exit(1)
    }
    if (['darwin', 'linux'].includes(process.platform) && typeof process.execve === 'function') {
        process.execve(binary, [binary, ...args], process.env)
    }
    const child = spawn(binary, args, { stdio: 'inherit', windowsHide: true, shell: false })
    child.on('error', (error) => {
        console.error(`${label}: failed to start native binary: ${error.message}`)
        process.exit(1)
    })
    child.on('exit', (code, signal) => {
        if (signal) process.kill(process.pid, signal)
        process.exit(code ?? 1)
    })
    for (const signal of ['SIGINT', 'SIGTERM']) process.on(signal, () => child.kill(signal))
}
