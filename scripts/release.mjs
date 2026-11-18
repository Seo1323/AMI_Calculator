#!/usr/bin/env node
// Usage: npm run release 0.2.0
// Bumps the version in package.json and Cargo.toml, commits, tags vX.Y.Z and pushes.
// The tag triggers .github/workflows/release.yml, which builds all installers.
import { readFileSync, writeFileSync } from 'node:fs';
import { execFileSync } from 'node:child_process';

const version = process.argv[2];
if (!/^\d+\.\d+\.\d+$/.test(version ?? '')) {
	console.error('Usage: npm run release <major.minor.patch>, e.g. npm run release 0.2.0');
	process.exit(1);
}
const git = (...args) => execFileSync('git', args, { stdio: 'inherit' });
if (execFileSync('git', ['status', '--porcelain']).toString().trim()) {
	console.error('Working tree is not clean, commit your changes first.');
	process.exit(1);
}

const pkg = JSON.parse(readFileSync('package.json', 'utf8'));
pkg.version = version;
writeFileSync('package.json', JSON.stringify(pkg, null, 2) + '\n');

const cargo = readFileSync('Cargo.toml', 'utf8');
const bumped = cargo.replace(/(\[workspace\.package\][^[]*?\nversion = ")[^"]+(")/, `$1${version}$2`);
if (bumped === cargo) throw new Error('workspace.package version not found in Cargo.toml');
writeFileSync('Cargo.toml', bumped);

execFileSync('npm', ['install', '--package-lock-only', '--silent'], { stdio: 'inherit' });
try {
	execFileSync('cargo', ['update', '--workspace', '--quiet'], { stdio: 'inherit' });
} catch {
	/* cargo not installed: CI regenerates Cargo.lock */
}

git('add', '-A');
git('commit', '-m', `Release v${version}`);
git('tag', '-a', `v${version}`, '-m', `AMI_Calculator v${version}`);
git('push');
git('push', 'origin', `v${version}`);
console.log(`\nv${version} pushed. Installers will appear in a draft release on GitHub in ~20 minutes.`);
