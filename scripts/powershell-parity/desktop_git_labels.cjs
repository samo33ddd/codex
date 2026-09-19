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
const currentFiles = {
  active: 'active-tool-activity-label-d0842236ddf3.js',
  row: 'conversation-blocks-801b796bbe8f.js',
  grouping: 'agent-activity-item-998595b44e9a.js',
};
const baselineSha256 = {
  [files.active]: '113932892d6616d535a9278fe7d9922adacc37fa163386b5c5bf37851d3e3d85',
  [files.row]: 'd6384cf380c36a77bb51c35b9c9c48be0ed9f51e1caaf3d08079a5736c94f282',
  [files.grouping]: '1a401d7bdc6655df97a6d1e6768bad9b5aff26cd3cc9f2e8d7108a59bfe82ffd',
  [currentFiles.active]: '8933fbad598f2514178164c6605bc7dfd131e5653bfc3e9b898d4303d7798cc1',
  [currentFiles.row]: '392f0683059d50069ef44c8e9785eb7a35e73c23152a60d569c2b010c6be53ff',
  [currentFiles.grouping]: '116193979a86470b9f539d1b643b4c7d2899ab81aef8413dccbbc19862d41fec',
};

function supportedAssets(names) {
  return [files, currentFiles].map(Object.values).find(group =>
    group.every(name => names.includes(name))) || [];
}

// A bounded PowerShell literal subset: quotes and ;/newlines are understood,
// while expansion, pipes, redirection and control flow keep the raw fallback.
function gitStages(command) {
  if (typeof command !== 'string' || command.length > 900) return null;
  const stages = [], words = [];
  let value = '', quote = '', quoted = false, started = false, trailingSemicolon = false;
  const word = () => {
    if (!started || value.length > 240) return !started;
    words.push({value, quoted});
    value = ''; quoted = false; started = false;
    return words.length <= 20;
  };
  const stage = () => {
    if (!word() || words.length === 0) return false;
    stages.push(words.splice(0));
    return stages.length <= 8;
  };
  for (let i = 0; i < command.length; i++) {
    const ch = command[i];
    if (quote) {
      if (ch === quote) {
        if (command[i + 1] === quote) { value += ch; i++; }
        else quote = '';
      } else {
        if (quote === '"' && (ch === '$' || ch === '`')) return null;
        value += ch;
      }
      continue;
    }
    if (ch === '"' || ch === "'") { quote = ch; quoted = true; started = true; continue; }
    if (ch === ';' || ch === '\r' || ch === '\n') {
      if (ch === ';' || started || words.length) {
        if (!stage()) return null;
      }
      trailingSemicolon = ch === ';';
      if (ch === '\r' && command[i + 1] === '\n') i++;
      continue;
    }
    if (/\s/.test(ch)) { if (!word()) return null; continue; }
    if (/[&|><`$#(){}]/.test(ch)) return null;
    value += ch; started = true; trailingSemicolon = false;
  }
  if (quote || trailingSemicolon) return null;
  if ((started || words.length) && !stage()) return null;
  return stages.length ? stages : null;
}

function gitStage(words) {
  if (words[0].quoted || !/^(?:git|git\.exe)$/i.test(words[0].value)) return null;
  const args = words.slice(1).map(word => word.value);
  let prefix = 0, usedCwd = false, usedConfig = false;
  while (args[prefix] === '-c' || args[prefix] === '-C') {
    const option = args[prefix++], value = args[prefix++];
    if (option === '-c') {
      if (usedConfig || !['core.safecrlf=false', 'core.quotepath=false', 'color.ui=false'].includes(value)) return null;
      usedConfig = true;
    } else {
      if (usedCwd || !value || !/^[\w.\/:\\-]+$/.test(value) || value.startsWith('-')) return null;
      usedCwd = true;
    }
  }
  const operation = args[prefix++], tail = args.slice(prefix);
  const allowed = {
    status: ['--short', '-s', '--branch', '-b', '--porcelain', '--porcelain=v1', '--porcelain=v2', '--no-color'],
    diff: ['--check', '--stat', '--name-only', '--name-status', '--cached', '--staged', '--no-color'],
    branch: ['--show-current', '--list', '-a', '-r', '--all', '--remotes', '--no-color'],
    log: ['--oneline', '--stat', '--no-color', '--decorate', '--no-decorate'],
    show: ['--stat', '--no-color', '--oneline', '--name-only', '--name-status'],
    remote: ['-v'],
    'rev-parse': ['--abbrev-ref', '--symbolic-full-name', '--show-toplevel', '--is-inside-work-tree'],
    'ls-remote': ['--heads', '--tags', '--refs', '--symref'],
    'ls-files': [],
  }[operation];
  if (!allowed) return null;
  let positional = 0;
  for (let i = 0; i < tail.length; i++) {
    const arg = tail[i];
    if (allowed.includes(arg)) continue;
    if (operation === 'log' && (/^-[1-9]\d{0,3}$/.test(arg) || /^--max-count=[1-9]\d{0,3}$/.test(arg))) continue;
    if (operation === 'log' && arg === '-n' && /^[1-9]\d{0,3}$/.test(tail[++i] || '')) continue;
    if ((operation === 'log' || operation === 'show') && positional++ === 0 && /^(?:HEAD|[0-9a-f]{7,40})$/.test(arg)) continue;
    if (operation === 'remote' && tail.length === 2 && tail[0] === 'show' && i === 0) continue;
    if (operation === 'remote' && tail.length === 2 && tail[0] === 'show' && i === 1 && /^[\w.-]+$/.test(arg)) continue;
    if (operation === 'rev-parse' && positional++ <= 1 && (arg === '@{u}' || arg === 'HEAD')) continue;
    if (operation === 'ls-remote' && positional++ < 3 && /^[\w./:-]+$/.test(arg)) continue;
    if (operation === 'ls-files' && positional++ === 0 && /^[\w./\\:-]+$/.test(arg)) continue;
    return null;
  }
  return operation;
}

function otherReadStage(words) {
  const head = words[0].value.toLowerCase(), args = words.slice(1).map(word => word.value);
  if (words[0].quoted) return false;
  if (['get-content', 'gc', 'cat', 'type'].includes(head)) {
    if (args.length === 1) return Boolean(args[0]);
    return args.length === 2 && args[0].toLowerCase() === '-literalpath' && Boolean(args[1]);
  }
  if (head === 'rg' || head === 'rg.exe') {
    return args.length >= 1 && args.length <= 4 &&
      args.every(arg => !arg.startsWith('-') || ['-n', '-i', '--files'].includes(arg));
  }
  return false;
}

function gitOperation(command) {
  const stages = gitStages(command);
  if (!stages) return null;
  const operations = stages.map(gitStage);
  const gitCount = operations.filter(Boolean).length;
  if (!gitCount || stages.some((words, index) => !operations[index] && !otherReadStage(words))) return null;
  if (gitCount !== stages.length) return 'git_and_other';
  return stages.length > 1 ? 'git_commands' : operations[0];
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
    branch: finished ? 'Проверена ветка Git' : 'Проверяется ветка Git',
    remote: finished ? 'Проверены удалённые репозитории Git' : 'Проверяются удалённые репозитории Git',
    'rev-parse': finished ? 'Проверена ссылка Git' : 'Проверяется ссылка Git',
    'ls-remote': finished ? 'Проверены удалённые ветки Git' : 'Проверяются удалённые ветки Git',
    'ls-files': finished ? 'Просмотрены файлы Git' : 'Просматриваются файлы Git',
    git_commands: finished ? 'Выполнены команды Git' : 'Выполняются команды Git',
    git_and_other: finished ? 'Выполнены Git и другие команды' : 'Выполняются Git и другие команды',
  })[operation] || null;
  if (operation === 'git_commands') return finished ? 'Ran Git commands' : 'Running Git commands';
  if (operation === 'git_and_other') return finished ? 'Ran Git and other commands' : 'Running Git and other commands';
  const label = {status: 'Git status', diff: 'Git diff',
    log: 'Git history', show: 'Git object', branch: 'Git branch', remote: 'Git remotes',
    'rev-parse': 'Git reference', 'ls-remote': 'remote Git branches', 'ls-files': 'Git files'}[operation];
  return label ? `${finished ? 'Inspected' : 'Inspecting'} ${label}` : null;
}

// Each chunk gets a private copy; no new import path or ASAR entry is needed.
const helper = `${gitStages.toString()}\n${gitStage.toString()}\n${otherReadStage.toString()}\n${gitOperation.toString()}\n${gitLocale.toString()}\n${gitLabel.toString()}\n`;

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
  if (name === files.active || name === currentFiles.active) {
    const functionName = name === files.active ? 'P' : 'N';
    const marker = `function ${functionName}(e){let t;t=e.executionStatus===\`interrupted\`?`;
    const replacement = `function ${functionName}(e){` + 'let g=e.parsedCmd.type===`unknown`&&(!e.parsedCmd.isFinished||e.executionStatus===`completed`)&&(e.output?.exitCode==null||e.output.exitCode===0)?gitOperation(e.parsedCmd.cmd):null;if(g){let s=gitLabel(g,e.parsedCmd.isFinished);return{activityKey:e.callId,icon:`run-command`,message:{id:`desktopGitLabel.${g}.${e.parsedCmd.isFinished?`done`:`active`}`,defaultMessage:s,description:`Git command presentation`}}}let t;t=e.executionStatus===`interrupted`?';
    return helper + replaceOne(source, marker, replacement, name);
  }
  if (name === files.row || name === currentFiles.row) {
    const marker = 'switch(n.type){case`format`:case`test`:case`lint`:case`noop`:case`unknown`:{if(a){';
    const replacement = 'let gitRowOperation=n.type===`unknown`&&!c&&!l&&!o&&(!a?p===`completed`:p===`inProgress`)?gitOperation(n.cmd):null;switch(n.type){case`format`:case`test`:case`lint`:case`noop`:case`unknown`:{if(gitRowOperation)return(0,X.jsx)(`span`,{className:f,children:gitLabel(gitRowOperation,!a)});if(a){';
    return helper + replaceOne(source, marker, replacement, name);
  }
  if (name === files.grouping || name === currentFiles.grouping) {
    // A recognized Git command becomes an individual activity row, so a
    // collapsed generic "N commands" group cannot hide its specific label.
    const [convert, standalone] = name === files.grouping ? ['gn', 'a'] : ['vn', 'Ve'];
    const marker = 'case`exec`:case`patch`:return $(' + convert + '(e),' + standalone + '(e)?`standalone`:`groupable`);';
    const replacement = 'case`exec`:case`patch`:return $(' + convert + '(e),e.type===`exec`&&e.parsedCmd.type===`unknown`&&gitOperation(e.parsedCmd.cmd)?`standalone`:' + standalone + '(e)?`standalone`:`groupable`);';
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
    const names = supportedAssets(Object.keys(input));
    if (!names.length) throw new Error('Missing desktop chunks for a supported baseline');
    for (const name of names) {
      if (typeof input[name] !== 'string') throw new Error(`Missing desktop chunk: ${name}`);
      output[name] = patchSource(name, input[name]);
    }
    return output;
  }
  if (typeof outputDir !== 'string' || path.resolve(input) === path.resolve(outputDir)) {
    throw new Error('Separate input and output directories are required');
  }
  const names = supportedAssets(fs.readdirSync(input));
  const original = Object.fromEntries(names.map(name =>
    [name, fs.readFileSync(path.join(input, name), 'utf8')]));
  const patched = patchAssets(original);
  const manifest = {files: names.map(name => ({
    name, beforeSha256: sha256(original[name]), afterSha256: sha256(patched[name]),
  }))};
  fs.mkdirSync(outputDir, {recursive: true});
  for (const name of names) fs.writeFileSync(path.join(outputDir, name), patched[name]);
  fs.writeFileSync(path.join(outputDir, 'desktop_git_labels_manifest.json'),
    JSON.stringify(manifest, null, 2) + '\n');
  return manifest;
}

module.exports = {patchAssets, patchSource, gitOperation, gitLabel, files, currentFiles, supportedAssets};
if (require.main === module) {
  const [input, output] = process.argv.slice(2);
  if (input === '--list-assets') {
    const names = output === '--installed' ? supportedAssets(JSON.parse(fs.readFileSync(0, 'utf8'))) : Object.values(files);
    console.log(JSON.stringify(names));
  } else if (!input || !output) {
    console.error('Usage: node desktop_git_labels.cjs input-dir output-dir');
    process.exitCode = 2;
  } else {
    console.log(JSON.stringify(patchAssets(input, output), null, 2));
  }
}
