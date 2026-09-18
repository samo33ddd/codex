// Version-pinned presentation overlay for the extracted Codex desktop chunks.
// It changes no command execution, classification, approval, or protocol data.
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');

const files = {
  active: 'active-tool-activity-label-8eae31ca0de8.js',
  row: 'conversation-blocks-8a28edabd83b.js',
  grouping: 'agent-activity-item-3f8bbac7aaf9.js',
};
const baselineSha256 = {
  [files.active]: '113932892d6616d535a9278fe7d9922adacc37fa163386b5c5bf37851d3e3d85',
  [files.row]: 'd6384cf380c36a77bb51c35b9c9c48be0ed9f51e1caaf3d08079a5736c94f282',
  [files.grouping]: '1a401d7bdc6655df97a6d1e6768bad9b5aff26cd3cc9f2e8d7108a59bfe82ffd',
};

// Deliberately accepts only a small, literal, single-command subset. Unknown
// options, shells, pipes, newlines and redirections retain the desktop fallback.
function gitOperation(command) {
  if (typeof command !== 'string' || command.length > 400 ||
      /[\r\n;&|><`$(){}\\'"#]/.test(command)) return null;
  const words = command.trim().split(/\s+/);
  if (words.length < 2 || words.length > 12 || !/^(?:git|git\.exe)$/i.test(words[0])) return null;
  const [, operation, ...args] = words;
  if (!['status', 'diff', 'log', 'show'].includes(operation)) return null;
  const safe = {
    status: new Set(['--short', '-s', '--branch', '-b', '--porcelain', '--porcelain=v1', '--porcelain=v2', '--no-color']),
    diff: new Set(['--stat', '--name-only', '--name-status', '--cached', '--staged', '--no-color']),
    log: new Set(['--oneline', '--stat', '--no-color', '--decorate', '--no-decorate']),
    show: new Set(['--stat', '--no-color', '--oneline', '--name-only', '--name-status']),
  }[operation];
  let seenRevision = false;
  for (let i = 0; i < args.length; i++) {
    const arg = args[i];
    if (safe.has(arg)) continue;
    if (operation === 'log' && /^--max-count=[1-9]\d{0,4}$/.test(arg)) continue;
    if (operation === 'log' && arg === '-n' && /^[1-9]\d{0,4}$/.test(args[++i] || '')) continue;
    // A single simple revision is unambiguous for log/show. Do not mistake a
    // file, shell option, or Git revision expression for a safe literal here.
    if ((operation === 'log' || operation === 'show') && !seenRevision &&
        /^(?:HEAD|[0-9a-f]{7,40})$/.test(arg)) { seenRevision = true; continue; }
    return null;
  }
  return operation;
}

function gitLocale() {
  return /^(?:ru)(?:-|$)/i.test(globalThis.document?.documentElement?.lang ||
    globalThis.navigator?.language || 'en') ? 'ru' : 'en';
}

function gitLabel(operation, finished = false) {
  const ru = gitLocale() === 'ru';
  if (ru) return ({
    status: finished ? 'Проверено состояние Git' : 'Проверяется состояние Git',
    diff: finished ? 'Просмотрены изменения Git' : 'Просматриваются изменения Git',
    log: finished ? 'Просмотрена история Git' : 'Просматривается история Git',
    show: finished ? 'Просмотрен объект Git' : 'Просматривается объект Git',
  })[operation] || null;
  const label = {status: 'Git status', diff: 'Git diff',
    log: 'Git history', show: 'Git object'}[operation];
  return label ? `${finished ? 'Inspected' : 'Inspecting'} ${label}` : null;
}

// Each chunk gets a private copy; no new import path or ASAR entry is needed.
const helper = `${gitOperation.toString()}\n${gitLocale.toString()}\n${gitLabel.toString()}\n`;

function replaceOne(source, oldText, newText, name) {
  const first = source.indexOf(oldText);
  if (first < 0 || source.indexOf(oldText, first + 1) >= 0 ||
      source.includes('function gitOperation(')) {
    throw new Error(`Desktop baseline mismatch: ${name}`);
  }
  return source.slice(0, first) + newText + source.slice(first + oldText.length);
}

function patchSource(name, source) {
  if (sha256(source) !== baselineSha256[name]) {
    throw new Error(`Desktop baseline mismatch: ${name}`);
  }
  if (name === files.active) {
    const marker = 'function P(e){let t;t=e.executionStatus===`interrupted`?';
    const replacement = 'function P(e){let g=e.parsedCmd.type===`unknown`&&(!e.parsedCmd.isFinished||e.executionStatus===`completed`)&&(e.output?.exitCode==null||e.output.exitCode===0)?gitOperation(e.parsedCmd.cmd):null;if(g){let s=gitLabel(g,e.parsedCmd.isFinished);return{activityKey:e.callId,icon:`run-command`,message:{id:`desktopGitLabel.${g}.${e.parsedCmd.isFinished?`done`:`active`}`,defaultMessage:s,description:`Git command presentation`}}}let t;t=e.executionStatus===`interrupted`?';
    return helper + replaceOne(source, marker, replacement, name);
  }
  if (name === files.row) {
    const marker = 'switch(n.type){case`format`:case`test`:case`lint`:case`noop`:case`unknown`:{if(a){';
    const replacement = 'let gitRowOperation=n.type===`unknown`&&!c&&!l&&!o&&(!a?p===`completed`:p===`inProgress`)?gitOperation(n.cmd):null;switch(n.type){case`format`:case`test`:case`lint`:case`noop`:case`unknown`:{if(gitRowOperation)return(0,X.jsx)(`span`,{className:f,children:gitLabel(gitRowOperation,!a)});if(a){';
    return helper + replaceOne(source, marker, replacement, name);
  }
  if (name === files.grouping) {
    // A recognized Git command becomes an individual activity row, so a
    // collapsed generic "N commands" group cannot hide its specific label.
    const marker = 'case`exec`:case`patch`:return $(gn(e),a(e)?`standalone`:`groupable`);';
    const replacement = 'case`exec`:case`patch`:return $(gn(e),e.type===`exec`&&e.parsedCmd.type===`unknown`&&gitOperation(e.parsedCmd.cmd)?`standalone`:a(e)?`standalone`:`groupable`);';
    return helper + replaceOne(source, marker, replacement, name);
  }
  throw new Error(`Unexpected desktop chunk: ${name}`);
}

function sha256(source) {
  return crypto.createHash('sha256').update(source).digest('hex');
}

// Accept a map of chunk name -> source and return a new map, or copy patched
// chunks from input directory to output directory with a reviewable manifest.
function patchAssets(input, outputDir) {
  if (typeof input !== 'string') {
    const output = {};
    for (const name of Object.values(files)) {
      if (typeof input[name] !== 'string') throw new Error(`Missing desktop chunk: ${name}`);
      output[name] = patchSource(name, input[name]);
    }
    return output;
  }
  if (typeof outputDir !== 'string' || path.resolve(input) === path.resolve(outputDir)) {
    throw new Error('Separate input and output directories are required');
  }
  const original = Object.fromEntries(Object.values(files).map(name =>
    [name, fs.readFileSync(path.join(input, name), 'utf8')]));
  const patched = patchAssets(original);
  const manifest = {files: Object.values(files).map(name => ({
    name, beforeSha256: sha256(original[name]), afterSha256: sha256(patched[name]),
  }))};
  fs.mkdirSync(outputDir, {recursive: true});
  for (const name of Object.values(files)) fs.writeFileSync(path.join(outputDir, name), patched[name]);
  fs.writeFileSync(path.join(outputDir, 'desktop_git_labels_manifest.json'),
    JSON.stringify(manifest, null, 2) + '\n');
  return manifest;
}

module.exports = {patchAssets, patchSource, gitOperation, gitLabel, files};
if (require.main === module) {
  const [input, output] = process.argv.slice(2);
  if (input === '--list-assets' && output === undefined) {
    console.log(JSON.stringify(Object.values(files)));
  } else if (!input || !output) {
    console.error('Usage: node desktop_git_labels.cjs input-dir output-dir');
    process.exitCode = 2;
  } else {
    console.log(JSON.stringify(patchAssets(input, output), null, 2));
  }
}
