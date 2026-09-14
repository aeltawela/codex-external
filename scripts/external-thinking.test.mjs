import assert from "node:assert/strict";
import test from "node:test";
import { updateThinkingCatalog } from "./external-thinking.mjs";

test("updates only qualified external entries without changing models or defaults", () => {
  const original = {
    models: [
      {
        slug: "glm-5.3-flash:cloud",
        default_reasoning_level: "high",
        supported_reasoning_levels: [{ effort: "medium" }],
        context_window: 384000,
      },
      {
        slug: "gpt-5.6-sol",
        supported_reasoning_levels: [{ effort: "ultra" }],
      },
      {
        slug: "unrelated-provider-model",
        supported_reasoning_levels: [{ effort: "low" }],
      },
    ],
  };
  const snapshot = structuredClone(original);
  const result = updateThinkingCatalog(original);
  assert.deepEqual(result.models[0], {
    ...original.models[0],
    supported_reasoning_levels: ["low", "high", "max"].map((effort) => ({
      effort,
      description: `${effort}: model-documented level; Ollama transport compatibility tested, relative reasoning quality not measured`,
    })),
  });
  assert.deepEqual(result.models.slice(1), original.models.slice(1));
  assert.deepEqual(original, snapshot);
  assert.deepEqual(updateThinkingCatalog(result), result);
});

test("Gemma exposes off/on without inventing three model-native thinking tiers", () => {
  const result = updateThinkingCatalog({
    models: [{ slug: "gemma4:31b-cloud", default_reasoning_level: "low" }],
  });
  assert.deepEqual(result.models[0], {
    slug: "gemma4:31b-cloud",
    default_reasoning_level: "high",
    supported_reasoning_levels: [
      {
        effort: "none",
        description: "Thinking off (Ollama reasoning.effort=none)",
      },
      {
        effort: "high",
        description:
          "Thinking on (Ollama reasoning.effort=high); not a separate native High tier",
      },
    ],
  });
});
