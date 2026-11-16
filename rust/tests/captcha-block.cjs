const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const source = fs.readFileSync(path.join(__dirname, '../src/stage12.rs'), 'utf8');
const script = source.match(/const FILL_CAPTCHA_JS: &str = r##"([\s\S]*?)"##;/)[1];

(async () => {
  for (const text of ['IP가 차단되었습니다.', '접근이 제한되었습니다.', '비정상적인 접근입니다.']) {
    const select = vm.runInNewContext(`(${script})`, {
      location: { href: 'https://www.kras.go.kr/kras/cert/certView.do' }, URL,
      document: {
        querySelector: () => ({ textContent: text }),
        querySelectorAll: () => { throw new Error('blocked page must not submit or inspect captcha'); },
      },
      setTimeout: () => { throw new Error('blocked page must not retry'); },
    });
    assert.equal(await select('01234'), 'access_blocked');
  }
  console.log('PASS: 3 access-block notices cause zero captcha submissions and zero retries');
})().catch(error => { console.error(error); process.exitCode = 1; });
