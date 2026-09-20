// Unlinger native npm `_cacache` maintenance driver.
//
// This file delegates all cache maintenance to the producer's own bundled
// `cacache` library. It does not implement, copy, or approximate the
// mark-and-sweep algorithm. It only:
//   1. aborts immediately if the controlling parent closes stdin (daemon crash
//      or cancel), so no indefinite GC survives a dead parent;
//   2. invokes the exact native `cacache.verify` public API;
//   3. prints a single bounded JSON object of native counters on stdout.
//
// Contract (see crates/unlinger-daemon/src/tool_cache.rs):
//   argv[1] = absolute path to the bundled cacache package directory
//   argv[2] = absolute path to the exact `_cacache` root to verify
//   stdout  = one JSON object, <= a few hundred bytes
//   stderr  = diagnostics only; never success data
//   exit 0  = native verify completed (counters are authoritative)
//   exit 7  = native verify threw or the parent disappeared

'use strict'

const cachePackage = process.argv[1]
const cacheRoot = process.argv[2]

if (typeof cachePackage !== 'string' || typeof cacheRoot !== 'string') {
  process.stderr.write('usage: tool_cache_driver.js <cacache-package-dir> <cache-root>\n')
  process.exit(7)
}

// Abort on stdin EOF. The parent holds the write end for the whole child
// lifetime, so an unexpected EOF means the parent is gone and this GC must not
// keep deleting.
let stdinClosed = false
process.stdin.on('end', () => {
  stdinClosed = true
  process.stderr.write('stdin closed; aborting cache maintenance\n')
  process.exit(7)
})
process.stdin.on('error', () => {
  stdinClosed = true
  process.exit(7)
})
process.stdin.resume()

const emit = (obj) => {
  if (stdinClosed) return
  process.stdout.write(JSON.stringify(obj), () => process.exit(0))
}

const main = async () => {
  let cacache
  try {
    // Resolve the exact bundled package by absolute path. No PATH, no
    // NODE_PATH, no user config is consulted for module resolution here.
    cacache = require(cachePackage)
  } catch (error) {
    process.stderr.write(`cannot load bundled cacache: ${String(error && error.message)}\n`)
    process.exit(7)
  }

  if (typeof cacache.verify !== 'function') {
    process.stderr.write('bundled cacache has no native verify function\n')
    process.exit(7)
  }

  let stats
  try {
    stats = await cacache.verify(cacheRoot)
  } catch (error) {
    process.stderr.write(`native verify failed: ${String(error && error.message)}\n`)
    process.exit(7)
  }

  if (stdinClosed) {
    process.exit(7)
  }

  // Report only the producer's own counters. `reclaimedCount` and
  // `reclaimedSize` are the native GC numbers; `badContentCount` counts
  // integrity failures that were also reclaimed.
  for (const key of ['reclaimedCount', 'reclaimedSize', 'badContentCount']) {
    if (!Number.isSafeInteger(stats[key]) || stats[key] < 0) process.exit(7)
  }
  emit({
    reclaimedCount: stats.reclaimedCount,
    reclaimedSize: stats.reclaimedSize,
    badContentCount: stats.badContentCount,
  })
}

main().catch((error) => {
  process.stderr.write(`driver failure: ${String(error && error.message)}\n`)
  process.exit(7)
})
