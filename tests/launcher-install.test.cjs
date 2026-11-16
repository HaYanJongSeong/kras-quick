const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const vm = require('node:vm');
const { installExecutable } = require('../bin/kras-quick.cjs');

function fixture(t) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'kras-launcher-test-'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const bin = path.join(root, 'cache', 'bin');
  fs.mkdirSync(bin, { recursive: true });
  const source = path.join(bin, 'kras-quick.exe');
  fs.writeFileSync(source, 'new-executable');
  return { root, bin, source, downloads: path.join(root, 'Downloads') };
}

test('installs only the executable into Downloads, not a test folder', t => {
  const f = fixture(t);
  const destination = installExecutable(f.source, f.downloads);
  assert.equal(destination, path.join(f.downloads, 'kras-quick.exe'));
  assert.equal(fs.readFileSync(destination, 'utf8'), 'new-executable');
  assert.deepEqual(fs.readdirSync(f.downloads), ['kras-quick.exe']);
});

test('does not rewrite an identical installed executable', t => {
  const f = fixture(t);
  const destination = installExecutable(f.source, f.downloads);
  fs.utimesSync(destination, new Date('2000-01-01'), new Date('2000-01-01'));
  const before = fs.statSync(destination).mtimeMs;
  installExecutable(f.source, f.downloads);
  assert.equal(fs.statSync(destination).mtimeMs, before);
});

test('updates the executable without changing parcel data', t => {
  const f = fixture(t);
  const destination = installExecutable(f.source, f.downloads);
  const data = path.join(f.downloads, 'KRAS', 'fixture', 'keep.xml');
  fs.mkdirSync(path.dirname(data), { recursive: true });
  fs.writeFileSync(data, 'preserved');
  fs.writeFileSync(f.source, 'updated-executable');
  installExecutable(f.source, f.downloads);
  assert.equal(fs.readFileSync(destination, 'utf8'), 'updated-executable');
  assert.equal(fs.readFileSync(data, 'utf8'), 'preserved');
  assert.ok(!fs.readdirSync(f.downloads).some(name => name.endsWith('.tmp')));
});

test('missing source leaves the installed executable unchanged', t => {
  const f = fixture(t);
  const destination = installExecutable(f.source, f.downloads);
  assert.throws(() => installExecutable(path.join(f.root, 'missing.exe'), f.downloads));
  assert.equal(fs.readFileSync(destination, 'utf8'), 'new-executable');
});

test('launches from Downloads and overrides stale test-root settings', t => {
  const f = fixture(t);
  const module = { exports: {} };
  let invocation;
  function load(name) {
    return name === 'node:child_process'
      ? { spawnSync: (...args) => { invocation = args; return { status: 0 }; } }
      : require(name);
  }
  load.main = module;
  const process = {
    platform: 'win32', arch: 'x64', argv: ['node', 'launcher', '--version'],
    env: { USERPROFILE: f.root, KRAS_QUICK_ROOT: 'old-test-folder' },
  };
  vm.runInNewContext(fs.readFileSync(path.join(__dirname, '../bin/kras-quick.cjs'), 'utf8'), {
    require: load, module, process, console, __dirname: f.bin,
  });
  assert.equal(process.exitCode, 0);
  assert.ok(fs.statSync(path.join(f.downloads, 'KRAS')).isDirectory());
  assert.equal(invocation[0], path.join(f.downloads, 'kras-quick.exe'));
  assert.equal(invocation[2].cwd, f.downloads);
  assert.equal(invocation[2].env.KRAS_QUICK_ROOT, f.downloads);
  assert.deepEqual(Array.from(invocation[1]), ['--version']);
});
