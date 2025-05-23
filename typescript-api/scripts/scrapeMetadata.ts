import fs from "node:fs";
import {
  type ChildProcessWithoutNullStreams,
  type SpawnOptionsWithoutStdio,
  execSync,
  spawn
} from "node:child_process";
import path from "node:path";

const CHAINS = ["moonbase", "moonriver", "moonbeam"];

function wrapSpawnWithLogging(
  command: string,
  args?: readonly string[],
  options?: SpawnOptionsWithoutStdio
): ChildProcessWithoutNullStreams {
  const process = spawn(command, args, options);

  process.stdout.on("data", (data) => {
    console.log(`stdout: ${data}`);
  });
  process.stderr.on("data", (data) => {
    console.error(`stderr: ${data}`);
  });
  process.on("close", (code) => {
    console.log(`child process exited with code ${code}`);
  });

  return process;
}

const fetchMetadata = async (port = 9933) => {
  const maxRetries = 60;
  const sleepTime = 500;
  const url = `http://127.0.0.1:${port}`;
  const payload = {
    id: "1",
    jsonrpc: "2.0",
    method: "state_getMetadata",
    params: []
  };

  for (let i = 0; i < maxRetries; i++) {
    try {
      const response = await fetch(url, {
        method: "POST",
        headers: {
          "Content-Type": "application/json"
        },
        body: JSON.stringify(payload)
      });

      if (!response.ok) {
        throw new Error(`HTTP error! status: ${response.status}`);
      }

      const data = await response.json();
      return data;
    } catch {
      console.log("Waiting for node to launch...");
      await new Promise((resolve) => setTimeout(resolve, sleepTime));
    }
  }
  console.log(`Error fetching metadata after ${(maxRetries * sleepTime) / 1000} seconds`);
  throw new Error("Error fetching metadata");
};

const nodes: { [key: string]: ChildProcessWithoutNullStreams } = {};

async function main() {
  const runtimeChainSpec = process.argv[2];
  const nodePath = path.join(process.cwd(), "..", "target", "release", "moonbeam");

  if (runtimeChainSpec) {
    console.log(`Bump package version to 0.${runtimeChainSpec}.0`);
    execSync(`npm version --no-git-tag-version 0.${runtimeChainSpec}.0`);
  }

  if (!fs.existsSync(nodePath)) {
    console.error("Moonbeam Node not found at path: ", nodePath);
    throw new Error("File not found");
  }

  for (const chain of CHAINS) {
    console.log(`Starting ${chain} node`);
    nodes[chain] = wrapSpawnWithLogging(nodePath, [
      "--no-hardware-benchmarks",
      "--unsafe-force-node-key-generation",
      "--no-telemetry",
      "--no-prometheus",
      "--alice",
      "--tmp",
      `--chain=${chain}-dev`,
      "--wasm-execution=interpreted-i-know-what-i-do",
      "--rpc-port=9933"
    ]);

    console.log(`Getting ${chain} metadata`);
    try {
      const metadata = await fetchMetadata();
      fs.writeFileSync(`metadata-${chain}.json`, JSON.stringify(metadata, null, 2));
      console.log(`✅ Metadata for ${chain} written to metadata-${chain}.json`);
      nodes[chain]?.kill();
      await new Promise((resolve) => setTimeout(resolve, 2000));
    } catch (error) {
      console.error(`❌ Error getting metadata for ${chain}`);
      throw error;
    }
  }
}

process.on("SIGINT", () => {
  for (const chain of CHAINS) {
    nodes[chain]?.kill();
  }
  process.exit();
});

main()
  .catch((error) => {
    console.error(error);
    process.exitCode = 1;
  })
  .finally(() => {
    for (const chain of CHAINS) {
      nodes[chain]?.kill();
    }
  });
