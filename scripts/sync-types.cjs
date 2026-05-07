#!/usr/bin/env node
/**
 * Type sync check script
 * Compares Rust models (models.rs) with TypeScript interfaces (git.ts)
 * Ensures field names match between the two representations.
 */

const fs = require('fs');
const path = require('path');

const RUST_MODELS_PATH = path.join(__dirname, '..', 'src-tauri', 'src', 'models.rs');
const TS_INTERFACES_PATH = path.join(__dirname, '..', 'src', 'services', 'git.ts');

/**
 * Extract struct fields from Rust models.rs
 * Matches patterns like: pub field_name: Type,
 */
function extractRustStructs(content) {
  const structs = {};

  // Match structs: pub struct Name { ... }
  const structRegex = /pub struct (\w+)\s*\{([^}]*)\}/g;
  let match;

  while ((match = structRegex.exec(content)) !== null) {
    const structName = match[1];
    const body = match[2];

    // Extract field names from pub field_name: Type patterns
    const fieldRegex = /pub\s+(\w+)\s*:/g;
    const fields = [];
    let fieldMatch;

    while ((fieldMatch = fieldRegex.exec(body)) !== null) {
      fields.push(fieldMatch[1]);
    }

    structs[structName] = fields;
  }

  return structs;
}

/**
 * Extract interface fields from TypeScript git.ts
 * Matches patterns like: fieldName: type;
 */
function extractTSInterfaces(content) {
  const interfaces = {};

  // Match interfaces: export interface Name { ... }
  const interfaceRegex = /export interface (\w+)\s*\{([^}]*)\}/g;
  let match;

  while ((match = interfaceRegex.exec(content)) !== null) {
    const interfaceName = match[1];
    const body = match[2];

    // Extract field names from fieldName: type patterns (ignore comments)
    const fieldRegex = /^\s*(\w+)\s*:/gm;
    const fields = [];
    let fieldMatch;

    while ((fieldMatch = fieldRegex.exec(body)) !== null) {
      fields.push(fieldMatch[1]);
    }

    interfaces[interfaceName] = fields;
  }

  return interfaces;
}

/**
 * Compare two structs/interfaces for field name mismatches
 */
function compareStructs(rustName, rustFields, tsName, tsFields) {
  const mismatches = [];

  const rustSet = new Set(rustFields);
  const tsSet = new Set(tsFields);

  // Check fields in Rust but not in TS
  for (const field of rustFields) {
    if (!tsSet.has(field)) {
      mismatches.push({ direction: 'rust→ts', field, missing: 'ts' });
    }
  }

  // Check fields in TS but not in Rust
  for (const field of tsFields) {
    if (!rustSet.has(field)) {
      mismatches.push({ direction: 'ts→rust', field, missing: 'rust' });
    }
  }

  return mismatches;
}

// Main execution
console.log('Checking types...\n');

const rustContent = fs.readFileSync(RUST_MODELS_PATH, 'utf8');
const tsContent = fs.readFileSync(TS_INTERFACES_PATH, 'utf8');

const rustStructs = extractRustStructs(rustContent);
const tsInterfaces = extractTSInterfaces(tsContent);

// Types to check (skip internal/options types that don't map)
const skipTypes = ['CacheEntry', 'DebouncedEntry'];

// Track results
let totalMismatches = 0;
const results = [];

for (const [structName, rustFields] of Object.entries(rustStructs)) {
  if (skipTypes.includes(structName)) continue;

  const tsName = structName; // Names should match

  if (!tsInterfaces[tsName]) {
    console.log(`⚠ ${structName}: TS interface not found`);
    continue;
  }

  const tsFields = tsInterfaces[tsName];
  const mismatches = compareStructs(structName, rustFields, tsName, tsFields);

  if (mismatches.length === 0) {
    console.log(`✓ ${structName}: OK (${rustFields.length} fields)`);
    results.push({ name: structName, ok: true, fields: rustFields.length });
  } else {
    console.log(`✗ ${structName}: MISMATCH`);
    for (const m of mismatches) {
      console.log(`  - Field "${m.field}" in ${m.missing} but not in other`);
    }
    results.push({ name: structName, ok: false, mismatches });
    totalMismatches += mismatches.length;
  }
}

// Check for TS interfaces without matching Rust structs
for (const [interfaceName, tsFields] of Object.entries(tsInterfaces)) {
  if (skipTypes.includes(interfaceName)) continue;
  if (!rustStructs[interfaceName]) {
    console.log(`⚠ ${interfaceName}: Rust struct not found (TS has ${tsFields.length} fields)`);
  }
}

console.log('\n--- Summary ---');

if (totalMismatches === 0) {
  console.log('✓ All types match!');
  process.exit(0);
} else {
  console.log(`✗ FAILED: ${totalMismatches} type mismatch(es) found`);
  process.exit(1);
}