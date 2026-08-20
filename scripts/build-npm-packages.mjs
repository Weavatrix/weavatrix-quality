// Assemble the universal `wvq` package or platform-specific fallback packages.
// Node built-ins only: no install scripts and no network access.
import {
    chmodSync,
    copyFileSync,
    cpSync,
    mkdirSync,
    readFileSync,
    rmSync,
    writeFileSync,
} from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..')
const WRAPPER = join(ROOT, 'npm', 'wvq')
const DIST = join(ROOT, 'npm', 'dist')
const PLATFORMS = {
    'win32-x64': { os: 'win32', cpu: 'x64', suffix: '.exe' },
    'win32-arm64': { os: 'win32', cpu: 'arm64', suffix: '.exe' },
    'darwin-x64': { os: 'darwin', cpu: 'x64', suffix: '' },
    'darwin-arm64': { os: 'darwin', cpu: 'arm64', suffix: '' },
    'linux-x64': { os: 'linux', cpu: 'x64', suffix: '' },
    'linux-arm64': { os: 'linux', cpu: 'arm64', suffix: '' },
}
const BINARY_STEMS = ['wvq', 'wvq-mcp', 'wvq-bench']
const wrapperManifest = JSON.parse(readFileSync(join(WRAPPER, 'package.json'), 'utf8'))
const [, , mode, ...rest] = process.argv

if (mode === 'main') {
    const version = rest[0] || wrapperManifest.version
    const target = prepareWrapper(version)
    const manifest = readManifest(target)
    manifest.optionalDependencies = Object.fromEntries(
        Object.keys(PLATFORMS).map((platform) => [`@weavatrix/wvq-${platform}`, version]),
    )
    writeManifest(target, manifest)
    console.log(`assembled ${target} @ ${version}`)
} else if (mode === 'current') {
    const [platform, binaryRoot, versionArg] = rest
    if (!PLATFORMS[platform] || !binaryRoot) usage()
    const target = prepareWrapper(versionArg || wrapperManifest.version)
    copyPlatformBinaries(platform, binaryRoot, join(target, 'bin', 'native', platform))
    console.log(`assembled current-platform ${target}`)
} else if (mode === 'universal') {
    const [artifactsRoot, versionArg] = rest
    if (!artifactsRoot) usage()
    const target = prepareWrapper(versionArg || wrapperManifest.version)
    for (const platform of Object.keys(PLATFORMS)) {
        copyPlatformBinaries(
            platform,
            join(artifactsRoot, platform),
            join(target, 'bin', 'native', platform),
        )
    }
    console.log(`assembled universal ${target}`)
} else if (PLATFORMS[mode]) {
    const [binaryRoot, versionArg] = rest
    if (!binaryRoot) usage()
    const version = versionArg || wrapperManifest.version
    const { os, cpu, suffix } = PLATFORMS[mode]
    const name = `@weavatrix/wvq-${mode}`
    const target = join(DIST, `wvq-${mode}`)
    rmSync(target, { recursive: true, force: true })
    mkdirSync(target, { recursive: true })
    copyPlatformBinaries(mode, binaryRoot, target)
    copyFileSync(join(ROOT, 'LICENSE'), join(target, 'LICENSE'))
    writeFileSync(join(target, 'package.json'), `${JSON.stringify({
        name,
        version,
        description: `Weavatrix Quality native binaries for ${os} ${cpu}.`,
        license: 'MIT',
        repository: wrapperManifest.repository,
        homepage: wrapperManifest.homepage,
        os: [os],
        cpu: [cpu],
        files: [...BINARY_STEMS.map((stem) => `${stem}${suffix}`), 'LICENSE'],
        preferUnplugged: true,
    }, null, 2)}\n`)
    console.log(`assembled ${target} @ ${version}`)
} else {
    usage()
}

function prepareWrapper(version) {
    const target = join(DIST, 'wvq')
    rmSync(target, { recursive: true, force: true })
    cpSync(WRAPPER, target, { recursive: true })
    const manifest = readManifest(target)
    manifest.version = version
    delete manifest.optionalDependencies
    writeManifest(target, manifest)
    const server = JSON.parse(readFileSync(join(ROOT, 'server.json'), 'utf8'))
    server.version = version
    server.packages[0].version = version
    writeFileSync(join(target, 'server.json'), `${JSON.stringify(server, null, 2)}\n`)
    return target
}

function copyPlatformBinaries(platform, sourceRoot, destinationRoot) {
    const entry = PLATFORMS[platform]
    mkdirSync(destinationRoot, { recursive: true })
    for (const stem of BINARY_STEMS) {
        const filename = `${stem}${entry.suffix}`
        const destination = join(destinationRoot, filename)
        copyFileSync(join(sourceRoot, filename), destination)
        if (entry.os !== 'win32') chmodSync(destination, 0o755)
    }
}

function readManifest(root) {
    return JSON.parse(readFileSync(join(root, 'package.json'), 'utf8'))
}

function writeManifest(root, manifest) {
    writeFileSync(join(root, 'package.json'), `${JSON.stringify(manifest, null, 2)}\n`)
}

function usage() {
    console.error('usage: node scripts/build-npm-packages.mjs main [version]')
    console.error('   or: node scripts/build-npm-packages.mjs current <platform-key> <binary-dir> [version]')
    console.error('   or: node scripts/build-npm-packages.mjs universal <artifacts-root> [version]')
    console.error('   or: node scripts/build-npm-packages.mjs <platform-key> <binary-dir> [version]')
    process.exit(2)
}
