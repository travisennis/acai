#!/usr/bin/env node

// Fetch models from OpenRouter API
async function fetchOpenRouterModels() {
  try {
    console.info('Fetching models from OpenRouter API...');
    const response = await fetch('https://openrouter.ai/api/v1/models');
    
    if (!response.ok) {
      throw new Error(`HTTP error! status: ${response.status}`);
    }
    
    const data = await response.json();
    
    if (!data.data || !Array.isArray(data.data)) {
      throw new Error('Invalid response format: expected data array');
    }
    
    return data.data;
  } catch (error) {
    console.error('Error fetching models:', error.message);
    process.exit(1);
  }
}

// Extract model key from ID (e.g., "google/gemini-3-flash-preview" -> "gemini-3-flash-preview")
function extractModelKey(modelId) {
  const parts = modelId.split('/');
  return parts[parts.length - 1].replace(/[^a-z0-9-]/gi, '-');
}

// Infer prompt format from model ID
function inferPromptFormat(modelId) {
  const id = modelId.toLowerCase();
  if (id.includes('gemini')) return 'markdown';
  if (id.includes('gpt') || id.includes('openai')) return 'xml';
  if (id.includes('claude')) return 'markdown';
  if (id.includes('qwen')) return 'markdown';
  if (id.includes('deepseek')) return 'bracket';
  if (id.includes('mistral')) return 'markdown';
  if (id.includes('moonshotai') || id.includes('kimi')) return 'markdown';
  return 'markdown'; // default
}

// Infer default temperature from model ID
function inferDefaultTemperature(modelId) {
  const id = modelId.toLowerCase();
  if (id.includes('codex') || id.includes('coder') || id.includes('code')) return -1;
  return 0.5;
}

// Check if model supports reasoning
function supportsReasoning(model) {
  return model.supported_parameters?.includes('include_reasoning') || 
         model.supported_parameters?.includes('reasoning') ||
         false;
}

// Check if model supports tool calling
function supportsToolCalling(model) {
  return model.supported_parameters?.includes('tools') || false;
}

// Generate complete model details for the agent
function generateModelDetails(model) {
  const modelKey = extractModelKey(model.id);
  const contextWindow = model.context_length || 0;
  const maxOutputTokens = model.top_provider?.max_completion_tokens || contextWindow;
  const promptCost = parseFloat(model.pricing?.prompt || '0');
  const completionCost = parseFloat(model.pricing?.completion || '0');
  
  return {
    modelKey,
    openrouterId: model.id,
    providerFile: './source/models/openrouter-provider.ts',
    registryId: `openrouter:${modelKey}`,
    registryEntry: {
      id: `openrouter:${modelKey}`,
      provider: 'openrouter',
      contextWindow,
      maxOutputTokens,
      defaultTemperature: inferDefaultTemperature(model.id),
      promptFormat: inferPromptFormat(model.id),
      supportsReasoning: supportsReasoning(model),
      supportsToolCalling: supportsToolCalling(model),
      costPerInputToken: promptCost,
      costPerOutputToken: completionCost,
    },
    clientConfig: {
      key: modelKey,
      openrouterId: model.id,
    },
    modelInfo: {
      id: model.id,
      name: model.name || 'Unnamed',
      contextLength: model.context_length,
      maxCompletionTokens: model.top_provider?.max_completion_tokens,
      pricing: model.pricing,
      supportedParameters: model.supported_parameters,
    },
  };
}

// Show help
function showHelp() {
  console.info(`
Add OpenRouter Model - Skill for acai project

Usage:
  node add-model.js <model-id>

Arguments:
  <model-id>    OpenRouter model ID (e.g., google/gemini-3-flash-preview)

Examples:
  node add-model.js google/gemini-3-flash-preview
  node add-model.js anthropic/claude-3-5-sonnet
  `);
}

// Main function
async function main() {
  const args = process.argv.slice(2);
  
  if (args.length === 0 || args[0] === '--help' || args[0] === '-h') {
    showHelp();
    return;
  }
  
  const modelId = args[0];
  
  // Fetch all models
  const models = await fetchOpenRouterModels();
  
  // Find model by exact ID
  const model = models.find(m => m.id === modelId);
  
  if (!model) {
    console.error(`Model "${modelId}" not found.`);
    console.info('Available models (first 10):');
    models.slice(0, 10).forEach(m => {
      console.info(`  - ${m.id}`);
    });
    if (models.length > 10) {
      console.info(`  ... and ${models.length - 10} more`);
    }
    process.exit(1);
  }
  
  // Generate and output model details as JSON
  const modelDetails = generateModelDetails(model);
  
  console.info('\n--- MODEL_DETAILS_JSON_START ---');
  console.info(JSON.stringify(modelDetails, null, 2));
  console.info('--- MODEL_DETAILS_JSON_END ---');
}

// Run main
main().catch(error => {
  console.error('Fatal error:', error);
  process.exit(1);
});
