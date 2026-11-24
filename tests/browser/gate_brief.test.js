// The dashboard's gate block: it renders the brief the host built, answers a
// briefed gate by a dry run, a confirmation, then the answer, and releases a
// plain one. §FS-rhei-viz.5.1.1 §FS-rhei-viz.5.1.2 §FS-rhei-viz.5.1.3
const {test} = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const path = require('node:path');

const source = fs.readFileSync(path.join(__dirname, '../../crates/rhei-cli/assets/flow.html'), 'utf8');
const functions = source.slice(source.indexOf('function gateBrief('), source.indexOf('// ---- bootstrap'));

function fakeElement(tag, attrs = {}) {
  return {
    tag, attrs, text: attrs.text || '', children: [], listeners: {},
    appendChild(child) { this.children.push(child); },
    addEventListener(event, handler) { this.listeners[event] = handler; },
  };
}

function textOf(node) {
  return (node.text || '') + (node.children || []).map(textOf).join('');
}

function gateContext(overrides = {}) {
  const context = vm.createContext({
    el: (tag, attrs) => fakeElement(tag, attrs),
    document: {createTextNode: text => ({text})},
    openLink: (rel, text, title) => ({text, link: rel, title, children: []}),
    IX: null, LIVE: false,
    ...overrides,
  });
  vm.runInContext(functions, context);
  return context;
}

const brief = {
  version: 1, ticket: 'inbox.2', ticket_title: 'Vet', title: 'Vet the proposal', state: 'vetting',
  visit: 1, since: null, source: 'questions', summary: 'Decide whether it ships.',
  display: {path: 'runtime/draft.md', absolute: '/ws/runtime/draft.md', readable: true, problem: null},
  attachments: [{path: 'notes.md', absolute: '/ws/notes.md', readable: false, problem: 'does not exist'}],
  questions: [
    {id: 'approve', kind: 'boolean', prompt: 'Ship it?', required: true},
    {id: 'scope', kind: 'multi', prompt: 'Which parts?', required: false,
      options: [{value: 'a'}, {value: 'b', label: 'Bee'}]},
    {id: 'note', kind: 'text', prompt: 'Anything else?', required: false},
  ],
  destinations: [{state: 'approved', description: 'ship', condition: null, final: true}],
  routes: [{to: 'approved', when: {approve: true}}, {to: 'rework', when: {}}],
  digest: 'sha256:abc', answer_path: 'runtime/gates/inbox.2/vetting.1.answer.json', problems: [],
};

test('the answer document carries the digest and leaves out what was not answered', () => {
  const context = gateContext();
  const document = context.gateAnswerDocument(brief, {approve: false, scope: [], note: ''});
  assert.deepEqual(JSON.parse(JSON.stringify(document)),
    {version: 1, request_digest: 'sha256:abc', answers: {approve: false}});
  assert.equal(context.gateAuthored(brief), true);
  assert.equal(context.gateAuthored({...brief, source: 'synthesized'}), false);
  assert.equal(context.gateAuthored(null), false);
});

test('refusals sit beside the question they name and the rest under the form', () => {
  const context = gateContext();
  const errors = [
    {kind: 'invalid', question: 'approve', message: '`approve` is required'},
    {kind: 'stale', question: null, message: 'the brief changed'},
    {kind: 'invalid', question: 'ghost', message: 'not asked'},
  ];
  const {byQuestion, general} = context.gateErrorsFor(errors, brief);
  assert.deepEqual(Object.keys(byQuestion), ['approve']);
  assert.deepEqual(Array.from(general, error => error.kind), ['stale', 'invalid']);
});

test('an answer is a dry run, a confirmed destination, then the answer', async () => {
  const posts = [];
  const replies = [
    {version: 1, ok: true, dry_run: true, ticket: 'inbox.2', from: 'vetting', to: 'approved', errors: []},
    {version: 1, ok: true, dry_run: false, ticket: 'inbox.2', from: 'vetting', to: 'approved', errors: []},
  ];
  const context = gateContext({
    fetch: (url, init) => {
      posts.push({url, body: JSON.parse(init.body)});
      return Promise.resolve({ok: true, json: () => replies.shift()});
    },
  });
  const asked = [];
  const result = await context.runGateAnswer({id: 'inbox.2'}, brief, {approve: true},
    move => { asked.push(move); return true; });
  assert.equal(result.stage, 'answered');
  assert.deepEqual(asked, ['this moves inbox.2 to approved']);
  assert.deepEqual(posts.map(post => [post.url, post.body.dry_run]), [['answer-gate', true], ['answer-gate', false]]);
  assert.equal(posts[0].body.from, 'vetting');
  assert.deepEqual(posts[1].body.answers, {version: 1, request_digest: 'sha256:abc', answers: {approve: true}});
  assert.equal(result.reply.to, 'approved');
});

test('a refused dry run asks nothing and answers nothing', async () => {
  const posts = [];
  const context = gateContext({
    fetch: (url, init) => {
      posts.push(JSON.parse(init.body));
      return Promise.resolve({ok: true, json: () => ({ok: false, errors: [
        {kind: 'invalid', question: 'approve', message: '`approve` is required'}]})});
    },
  });
  let asked = false;
  const result = await context.runGateAnswer({id: 'inbox.2'}, brief, {}, () => { asked = true; return true; });
  assert.equal(result.stage, 'refused');
  assert.equal(asked, false);
  assert.equal(posts.length, 1);
});

test('declining the destination writes nothing', async () => {
  const posts = [];
  const context = gateContext({
    fetch: (url, init) => {
      posts.push(JSON.parse(init.body));
      return Promise.resolve({ok: true, json: () => ({ok: true, to: 'rework', errors: []})});
    },
  });
  const result = await context.runGateAnswer({id: 'inbox.2'}, brief, {approve: false}, () => false);
  assert.equal(result.stage, 'cancelled');
  assert.deepEqual(posts.map(post => post.dry_run), [true]);
});

test('the brief shows its files, destinations and routes, and an unknown since', () => {
  const context = gateContext({LIVE: true, IX: {plan: {workspace: '/ws/'}}});
  const shown = context.renderGateBrief(brief);
  const text = textOf(shown);
  assert.match(text, /Vet the proposal/);
  assert.match(text, /inbox\.2 · gate vetting · visit 1 · since unknown/);
  assert.match(text, /Decide whether it ships\./);
  assert.match(text, /runtime\/draft\.md  readable/);
  assert.match(text, /notes\.md  unreadable: does not exist/);
  assert.match(text, /Leads to: approved \(final\) — ship/);
  assert.match(text, /Routes, first match wins: approved when approve = true · rework when always/);
  const display = shown.children.find(child => child.children && child.children[0] && child.children[0].text === 'Read ');
  assert.equal(display.children[1].link, 'runtime/draft.md', 'a readable file inside the workspace opens in the editor');
  const attached = shown.children.find(child => child.children && child.children[0] && child.children[0].text === 'Attached ');
  assert.equal(attached.children[1].link, undefined, 'an unreadable file is not a link');
});

test('an option reads as its label, with the value only when it differs', () => {
  const context = gateContext();
  assert.equal(context.gateOptionText({value: 'b', label: 'Bee'}), 'Bee (b)');
  assert.equal(context.gateOptionText({value: 'apply', label: 'apply'}), 'apply');
  assert.equal(context.gateOptionText({value: 'a'}), 'a');
});

test('a multi question keeps its picks in the order it offers them', () => {
  const context = gateContext({LIVE: true, IX: {plan: {capabilities: {gate_answer: true}}}});
  const values = {};
  const control = context.gateControl({id: 'inbox.2'}, brief.questions[1], values, true);
  const [first, second] = control.children.map(label => label.children[0]);
  second.checked = true; second.listeners.change();
  first.checked = true; first.listeners.change();
  assert.deepEqual(Array.from(values.scope), ['a', 'b']);
  second.checked = false; second.listeners.change();
  assert.deepEqual(Array.from(values.scope), ['a']);
});

function findAll(node, wanted, found = []) {
  if (wanted(node)) found.push(node);
  for (const child of node.children || []) findAll(child, wanted, found);
  return found;
}

const byClass = name => node => node.attrs && node.attrs.class === name;
const buttons = node => findAll(node, child => child.tag === 'button');

// The brief rhei synthesizes at a plain gate: a destination among every edge
// leaving it, the `from: "*"` one to `cancelled` included, and a reason.
const plain = {
  ...brief, ticket: 'inbox.3', title: 'Park the ruling', state: 'parked', source: 'synthesized',
  summary: null, display: null, attachments: [], routes: [],
  questions: [
    {id: 'destination', kind: 'choice', prompt: 'Where should inbox.3 go?', required: true,
      options: [{value: 'apply', label: 'apply', description: 'An agent applies the ruling'},
        {value: 'cancelled', label: 'cancelled', description: 'Dropped'}]},
    {id: 'reason', kind: 'text', prompt: 'Why?', required: true},
  ],
  destinations: [
    {state: 'apply', description: 'An agent applies the ruling', condition: null, final: false},
    {state: 'cancelled', description: 'Dropped', condition: null, final: true},
  ],
};

function liveBlock(gates, ms) {
  const context = gateContext({
    LIVE: true,
    IX: {plan: {workspace: '/ws', gates, capabilities: {gate_transition: true, gate_answer: true}}},
    isTerminal: state => state === 'cancelled',
  });
  return context.renderGateBlock({id: Object.keys(gates)[0] || 'inbox.4', state: ms.name}, ms);
}

test('a briefed gate gets the answer form and never the release', () => {
  const block = liveBlock({'inbox.2': brief}, {name: 'vetting', transitions: [{to: 'approved'}, {to: 'rework'}]});
  assert.equal(findAll(block, byClass('gform')).length, 1);
  assert.equal(findAll(block, byClass('grelease')).length, 0);
  assert.deepEqual(buttons(block).map(button => button.text), ['Answer…']);
});

test('a plain gate releases to every destination its brief shows and lists no questions', () => {
  const block = liveBlock({'inbox.3': plain}, {name: 'parked', transitions: [{to: 'apply'}]});
  assert.equal(findAll(block, byClass('gform')).length, 0);
  const release = findAll(block, byClass('grelease'));
  assert.equal(release.length, 1);
  const choices = buttons(release[0]);
  assert.deepEqual(choices.map(button => button.text), ['apply', 'cancelled (final)']);
  assert.equal(choices[1].attrs.title, 'Dropped');
  assert.ok(choices.every(button => !button.disabled), 'a live, wired page can release');
  const text = textOf(block);
  assert.doesNotMatch(text, /Where should inbox\.3 go\?|Why\?|required/, 'its questions are the controls');
  assert.match(text, /Result — needed to finish the ticket, unless it already has a result/);
});

test('without a brief the release offers every edge the machine shows', () => {
  const ms = {name: 'parked', transitions: [{to: 'apply'}, {to: 'cancelled', wildcard: true}, {to: 'apply'}]};
  const block = liveBlock({}, ms);
  assert.deepEqual(buttons(block).map(button => button.text), ['apply', 'cancelled (final)']);
});
