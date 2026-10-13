const fs = require('fs');
const path = require('path');

const [criterionDir, base, pr, baseLabel = base, prLabel = pr] = process.argv.slice(2);

function findBenchmarks(dir) {
     const found = [];
     for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
          if (!entry.isDirectory()) continue;
          const child = path.join(dir, entry.name);
          if (fs.existsSync(path.join(child, base, 'estimates.json'))) found.push(child);
          else found.push(...findBenchmarks(child));
     }
     return found;
}

function read(dir, baseline) {
     const file = path.join(dir, baseline, 'estimates.json');
     if (!fs.existsSync(file)) return null;
     return JSON.parse(fs.readFileSync(file, 'utf8')).mean.point_estimate;
}

function formatTime(ns) {
     const units = [['s', 1e9], ['ms', 1e6], ['µs', 1e3]];
     for (const [unit, scale] of units) {
          if (ns >= scale) return `${(ns / scale).toFixed(2)} ${unit}`;
     }
     return `${ns.toFixed(2)} ns`;
}

const rows = findBenchmarks(criterionDir)
     .map((dir) => {
          const id = JSON.parse(fs.readFileSync(path.join(dir, base, 'benchmark.json'), 'utf8')).full_id;
          return { id, before: read(dir, base), after: read(dir, pr) };
     })
     .filter((row) => row.before !== null && row.after !== null)
     .sort((a, b) => a.id.localeCompare(b.id));

console.log(`| Benchmark | \`${baseLabel}\` | ${prLabel} | Change |`);
console.log('|---|---:|---:|---:|');
for (const { id, before, after } of rows) {
     const change = ((after - before) / before) * 100;
     const sign = change > 0 ? '+' : '';
     console.log(`| \`${id}\` | ${formatTime(before)} | ${formatTime(after)} | ${sign}${change.toFixed(1)}% |`);
}
