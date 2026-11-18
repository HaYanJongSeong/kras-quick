const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const source = fs.readFileSync(path.join(__dirname, '../src/lookup_evaluation.js'), 'utf8');
const section = source.slice(source.indexOf('let timeoutId', source.indexOf('let emptyRequestInfoCalled')), source.indexOf('const readBuildingOptions'));

(async () => {
  for (const text of ['오류가 발생하였습니다. 잠시 후 다시 시도하여 주시기 바랍니다.', 'IP가 차단되었습니다.', '로그인이 만료되었습니다.']) {
    let clicks = 0;
    let cleared = 0;
    class KrasEvaluationError extends Error {}
    const context = {
      KrasEvaluationError,
      completion: new Promise(() => {}),
      certView: {}, originalCheckLedger() {}, originalEmptyRequest() {},
      button: { click() { clicks++; } },
      document: { querySelector() { return { textContent: text }; } },
      window: {
        setTimeout() { return 1; }, clearTimeout() { cleared++; },
        setInterval(callback) { Promise.resolve().then(callback); return 2; },
        clearInterval() { cleared++; },
      },
    };
    await assert.rejects(vm.runInNewContext(`(async()=>{${section}})()`, context), KrasEvaluationError);
    assert.equal(clicks, 1);
    assert.equal(cleared, 2);
  }
  console.log('PASS: error/block/login notices end lookup without waiting for the 300-second timeout; timers cleaned; one lookup click');
})().catch(error => { console.error(error); process.exitCode = 1; });
