import { readFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import {
  TESTNET_HARNESS_CONFIRMATION,
  runTestnetConformanceHarness,
} from "./testnet-conformance-harness.js";
import {
  createCanonicalSupportedVerifyPort,
  createEphemeralTestnetCredentialPort,
} from "./testnet-live-conformance.js";
import { LocalTestnetStateAdapter } from "./testnet-state-adapter.js";

const SERVICE_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const FIXTURE_PATH = resolve(
  SERVICE_ROOT,
  "fixtures/testnet-harness-v2.expected.json",
);
const EXECUTE_ARGUMENT = "--execute-bounded-testnet";
const EXECUTE_ENVIRONMENT = "NC_X402_TESTNET_CONFIRM";

interface HarnessFixture {
  readonly request: Record<string, unknown>;
  readonly boundary: { readonly expectedPayTo: string };
}

async function main(): Promise<void> {
  const fixture = JSON.parse(await readFile(FIXTURE_PATH, "utf8")) as HarnessFixture;
  const execute =
    process.argv.slice(2).length === 1 &&
    process.argv[2] === EXECUTE_ARGUMENT &&
    process.env[EXECUTE_ENVIRONMENT] === TESTNET_HARNESS_CONFIRMATION;
  const request = execute
    ? {
        ...fixture.request,
        execute: true,
        confirmation: TESTNET_HARNESS_CONFIRMATION,
      }
    : fixture.request;
  let publicAccountId: string | null = null;
  const credentialPort = createEphemeralTestnetCredentialPort();
  const boundary = execute
    ? {
        expectedPayTo: fixture.boundary.expectedPayTo,
        statePort: new LocalTestnetStateAdapter({ workspaceRoot: SERVICE_ROOT }),
        credentialPort: {
          createEphemeral: async () => {
            const credential = await credentialPort.createEphemeral();
            publicAccountId = credential.publicAccountId;
            return credential;
          },
        },
        canonicalPort: createCanonicalSupportedVerifyPort(),
      }
    : { expectedPayTo: fixture.boundary.expectedPayTo };
  const result = await runTestnetConformanceHarness(request, boundary);
  const output = execute
    ? Object.freeze({
        schemaVersion: 1 as const,
        publicAccountId,
        result,
      })
    : result;
  process.stdout.write(`${JSON.stringify(output, null, 2)}\n`);
  if (execute && result.status !== "completed") {
    process.exitCode = 1;
  }
}

await main();
