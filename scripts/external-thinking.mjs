export function updateThinkingCatalog(catalog) {
  const result = structuredClone(catalog);
  const tiered = new Set([
    "deepseek-v4.1-flash:cloud",
    "glm-5.3-flash:cloud",
    "glm-5.3:cloud",
    "kimi-k3:cloud",
  ]);
  for (const model of result.models) {
    if (model.slug === "gemma4:31b-cloud") {
      model.default_reasoning_level = "high";
      model.supported_reasoning_levels = [
        {
          effort: "none",
          description: "Thinking off (Ollama reasoning.effort=none)",
        },
        {
          effort: "high",
          description:
            "Thinking on (Ollama reasoning.effort=high); not a separate native High tier",
        },
      ];
    }
    if (!tiered.has(model.slug)) continue;
    model.supported_reasoning_levels = ["low", "high", "max"].map((effort) => ({
      effort,
      description: `${effort}: model-documented level; Ollama transport compatibility tested, relative reasoning quality not measured`,
    }));
  }
  return result;
}
