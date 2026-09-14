// Exercise the installed app-server against a local fake provider. No paid inference.
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import http from "node:http";
import readline from "node:readline";

const received = [];
const server = http.createServer(async (req, res) => {
  let body = "";
  for await (const chunk of req) body += chunk;
  const data = JSON.parse(body);
  received.push({ model: data.model, effort: data.reasoning?.effort });
  res.writeHead(200, { "Content-Type": "text/event-stream" });
  res.end(
    "data: " +
      JSON.stringify({
        type: "response.completed",
        response: {
          id: "local-thinking-check",
          status: "completed",
          output: [],
          usage: { input_tokens: 1, output_tokens: 1, total_tokens: 2 },
        },
      }) +
      "\n\n",
  );
});
await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
const port = server.address().port;
const child = spawn(
  process.argv[2],
  [
    "app-server",
    "-c",
    `model_providers.ollama_cloud.base_url="http://127.0.0.1:${port}/v1"`,
  ],
  {
    env: { ...process.env, OLLAMA_API_KEY: "local-fake-provider-only" },
    stdio: ["pipe", "pipe", "ignore"],
  },
);
let id = 0;
const pending = new Map();
const completed = new Set();
readline.createInterface({ input: child.stdout }).on("line", (line) => {
  let msg;
  try {
    msg = JSON.parse(line);
  } catch {
    return;
  }
  if (msg.id != null && pending.has(msg.id)) {
    const { resolve, reject } = pending.get(msg.id);
    pending.delete(msg.id);
    msg.error
      ? reject(new Error(JSON.stringify(msg.error)))
      : resolve(msg.result);
  }
  if (msg.method === "turn/completed") completed.add(msg.params.threadId);
});
function rpc(method, params) {
  return new Promise((resolve, reject) => {
    const key = ++id;
    pending.set(key, { resolve, reject });
    child.stdin.write(JSON.stringify({ id: key, method, params }) + "\n");
  });
}
let expired = false;
const deadline = setTimeout(() => {
  expired = true;
  for (const { reject } of pending.values())
    reject(new Error("Isolated wire check timed out"));
  pending.clear();
  child.kill();
}, 45000);
try {
  await rpc("initialize", {
    clientInfo: { name: "thinking_wire_check", version: "1" },
    capabilities: { experimentalApi: true },
  });
  const list = await rpc("model/list", { limit: 100 });
  for (const model of [
    "deepseek-v4.1-flash:cloud",
    "glm-5.3-flash:cloud",
    "glm-5.3:cloud",
    "kimi-k3:cloud",
    "gemma4:31b-cloud",
  ]) {
    const efforts = model.startsWith("gemma")
      ? ["none", "high"]
      : ["low", "high", "max"];
    assert.deepEqual(
      list.data
        .find((x) => x.model === model)
        .supportedReasoningEfforts.map((x) => x.reasoningEffort),
      efforts,
    );
    for (const effort of efforts) {
      const thread = await rpc("thread/start", {
        model,
        ephemeral: true,
        cwd: process.cwd(),
      });
      assert.equal(thread.modelProvider, "ollama_cloud");
      const before = received.length;
      await rpc("turn/start", {
        threadId: thread.thread.id,
        effort,
        input: [{ type: "text", text: "Local wire check. No tools." }],
      });
      while (!completed.has(thread.thread.id)) {
        if (expired) throw new Error("Isolated wire check timed out");
        await new Promise((resolve) => setTimeout(resolve, 20));
      }
      assert.deepEqual(received.slice(before), [{ model, effort }]);
      console.log(JSON.stringify({ model, effort, wire: "PASS" }));
    }
  }
} finally {
  clearTimeout(deadline);
  child.kill();
  server.closeAllConnections();
  server.close();
}
