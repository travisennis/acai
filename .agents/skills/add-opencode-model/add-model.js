#!/usr/bin/env node

// Fetch models from OpenCode API to confirm model exists
async function fetchOpenCodeModels() {
  try {
    console.info('Fetching models from OpenCode API...');
    const response = await fetch('https://opencode.ai/zen/v1/models');
    
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

// Fetch models from OpenRouter API for detailed information
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
    console.error('Error fetching OpenRouter models:', error.message);
    process.exit(1);
  }
}

// Search OpenRouter models to find a match for the OpenCode model
function searchOpenRouterModels(openrouterModels, searchTerm) {
  const term = searchTerm.toLowerCase();
  
  // Try different matching strategies
  // 1. Exact match on the model key (after the slash)
  const directMatch = openrouterModels.find(m => {
    const modelKey = m.id.split('/').pop();
    return modelKey === searchTerm || modelKey === term;
  });
  
  if (directMatch) return directMatch;
  
  // 2. Match by provider and model name
  const provider = searchTerm.split('/')[0];
  const modelName = searchTerm.split('/')[1];
  
  if (modelName) {
    const providerMatch = openrouterModels.find(m => {
      const mProvider = m.id.split('/')[0];
      const mName = m.id.split('/').pop();
      return mProvider === provider && mName?.toLowerCase().includes(modelName.toLowerCase());
    });
    
    if (providerMatch) return providerMatch;
  }
  
  // 3. Partial match on model name
  const partialMatch = openrouterModels.find(m => {
    const modelKey = m.id.split('/').pop()?.toLowerCase();
    return modelKey?.includes(term);
  });
  
  if (partialMatch) return partialMatch;
  
  return null;
}

// Determine client type based on model provider
function determineClientType(modelId) {
  const provider = modelId.split('/')[0].toLowerCase();
  
  if (provider === 'openai') {
    return '@ai-sdk/openai';
  }
  
  if (provider === 'google') {
    return '@ai-sdk/google';
  }
  
  if (provider === 'anthropic') {
    return '@ai-sdk/anthropic';
  }
  
  return '@ai-sdk/openai-compatible';
}

// Extract model key from ID
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
  if (!model) return false;
  return model.supported_parameters?.includes('include_reasoning') || 
         model.supported_parameters?.includes('reasoning') ||
         false;
}

// Check if model supports tool calling
function supportsToolCalling(model) {
  if (!model) return false;
  return model.supported_parameters?.includes('tools') || false;
}

// Generate complete model details for the agent
function generateModelDetails(opencodeModel, openrouterModel, clientType) {
  const modelKey = extractModelKey(opencodeModel.id);
  const contextWindow = openrouterModel?.context_length || 0;
  const maxOutputTokens = openrouterModel?.top_provider?.max_completion_tokens || contextWindow;
  const promptCost = parseFloat(openrouterModel?.pricing?.prompt || '0');
  const completionCost = parseFloat(openrouterModel?.pricing?.completion || '0');
  
  return {
    modelKey,
    opencodeId: opencodeModel.id,
    openrouterId: openrouterModel?.id || null,
    providerFile: './source/models/opencode-zen-provider.ts',
    registryId: `opencode:${modelKey}`,
    clientType,
    registryEntry: {
      id: `opencode:${modelKey}`,
      provider: 'opencode',
      contextWindow,
      maxOutputTokens,
      defaultTemperature: inferDefaultTemperature(opencodeModel.id),
      promptFormat: inferPromptFormat(opencodeModel.id),
      supportsReasoning: supportsReasoning(openrouterModel),
      supportsToolCalling: supportsToolCalling(openrouterModel),
      costPerInputToken: promptCost,
      costPerOutputToken: completionCost,
    },
    clientConfig: {
      key: modelKey,
      opencodeId: opencodeModel.id,
      clientType,
      // OpenRouter ID used for matching (may be different from OpenCode ID)
      openrouterId: openrouterModel?.id || null,
    },
    modelInfo: {
      opencodeId: opencodeModel.id,
      opencodeName: opencodeModel.name || 'Unnamed',
      openrouterId: openrouterModel?.id || null,
      openrouterName: openrouterModel?.name || null,
      contextLength: openrouterModel?.context_length,
      maxCompletionTokens: openrouterModel?.top_provider?.max_completion_tokens,
      pricing: openrouterModel?.pricing,
      supportedParameters: openrouterModel?.supported_parameters,
    },
  };
}

// Show help
function showHelp() {
  console.info(`
Add OpenCode Model - Skill for acai project

Usage:
  node add-model.js <model-id>

Arguments:
  <model-id>    OpenCode model ID (e.g., google/gemini-3-flash-preview)

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
  
  const opencodeModelId = args[0];
  
  // Fetch models from both APIs
  const [opencodeModels, openrouterModels] = await Promise.all([
    fetchOpenCodeModels(),
    fetchOpenRouterModels(),
  ]);
  
  // Verify model exists in OpenCode
  const opencodeModel = opencodeModels.find(m => m.id === opencodeModelId);
  
  if (!opencodeModel) {
    console.error(`Model "${opencodeModelId}" not found in OpenCode.`);
    console.info('Available OpenCode models (first 10):');
    opencodeModels.slice(0, 10).forEach(m => {
      console.info(`  - ${m.id}`);
    });
    if (opencodeModels.length > 10) {
      console.info(`  ... and ${opencodeModels.length - 10} more`);
    }
    process.exit(1);
  }
  
  console.info(`\n✓ Found model in OpenCode: ${opencodeModel.id}`);
  
  // Search OpenRouter for matching model to get details
  // Try different matching strategies
  let openrouterModel = searchOpenRouterModels(openrouterModels, opencodeModelId);
  
  if (!openrouterModel) {
    // Try matching with just the model name (without provider prefix)
    const modelName = opencodeModelId.split('/').pop();
    if (modelName) {
      openrouterModel = searchOpenRouterModels(openrouterModels, modelName);
    }
  }
  
  if (openrouterModel) {
    console.info(`✓ Found matching model in OpenRouter: ${openrouterModel.id}`);
  } else {
    console.info('⚠ No matching model found in OpenRouter - using defaults');
  }
  
  // Determine client type
  const clientType = determineClientType(openrouterModel.id);
  console.info(`✓ Client type: ${clientType}`);
  
  // Generate and output model details as JSON
  const modelDetails = generateModelDetails(opencodeModel, openrouterModel, clientType);
  
  console.info('\n--- MODEL_DETAILS_JSON_START ---');
  console.info(JSON.stringify(modelDetails, null, 2));
  console.info('--- MODEL_DETAILS_JSON_END ---');
}

// Run main
main().catch(error => {
  console.error('Fatal error:', error);
  process.exit(1);
});
