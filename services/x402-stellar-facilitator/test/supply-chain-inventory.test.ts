import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

interface InventoryPackage {
  readonly name: string;
  readonly version: string;
  readonly license: string;
  readonly scope: "runtime" | "development";
  readonly direct: "runtime" | "development" | false;
}

interface DependencyLicenseInventory {
  readonly schemaVersion: number;
  readonly packageManager: string;
  readonly packageCount: number;
  readonly policy: {
    readonly prohibitedLicenseFamilies: readonly string[];
    readonly unknownRuntimeLicenseAllowed: boolean;
    readonly installScriptsExecuted: boolean;
  };
  readonly directDependencies: {
    readonly dependencies: Readonly<Record<string, string>>;
    readonly devDependencies: Readonly<Record<string, string>>;
  };
  readonly packages: readonly InventoryPackage[];
}

test("committed dependency and license inventory keeps the approved closure", async () => {
  const inventoryUrl = new URL(
    "../../supply-chain/dependency-license-inventory.json",
    import.meta.url,
  );
  const inventory = JSON.parse(
    await readFile(inventoryUrl, "utf8"),
  ) as DependencyLicenseInventory;

  assert.equal(inventory.schemaVersion, 1);
  assert.equal(inventory.packageManager, "pnpm@11.19.0");
  assert.deepEqual(inventory.directDependencies.dependencies, {
    "@x402/core": "2.23.0",
    "@x402/stellar": "2.23.0",
  });
  assert.deepEqual(inventory.directDependencies.devDependencies, {
    "@types/node": "24.13.3",
    typescript: "5.9.3",
  });
  assert.deepEqual(inventory.policy.prohibitedLicenseFamilies, ["AGPL", "GPL", "LGPL", "SSPL"]);
  assert.equal(inventory.policy.unknownRuntimeLicenseAllowed, false);
  assert.equal(inventory.policy.installScriptsExecuted, false);
  assert.equal(inventory.packageCount, inventory.packages.length);

  const sortedPackages = [...inventory.packages].sort(
    (left, right) =>
      left.name.localeCompare(right.name) ||
      left.version.localeCompare(right.version) ||
      left.license.localeCompare(right.license),
  );
  assert.deepEqual(inventory.packages, sortedPackages);
  assert.equal(
    inventory.packages.some(({ license }) => /(?:AGPL|GPL|LGPL|SSPL)/i.test(license)),
    false,
  );
  assert.equal(
    inventory.packages.some(
      ({ scope, license }) =>
        scope === "runtime" && /^(UNKNOWN|UNLICENSED|NONE|NOASSERTION)$/i.test(license),
    ),
    false,
  );
});
