#!/usr/bin/env node

const { spawn } = require('child_process');
const path = require('path');
const fs = require('fs');

const platform = process.platform;
let binaryName = 'nexus-linux-x64';

if (platform === 'win32') {
  binaryName = 'nexus-win-x64.exe';
} else if (platform === 'darwin') {
  binaryName = 'nexus-darwin';
} else if (platform === 'linux') {
  binaryName = 'nexus-linux-x64';
}

const binaryPath = path.join(__dirname, '..', 'binaries', binaryName);

if (!fs.existsSync(binaryPath)) {
  console.error(`\x1b[31mError:\x1b[0m Binary not found for platform '${platform}': ${binaryPath}`);
  console.error(`Available binaries in package:`, fs.readdirSync(path.join(__dirname, '..', 'binaries')));
  process.exit(1);
}

// Spawn the native compiled binary, inheriting terminal stdio
const child = spawn(binaryPath, process.argv.slice(2), {
  stdio: 'inherit',
  windowsHide: false
});

child.on('exit', (code, signal) => {
  if (signal) {
    process.kill(process.pid, signal);
  } else {
    process.exit(code ?? 0);
  }
});

child.on('error', (err) => {
  console.error(`\x1b[31mFailed to start Nexus binary:\x1b[0m`, err);
  process.exit(1);
});
