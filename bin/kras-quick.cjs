#!/usr/bin/env node
'use strict';

const { spawnSync } = require('node:child_process');
const { join } = require('node:path');
const fs = require('node:fs');
const { homedir } = require('node:os');
const { createHash, randomUUID } = require('node:crypto');

function installExecutable(source, directory) {
  fs.mkdirSync(directory, { recursive: true });
  const destination = join(directory, 'kras-quick.exe');
  const hash = file => createHash('sha256').update(fs.readFileSync(file)).digest('hex');
  if (fs.existsSync(destination) && hash(source) === hash(destination)) return destination;
  const temporary = join(directory, `.kras-quick-${randomUUID()}.tmp`);
  try {
    fs.copyFileSync(source, temporary, fs.constants.COPYFILE_EXCL);
    fs.renameSync(temporary, destination);
  } finally {
    if (fs.existsSync(temporary)) fs.unlinkSync(temporary);
  }
  return destination;
}

function main() {
  if (process.platform !== 'win32' || process.arch !== 'x64') {
    console.error('KRAS Quick는 Windows x64에서 실행해야 합니다.');
    process.exitCode = 1;
    return;
  }
  try {
    const downloads = join(process.env.USERPROFILE || homedir(), 'Downloads');
    const executable = installExecutable(join(__dirname, 'kras-quick.exe'), downloads);
    fs.mkdirSync(join(downloads, 'KRAS'), { recursive: true });
    const result = spawnSync(executable, process.argv.slice(2), {
      cwd: downloads,
      stdio: 'inherit',
      shell: false,
      env: { ...process.env, KRAS_QUICK_ROOT: downloads },
    });
    if (result.error) throw result.error;
    process.exitCode = result.status ?? 1;
  } catch (error) {
    console.error(`KRAS Quick 설치·실행 실패: ${error.message}. 실행 중인 KRAS Quick이 있다면 종료해 주세요.`);
    process.exitCode = 1;
  }
}

module.exports = { installExecutable };
if (require.main === module) main();
