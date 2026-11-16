const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

const source = fs.readFileSync(path.join(__dirname, '../src/stage12.rs'), 'utf8');
const script = source.match(/const SELECT_BUILDING_JS: &str = r##"([\s\S]*?)"##;/)[1];

async function select(current, target) {
  const events = [];
  const building = {
    value: current,
    options: [{ value: 'a', textContent: '(일반) A동' }, { value: '', textContent: '토지' }],
    dispatchEvent(event) { events.push(event.type); },
  };
  const context = {
    document: { querySelector(selector) { return selector === '#bldgSeCd' ? building : { options: [] }; } },
    Event: class { constructor(type) { this.type = type; } },
    MouseEvent: class { constructor(type) { this.type = type; } },
    setTimeout() { throw new Error('ordinary selection must not enter the floor wait loop'); },
  };
  const result = await vm.runInNewContext(`(${script})`, context)(target);
  assert.equal(result.length, 0);
  return events;
}

(async () => {
  assert.deepEqual(await select('a', 'a'), []);
  assert.deepEqual(await select('', 'a'), ['click', 'input', 'change']);
  assert.deepEqual(await select('a', ''), ['click', 'input', 'change']);
  await assert.rejects(() => select('a', 'missing'), /Stage 1/);
  console.log('PASS: same ordinary selection 0 events; changed selection 3 events; land reset retained; missing option rejected; no floor wait');
})().catch(error => { console.error(error); process.exitCode = 1; });
