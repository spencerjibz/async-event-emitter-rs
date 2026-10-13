const fs = require('fs');
const path = require('path');

const MARKER = '<!-- benchmark-comparison -->';
const ID_PATTERN = /^[A-Za-z0-9_+./-]{1,120}$/;
const MAX_RESULTS = 200;

function findBenchmarks(dir, baseline) {
     const found = [];
     for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
          if (!entry.isDirectory()) continue;
          const child = path.join(dir, entry.name);
          if (fs.existsSync(path.join(child, baseline, 'estimates.json'))) found.push(child);
          else found.push(...findBenchmarks(child, baseline));
     }
     return found;
}

function estimate(dir, baseline) {
     const file = path.join(dir, baseline, 'estimates.json');
     if (!fs.existsSync(file)) return null;
     return JSON.parse(fs.readFileSync(file, 'utf8')).mean.point_estimate;
}

function collect(criterionDir, base, pr, baseUsesPrBenches) {
     const dirs = fs.existsSync(criterionDir) ? findBenchmarks(criterionDir, base) : [];
     const results = dirs
          .map((dir) => ({
               id: JSON.parse(fs.readFileSync(path.join(dir, base, 'benchmark.json'), 'utf8')).full_id,
               base: estimate(dir, base),
               pr: estimate(dir, pr),
          }))
          .filter((row) => row.base !== null && row.pr !== null)
          .sort((a, b) => a.id.localeCompare(b.id));
     return { baseUsesPrBenches, results };
}

function hasExactKeys(value, keys) {
     return (
          typeof value === 'object' &&
          value !== null &&
          !Array.isArray(value) &&
          Object.keys(value).sort().join() === keys
     );
}

function validate(data) {
     if (!hasExactKeys(data, 'baseUsesPrBenches,results') || typeof data.baseUsesPrBenches !== 'boolean') {
          throw new Error('invalid benchmark results');
     }
     if (!Array.isArray(data.results) || data.results.length > MAX_RESULTS) {
          throw new Error('invalid benchmark results');
     }
     for (const row of data.results) {
          const valid =
               hasExactKeys(row, 'base,id,pr') &&
               typeof row.id === 'string' &&
               ID_PATTERN.test(row.id) &&
               [row.base, row.pr].every((v) => typeof v === 'number' && Number.isFinite(v) && v > 0);
          if (!valid) throw new Error('invalid benchmark result');
     }
     return data;
}

function formatTime(ns) {
     for (const [unit, scale] of [['s', 1e9], ['ms', 1e6], ['µs', 1e3]]) {
          if (ns >= scale) return `${(ns / scale).toFixed(2)} ${unit}`;
     }
     return `${ns.toFixed(2)} ns`;
}

function render(data, baseRef) {
     const ref = String(baseRef).replace(/[^A-Za-z0-9._/-]/g, '');
     const lines = [MARKER, `### Benchmarks against \`${ref}\``, ''];
     if (data.results.length === 0) {
          lines.push(`Benchmarks could not run on \`${ref}\`, so there is nothing to compare against.`);
          return `${lines.join('\n')}\n`;
     }
     if (data.baseUsesPrBenches) {
          lines.push(`\`${ref}\` has no benchmarks, so it was measured with this PR's.`, '');
     }
     lines.push(`| Benchmark | \`${ref}\` | this PR | Change |`, '|---|---:|---:|---:|');
     for (const { id, base, pr } of data.results) {
          const change = ((pr - base) / base) * 100;
          const sign = change > 0 ? '+' : '';
          lines.push(`| \`${id}\` | ${formatTime(base)} | ${formatTime(pr)} | ${sign}${change.toFixed(1)}% |`);
     }
     return `${lines.join('\n')}\n`;
}

module.exports = { collect, validate, render };

if (require.main === module) {
     const [command, ...args] = process.argv.slice(2);
     if (command === 'collect') {
          const [criterionDir, base, pr] = args;
          const data = collect(criterionDir, base, pr, process.env.BASE_USES_PR_BENCHES === 'true');
          process.stdout.write(`${JSON.stringify(data, null, 2)}\n`);
     } else if (command === 'render') {
          const [file, baseRef] = args;
          process.stdout.write(render(validate(JSON.parse(fs.readFileSync(file, 'utf8'))), baseRef));
     } else {
          console.error('usage: benchmark-table.js collect <criterion-dir> <base> <pr> | render <results.json> <base-ref>');
          process.exit(1);
     }
}
