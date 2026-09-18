const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const vm = require('node:vm');
const {spawnSync} = require('node:child_process');
const {patchAssets, patchSource, gitOperation, gitLabel, files} = require('./desktop_git_labels.cjs');

const accepted = [
  ['git status', 'status'],
  ['git.exe status --short', 'status'],
  ['git diff --stat', 'diff'],
  ['git diff --name-only', 'diff'],
  ['git log -n 5 --oneline', 'log'],
  ['git show HEAD --stat', 'show'],
];
for (const [command, operation] of accepted) assert.equal(gitOperation(command), operation, command);

const rejected = [
  'git status --short\nrg TODO src',
  'git status; Remove-Item x',
  'git diff | cat',
  'git diff --output=patch.txt',
  'git diff --ext-diff',
  'git show --format=%B HEAD',
  'git log --max-count=0',
  'git -C project status',
  'pwsh -Command git status',
  'git push',
  'git status > status.txt',
  'git status $(Remove-Item x)',
];
for (const command of rejected) assert.equal(gitOperation(command), null, command);

globalThis.document = {documentElement: {lang: 'ru-RU'}};
assert.equal(gitLabel('status'), 'Проверяется состояние Git');
assert.equal(gitLabel('diff', true), 'Просмотрены изменения Git');
globalThis.document.documentElement.lang = 'en-US';
assert.equal(gitLabel('log'), 'Inspecting Git history');
assert.equal(gitLabel('show', true), 'Inspected Git object');

const base = path.resolve(__dirname, '../../.local/desktop');
const source = Object.fromEntries(Object.values(files).map(name =>
  [name, fs.readFileSync(path.join(base, name), 'utf8')]));
const patched = patchAssets(source);
const listing = spawnSync(process.execPath, [path.join(__dirname, 'desktop_git_labels.cjs'), '--list-assets'],
  {encoding: 'utf8'});
assert.equal(listing.status, 0, listing.stderr);
assert.deepEqual(JSON.parse(listing.stdout), Object.values(files));
for (const name of Object.values(files)) {
  assert.notEqual(patched[name], source[name]);
  assert.match(patched[name], /function gitOperation\(/);
  assert.throws(() => patchSource(name, patched[name]), /baseline mismatch/);
  const check = spawnSync(process.execPath, ['--check', '--input-type=module'],
    {input: patched[name], encoding: 'utf8'});
  assert.equal(check.status, 0, `${name}: ${check.stderr}`);
}
assert.match(patched[files.active], /desktopGitLabel/);
assert.match(patched[files.row], /children:gitLabel\(gitRowOperation,!a\)/);
assert.match(patched[files.grouping], /\?`standalone`:a\(e\)/);

// Exercise the actual patched functions with small UI stubs. This catches a
// guard that accidentally hides the label when the normal row shows raw text.
function isolatedFunction(source, name, stubs) {
  const start = source.indexOf(`function ${name}(`);
  assert.ok(start >= 0);
  const end = name === 'P' ? source.indexOf('}var F,I=', start) + 1 :
    source.indexOf('function ', start + 9);
  assert.ok(end > start);
  const helpers = source.slice(0, source.indexOf('import'));
  return vm.runInNewContext(`${helpers}\n${source.slice(start, end)}\n${name}`, stubs);
}
const active = isolatedFunction(patched[files.active], 'P', {
  globalThis: {document: {documentElement: {lang: 'en'}}},
  F: {commandStopped: 'stopped', commandRan: 'ran', commandRunning: 'running',
    commandStoppedWithDetail: 'stopped detail', commandRanWithDetail: 'ran detail',
    commandRunningWithDetail: 'running detail'},
});
const item = (cmd, status, exitCode) => ({callId: 'test', parsedCmd: {
  type: 'unknown', cmd, isFinished: status !== 'inProgress'},
  executionStatus: status, output: {exitCode}});
assert.equal(active(item('git status --short', 'inProgress')).message.defaultMessage,
  'Inspecting Git status');
assert.equal(active(item('git status --short', 'completed', 0)).message.defaultMessage,
  'Inspected Git status');
assert.equal(active(item('git status --short', 'failed', 1)).message, 'ran detail');
assert.equal(active(item('git status --short\nrg TODO', 'completed', 0)).message, 'ran detail');

const row = isolatedFunction(patched[files.row], 'VC', {
  globalThis: {document: {documentElement: {lang: 'ru'}}},
  ZC: {c: size => Array(size)}, Um: () => false, HC: () => false, JC: () => null,
  X: {jsx: (type, props) => ({type, props})}, Il: (...classes) => classes.join(' '),
  q: 'message',
});
function rowProps(status, cmd = 'git diff --stat') {
  return {summary: {type: 'unknown', cmd}, cmd, isInProgress: status === 'inProgress',
    isBackgroundTerminalRunning: false, isFinishedBackgroundTerminal: false,
    wasInterrupted: false, wasDeclinedByAutoReview: false, isExpanded: false,
    showRawCommand: true, summaryStatusClassName: 'test', executionStatus: status};
}
assert.equal(row(rowProps('inProgress')).props.children, 'Просматриваются изменения Git');
const completedRow = rowProps('completed');
completedRow.isFinishedBackgroundTerminal = true; // Actual completed item retains processId.
assert.equal(row(completedRow).props.children, 'Просмотрены изменения Git');
assert.notEqual(row(rowProps('failed')).props.children, 'Просмотрены изменения Git');
assert.notEqual(row(rowProps('completed', 'git status\nrg TODO')).props.children,
  'Проверено состояние Git');

const temp = fs.mkdtempSync(path.join(os.tmpdir(), 'codex-git-labels-'));
try {
  const input = path.join(temp, 'input');
  const output = path.join(temp, 'output');
  fs.mkdirSync(input);
  for (const name of Object.values(files)) fs.writeFileSync(path.join(input, name), source[name]);
  const manifest = patchAssets(input, output);
  assert.equal(manifest.files.length, 3);
  for (const entry of manifest.files) {
    assert.match(entry.beforeSha256, /^[0-9a-f]{64}$/);
    assert.match(entry.afterSha256, /^[0-9a-f]{64}$/);
    assert.equal(fs.readFileSync(path.join(output, entry.name), 'utf8'), patched[entry.name]);
  }
  assert.deepEqual(JSON.parse(fs.readFileSync(path.join(output, 'desktop_git_labels_manifest.json'))), manifest);
} finally {
  fs.rmSync(temp, {recursive: true, force: true});
}

console.log('Desktop Git label classifier and guarded chunks: OK');
