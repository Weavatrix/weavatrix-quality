#!/usr/bin/env node
import { runNative, selectWvqInvocation } from './run-native.mjs'

const invocation = selectWvqInvocation(process.argv.slice(2))
runNative(invocation.kind, invocation.label, invocation.args)
