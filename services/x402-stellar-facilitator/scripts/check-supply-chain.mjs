import { createHash } from "node:crypto";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

const ROOT = dirname(dirname(fileURLToPath(import.meta.url)));
const PACKAGE_PATH = join(ROOT, "package.json");
const LOCKFILE_PATH = join(ROOT, "pnpm-lock.yaml");
const INVENTORY_PATH = join(ROOT, "supply-chain", "dependency-license-inventory.json");

const EXPECTED = Object.freeze({
  packageManager: "pnpm@11.19.0",
  dependencies: Object.freeze({
    "@x402/core": "2.23.0",
    "@x402/stellar": "2.23.0",
  }),
  devDependencies: Object.freeze({
    "@types/node": "24.13.3",
    typescript: "5.9.3",
  }),
});

function fail(code, reason) {
  throw new Error(`${code}:${reason}`);
}

function stableObject(value) {
  return Object.fromEntries(Object.entries(value).sort(([left], [right]) => left.localeCompare(right)));
}

function assertExactMap(actual, expected, label) {
  const normalizedActual = stableObject(actual ?? {});
  const normalizedExpected = stableObject(expected);
  if (JSON.stringify(normalizedActual) !== JSON.stringify(normalizedExpected)) {
    fail("direct_dependency_drift", `${label} must equal the approved exact version map`);
  }
}

function runPnpm(args) {
  const pnpmScript = process.env.npm_execpath;
  if (!pnpmScript) {
    fail("pnpm_runtime_unavailable", "run this check through pnpm run");
  }
  const result = spawnSync(process.execPath, [pnpmScript, ...args], {
    cwd: ROOT,
    encoding: "utf8",
    env: { ...process.env, CI: "true", npm_config_offline: "true" },
    windowsHide: true,
  });
  if (result.status !== 0) {
    fail("pnpm_inventory_failed", `${args.join(" ")}: ${result.stderr.trim()}`);
  }
  try {
    return JSON.parse(result.stdout);
  } catch (error) {
    fail("pnpm_inventory_invalid_json", `${args.join(" ")}: ${error.message}`);
  }
}

function flattenLicenses(groups) {
  const packages = new Map();
  for (const [groupLicense, entries] of Object.entries(groups)) {
    if (!Array.isArray(entries)) {
      fail("license_inventory_invalid", `license group ${groupLicense} is not an array`);
    }
    for (const entry of entries) {
      const license = String(entry.license ?? groupLicense).trim();
      for (const version of entry.versions ?? []) {
        const key = `${entry.name}@${version}`;
        const prior = packages.get(key);
        if (prior && prior.license !== license) {
          fail("license_inventory_conflict", `${key} has conflicting licenses`);
        }
        packages.set(key, { name: entry.name, version: String(version), license });
      }
    }
  }
  return packages;
}

function isUnknownLicense(license) {
  return license.length === 0 || /^(UNKNOWN|UNLICENSED|NONE|NOASSERTION)$/i.test(license);
}

function isProhibitedLicense(license) {
  return /(?:^|[^A-Z])(?:AGPL|GPL|LGPL|SSPL)(?:[^A-Z]|$)/i.test(license);
}

function resolvedDirectVersions(rootList) {
  const root = rootList[0];
  if (!root) {
    fail("dependency_tree_unavailable", "pnpm list returned no workspace root");
  }
  return {
    dependencies: Object.fromEntries(
      Object.entries(root.dependencies ?? {}).map(([name, value]) => [name, value.version]),
    ),
    devDependencies: Object.fromEntries(
      Object.entries(root.devDependencies ?? {}).map(([name, value]) => [name, value.version]),
    ),
  };
}

function buildInventory() {
  const manifest = JSON.parse(readFileSync(PACKAGE_PATH, "utf8"));
  if (manifest.packageManager !== EXPECTED.packageManager) {
    fail("package_manager_drift", `expected ${EXPECTED.packageManager}`);
  }
  assertExactMap(manifest.dependencies, EXPECTED.dependencies, "dependencies");
  assertExactMap(manifest.devDependencies, EXPECTED.devDependencies, "devDependencies");

  if (!existsSync(LOCKFILE_PATH)) {
    fail("lockfile_missing", "pnpm-lock.yaml is required");
  }
  const lockfile = readFileSync(LOCKFILE_PATH);
  const resolved = resolvedDirectVersions(runPnpm(["list", "--depth", "0", "--json"]));
  assertExactMap(resolved.dependencies, EXPECTED.dependencies, "resolved dependencies");
  assertExactMap(resolved.devDependencies, EXPECTED.devDependencies, "resolved devDependencies");

  const allPackages = flattenLicenses(runPnpm(["licenses", "list", "--json"]));
  const runtimePackages = flattenLicenses(runPnpm(["licenses", "list", "--prod", "--json"]));
  const directRuntime = new Set(Object.keys(EXPECTED.dependencies));
  const directDevelopment = new Set(Object.keys(EXPECTED.devDependencies));

  const packages = [...allPackages.entries()]
    .map(([key, value]) => ({
      ...value,
      scope: runtimePackages.has(key) ? "runtime" : "development",
      direct: directRuntime.has(value.name)
        ? "runtime"
        : directDevelopment.has(value.name)
          ? "development"
          : false,
    }))
    .sort((left, right) =>
      left.name.localeCompare(right.name) ||
      left.version.localeCompare(right.version) ||
      left.license.localeCompare(right.license),
    );

  for (const dependency of packages) {
    if (isProhibitedLicense(dependency.license)) {
      fail("prohibited_license", `${dependency.name}@${dependency.version}:${dependency.license}`);
    }
    if (dependency.scope === "runtime" && isUnknownLicense(dependency.license)) {
      fail("unknown_runtime_license", `${dependency.name}@${dependency.version}`);
    }
  }

  const licenseCounts = Object.fromEntries(
    [...new Set(packages.map(({ license }) => license))]
      .sort()
      .map((license) => [license, packages.filter((dependency) => dependency.license === license).length]),
  );

  return {
    schemaVersion: 1,
    source: "pnpm licenses list --json",
    packageManager: EXPECTED.packageManager,
    lockfileSha256: createHash("sha256").update(lockfile).digest("hex"),
    policy: {
      prohibitedLicenseFamilies: ["AGPL", "GPL", "LGPL", "SSPL"],
      unknownRuntimeLicenseAllowed: false,
      installScriptsExecuted: false,
    },
    directDependencies: {
      dependencies: stableObject(EXPECTED.dependencies),
      devDependencies: stableObject(EXPECTED.devDependencies),
    },
    packageCount: packages.length,
    licenseCounts,
    packages,
  };
}

function serialize(value) {
  return `${JSON.stringify(value, null, 2)}\n`;
}

const mode = process.argv[2];
if (mode !== "--write" && mode !== "--check") {
  fail("invalid_mode", "expected --write or --check");
}

const inventory = buildInventory();
const rendered = serialize(inventory);
if (mode === "--write") {
  mkdirSync(dirname(INVENTORY_PATH), { recursive: true });
  writeFileSync(INVENTORY_PATH, rendered, "utf8");
} else {
  if (!existsSync(INVENTORY_PATH)) {
    fail("inventory_missing", "run pnpm run update:supply-chain");
  }
  if (readFileSync(INVENTORY_PATH, "utf8") !== rendered) {
    fail("inventory_drift", "run pnpm run update:supply-chain and review the diff");
  }
}

console.log(
  JSON.stringify({
    status: "supply_chain_verified",
    mode,
    packageCount: inventory.packageCount,
    licenseCounts: inventory.licenseCounts,
    networkAccessAllowed: false,
    installScriptsExecuted: false,
  }),
);
