// Browser chips retain each movement's reason. §FS-rhei-viz.4
const {test} = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const path = require('node:path');

test('operator inspector preserves repeated source reasons and selection', () => {
  const source = fs.readFileSync(path.join(__dirname, '../../crates/rhei-cli/assets/flow.html'), 'utf8');
  const functions = source.slice(source.indexOf('function previousMovements('), source.indexOf('// ANSI 16-color'));
  let selected;
  const context = vm.createContext({
    el: (_tag, attrs) => ({text: attrs.text || '', children: [],
      appendChild(child) { this.children.push(child); },
      addEventListener(_event, handler) { this.click = handler; }}),
    dot: () => ({text: ''}), stateColor: state => state,
    document: {createTextNode: text => ({text})},
    highlightState: state => { selected = state; },
  });
  vm.runInContext(functions, context);
  const history = [
    {from:'gate', to:'work', forced_reason:'first correction'},
    {from:'work', to:'gate', forced_reason:'return to gate'},
    {from:'gate', to:'done'},
  ];
  const chips = Array.from(context.previousMovements({history}), hop => context.historyChip(hop));
  assert.deepEqual(chips.map(chip => chip.children.map(child => child.text).join('')),
    ['gate', 'work — forced: return to gate', 'gate — forced: first correction']);
  chips.forEach((chip, index) => { chip.click(); assert.equal(selected, ['gate', 'work', 'gate'][index]); });
  assert.equal(history[0].forced_reason, 'first correction');
});
