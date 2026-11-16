#!/usr/bin/env node
'use strict';

const { spawnSync } = require('node:child_process');
const { join } = require('node:path');

if (process.platform !== 'win32' || process.arch !== 'x64') {
  console.error('KRAS Quick는 Windows x64에서 실행해야 합니다.');
  process.exit(1);
}

const result = spawnSync(join(__dirname, 'kras-quick.exe'), process.argv.slice(2), {
  stdio: 'inherit',
  shell: false,
  env: { ...process.env, KRAS_QUICK_ROOT: process.env.KRAS_QUICK_ROOT || process.cwd() },
});
if (result.error) {
  console.error(`KRAS Quick 실행 실패: ${result.error.message}`);
  process.exit(1);
}
process.exit(result.status ?? 1);
