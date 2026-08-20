import { existsSync } from 'node:fs'
import { createRequire } from 'node:module'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const PLATFORMS = {
    'win32 x64': 'win32-x64',
    'win32 arm64': 'win32-arm64',
    'darwin x64': 'darwin-x64',
    'darwin arm64': 'darwin-arm64',
    'linux x64': 'linux-x64',
    'linux arm64': 'linux-arm64',
}

const KINDS = {
    wvq: { env: 'WVQ_BINARY', stem: 'wvq' },
    mcp: { env: 'WVQ_MCP_BINARY', stem: 'wvq-mcp' },
    bench: { env: 'WVQ_BENCH_BINARY', stem: 'wvq-bench' },
}

export function resolveBinary(kind = 'wvq') {
    const command = KINDS[kind]
    if (!command) throw new Error(`unknown WVQ binary kind: ${kind}`)
    const override = process.env[command.env]
    if (override) {
        if (!existsSync(override)) throw new Error(`${command.env} does not exist: ${override}`)
        return override
    }
    const key = `${process.platform} ${process.arch}`
    const platform = PLATFORMS[key]
    if (!platform) {
        throw new Error(`unsupported WVQ platform: ${key}; supported: win32/darwin/linux on x64/arm64`)
    }
    const filename = process.platform === 'win32' ? `${command.stem}.exe` : command.stem
    const bundled = join(dirname(fileURLToPath(import.meta.url)), 'native', platform, filename)
    if (existsSync(bundled)) return bundled
    const packageName = `@weavatrix/wvq-${platform}`
    const packaged = locatePackageBinary(packageName, filename)
    if (packaged) return packaged
    const workspace = locateWorkspaceBinary(filename)
    if (workspace) return workspace
    throw new Error(
        `native ${command.stem} for ${key} is missing; reinstall wvq or set ${command.env} to a matching binary`,
    )
}

function locatePackageBinary(packageName, filename) {
    const bases = [import.meta.url, process.argv[1], join(process.cwd(), 'package.json')]
    for (const base of bases) {
        if (!base) continue
        try {
            const packageJson = createRequire(base).resolve(`${packageName}/package.json`)
            const binary = join(dirname(packageJson), filename)
            if (existsSync(binary)) return binary
        } catch (error) {
            if (error?.code !== 'MODULE_NOT_FOUND') throw error
        }
    }
    return null
}

function locateWorkspaceBinary(filename) {
    const root = join(dirname(fileURLToPath(import.meta.url)), '..', '..', '..')
    for (const profile of ['release', 'debug']) {
        const binary = join(root, 'target', profile, filename)
        if (existsSync(binary)) return binary
    }
    return null
}
