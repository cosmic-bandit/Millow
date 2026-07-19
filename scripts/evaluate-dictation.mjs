import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("..", import.meta.url));
const corpusPath = path.join(root, "benchmarks", "dictation-corpus.json");
const resultsPath = process.argv[2] ? path.resolve(process.argv[2]) : null;

if (!resultsPath) {
  console.error("Kullanım: npm run eval:dictation -- <sonuclar.jsonl>");
  process.exit(1);
}

const corpus = JSON.parse(fs.readFileSync(corpusPath, "utf8"));
const cases = new Map(corpus.cases.map((entry) => [entry.id, entry]));
const lines = fs.readFileSync(resultsPath, "utf8").split(/\r?\n/).filter(Boolean);
const results = lines.map((line, index) => {
  try {
    return JSON.parse(line);
  } catch (error) {
    throw new Error(`${resultsPath}:${index + 1} geçerli JSON değil: ${error.message}`);
  }
});

const modes = ["fast", "clean", "rewrite"];
const normalize = (value) => value
  .normalize("NFKC")
  .toLocaleLowerCase("tr-TR")
  .replace(/[^\p{L}\p{N}]+/gu, " ")
  .trim()
  .replace(/\s+/g, " ");

function editDistance(reference, hypothesis) {
  const previous = Array.from({ length: hypothesis.length + 1 }, (_, index) => index);
  for (let row = 1; row <= reference.length; row += 1) {
    const current = [row];
    for (let column = 1; column <= hypothesis.length; column += 1) {
      current[column] = Math.min(
        current[column - 1] + 1,
        previous[column] + 1,
        previous[column - 1] + (reference[row - 1] === hypothesis[column - 1] ? 0 : 1),
      );
    }
    previous.splice(0, previous.length, ...current);
  }
  return previous[hypothesis.length];
}

function percentile(values, ratio) {
  if (!values.length) return null;
  const sorted = [...values].sort((a, b) => a - b);
  return sorted[Math.min(sorted.length - 1, Math.ceil(sorted.length * ratio) - 1)];
}

function canonicalFormatting(value) {
  return value
    .normalize("NFKC")
    .trim()
    .replace(/[ \t]+/g, " ")
    .replace(/[ \t]*\n[ \t]*/g, "\n");
}

const rows = results.map((result) => {
  const testCase = cases.get(result.id);
  if (!testCase) throw new Error(`Bilinmeyen test kimliği: ${result.id}`);
  if (!modes.includes(result.mode)) throw new Error(`Bilinmeyen mod: ${result.mode}`);
  if (typeof result.text !== "string") throw new Error(`${result.id}: text alanı gerekli`);
  if (!Number.isFinite(result.latency_ms) || result.latency_ms < 0) {
    throw new Error(`${result.id}: latency_ms sıfır veya pozitif bir sayı olmalı`);
  }

  const referenceWords = normalize(testCase.expected[result.mode]).split(" ").filter(Boolean);
  const hypothesisWords = normalize(result.text).split(" ").filter(Boolean);
  const errors = editDistance(referenceWords, hypothesisWords);
  const terms = testCase.protected_terms ?? [];
  const normalizedOutput = normalize(result.text);
  const preservedTerms = terms.filter((term) => normalizedOutput.includes(normalize(term))).length;

  return {
    ...result,
    category: testCase.category,
    wer: referenceWords.length ? errors / referenceWords.length : Number(hypothesisWords.length > 0),
    terms: terms.length,
    preservedTerms,
    formatMatch: testCase.category === "formatting"
      ? canonicalFormatting(result.text) === canonicalFormatting(testCase.expected[result.mode])
      : null,
  };
});

function summarize(mode) {
  const selected = rows.filter((row) => row.mode === mode);
  const latencies = selected.map((row) => row.latency_ms);
  const termCount = selected.reduce((sum, row) => sum + row.terms, 0);
  const preserved = selected.reduce((sum, row) => sum + row.preservedTerms, 0);
  const formatting = selected.filter((row) => row.formatMatch !== null);
  return {
    count: selected.length,
    wer: selected.length ? selected.reduce((sum, row) => sum + row.wer, 0) / selected.length : null,
    p50: percentile(latencies, 0.5),
    p95: percentile(latencies, 0.95),
    termRecall: termCount ? preserved / termCount : null,
    formatAccuracy: formatting.length
      ? formatting.filter((row) => row.formatMatch).length / formatting.length
      : null,
  };
}

const percent = (value) => value === null ? "—" : `${(value * 100).toFixed(1)}%`;
const milliseconds = (value) => value === null ? "—" : `${Math.round(value)} ms`;

console.log("# Millow Dikte Değerlendirmesi\n");
console.log(`Kaynak: ${path.relative(root, resultsPath)} · ${rows.length} sonuç\n`);
console.log("| Mod | Örnek | Ortalama WER ↓ | Terim koruma ↑ | Biçim tam eşleşme ↑ | P50 gecikme ↓ | P95 gecikme ↓ |");
console.log("|---|---:|---:|---:|---:|---:|---:|");
for (const mode of modes) {
  const summary = summarize(mode);
  console.log(`| ${mode} | ${summary.count} | ${percent(summary.wer)} | ${percent(summary.termRecall)} | ${percent(summary.formatAccuracy)} | ${milliseconds(summary.p50)} | ${milliseconds(summary.p95)} |`);
}

const missing = [];
for (const mode of modes) {
  for (const testCase of corpus.cases) {
    if (!rows.some((row) => row.mode === mode && row.id === testCase.id)) {
      missing.push(`${mode}/${testCase.id}`);
    }
  }
}
if (missing.length) {
  console.log(`\nEksik koşum: ${missing.length}/120. İlk eksikler: ${missing.slice(0, 12).join(", ")}${missing.length > 12 ? "…" : ""}`);
}
